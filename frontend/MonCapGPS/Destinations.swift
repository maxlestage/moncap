import Foundation
import SQLite3

/// Toutes les destinations choisies, gardées sur l'appareil dans SQLite.
///
/// Les réglages (`UserDefaults`) n'en gardaient que douze, et la liste du
/// serveur la remplaçait à chaque chargement. Or le serveur tient désormais
/// ses données dans un fichier SQLite qui peut repartir de zéro (un disque
/// éphémère, un déménagement) : c'est l'appareil qui fait foi. Il garde
/// **toutes** les destinations, une ligne par lieu — y retourner la remonte en
/// tête et compte une visite de plus — et la liste du serveur s'y fusionne
/// sans jamais rien effacer.
///
/// La clé d'un lieu est celle du serveur (`trajets::recent_key`) : nom en
/// minuscules aux espaces réduits, et coordonnées à une dizaine de mètres.
final class Destinations: @unchecked Sendable {
    static let shared = Destinations()

    /// Une seule file pour la connexion : SQLite n'aime pas être pris par
    /// deux fils à la fois sur la même poignée.
    private let file = DispatchQueue(label: "moncap.destinations")
    private var db: OpaquePointer?

    /// Copie le texte lié : la chaîne Swift ne vit pas jusqu'à l'exécution.
    private static let transitoire = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

    private init() {
        let dossier = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask)[0]
        try? FileManager.default.createDirectory(
            at: dossier, withIntermediateDirectories: true)
        let chemin = dossier.appendingPathComponent("destinations.sqlite").path
        guard sqlite3_open_v2(
            chemin, &db, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE, nil) == SQLITE_OK
        else {
            db = nil
            return
        }
        execute("PRAGMA journal_mode = WAL")
        execute("""
            CREATE TABLE IF NOT EXISTS destinations (
                cle TEXT PRIMARY KEY, nom TEXT NOT NULL,
                sous_titre TEXT NOT NULL DEFAULT '', lat REAL NOT NULL, lon REAL NOT NULL,
                server_id INTEGER, fois INTEGER NOT NULL DEFAULT 1, rang INTEGER NOT NULL)
            """)
        execute("CREATE INDEX IF NOT EXISTS destinations_rang ON destinations (rang)")
    }

    /// La clé d'un lieu, la même que sur le serveur.
    static func cle(nom: String, lat: Double, lon: Double) -> String {
        let norme = nom.split(whereSeparator: \.isWhitespace).joined(separator: " ").lowercased()
        return "\(norme)|\(String(format: "%.4f", lat)),\(String(format: "%.4f", lon))"
    }

    // MARK: - Lecture

    /// Toutes les destinations, les plus récentes d'abord.
    func toutes() -> [RecentSearch] {
        file.sync {
            var liste: [RecentSearch] = []
            requete(
                "SELECT nom, sous_titre, lat, lon, server_id FROM destinations ORDER BY rang DESC"
            ) { st in
                while sqlite3_step(st) == SQLITE_ROW {
                    liste.append(RecentSearch(
                        serverId: sqlite3_column_type(st, 4) == SQLITE_NULL
                            ? nil : Int(sqlite3_column_int64(st, 4)),
                        name: Self.texte(st, 0),
                        subtitle: Self.texte(st, 1),
                        lat: sqlite3_column_double(st, 2),
                        lon: sqlite3_column_double(st, 3)))
                }
            }
            return liste
        }
    }

    // MARK: - Écriture

    /// Enregistre une destination : nouvelle, elle s'ajoute en tête ; déjà
    /// connue, elle y remonte avec une visite de plus.
    func enregistre(_ r: RecentSearch) {
        file.sync {
            requete("""
                INSERT INTO destinations (cle, nom, sous_titre, lat, lon, server_id, fois, rang)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1,
                    (SELECT coalesce(max(rang), 0) + 1 FROM destinations))
                ON CONFLICT (cle) DO UPDATE SET nom = excluded.nom,
                    sous_titre = excluded.sous_titre, lat = excluded.lat, lon = excluded.lon,
                    server_id = coalesce(excluded.server_id, server_id),
                    fois = fois + 1, rang = excluded.rang
                """) { st in
                lie(st, r)
                sqlite3_step(st)
            }
        }
    }

    /// Fusionne la liste du serveur (les plus récentes d'abord) : un lieu
    /// connu y gagne son identifiant serveur, un lieu inconnu (choisi sur un
    /// autre appareil du même compte) s'ajoute. Rien n'est jamais effacé : un
    /// serveur reparti de zéro ne vide pas l'historique de l'appareil.
    ///
    /// Les identifiants serveur repartent tous de cette liste : un serveur
    /// neuf renumérote, et un ancien identifiant désignerait un autre lieu.
    func fusionne(_ distantes: [RecentSearch]) {
        file.sync {
            execute("BEGIN")
            execute("UPDATE destinations SET server_id = NULL")
            // Des plus anciennes aux plus récentes : l'ordre relatif tient.
            for r in distantes.reversed() {
                requete("""
                    INSERT INTO destinations (cle, nom, sous_titre, lat, lon, server_id, fois, rang)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1,
                        (SELECT coalesce(max(rang), 0) + 1 FROM destinations))
                    ON CONFLICT (cle) DO UPDATE SET server_id = excluded.server_id
                    """) { st in
                    lie(st, r)
                    sqlite3_step(st)
                }
            }
            execute("COMMIT")
        }
    }

    /// Oublie une destination.
    func supprime(_ r: RecentSearch) {
        file.sync {
            requete("DELETE FROM destinations WHERE cle = ?1") { st in
                sqlite3_bind_text(
                    st, 1, Self.cle(nom: r.name, lat: r.lat, lon: r.lon), -1,
                    Self.transitoire)
                sqlite3_step(st)
            }
        }
    }

    /// Oublie toutes les destinations.
    func efface() {
        file.sync { execute("DELETE FROM destinations") }
    }

    // MARK: - SQLite

    private func execute(_ sql: String) {
        guard let db else { return }
        sqlite3_exec(db, sql, nil, nil, nil)
    }

    /// Prépare `sql`, le confie à `corps`, puis le libère.
    private func requete(_ sql: String, _ corps: (OpaquePointer) -> Void) {
        guard let db else { return }
        var st: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &st, nil) == SQLITE_OK, let st else { return }
        defer { sqlite3_finalize(st) }
        corps(st)
    }

    /// Lie les six paramètres d'une destination.
    private func lie(_ st: OpaquePointer, _ r: RecentSearch) {
        sqlite3_bind_text(
            st, 1, Self.cle(nom: r.name, lat: r.lat, lon: r.lon), -1, Self.transitoire)
        sqlite3_bind_text(st, 2, r.name, -1, Self.transitoire)
        sqlite3_bind_text(st, 3, r.subtitle, -1, Self.transitoire)
        sqlite3_bind_double(st, 4, r.lat)
        sqlite3_bind_double(st, 5, r.lon)
        if let id = r.serverId {
            sqlite3_bind_int64(st, 6, Int64(id))
        } else {
            sqlite3_bind_null(st, 6)
        }
    }

    private static func texte(_ st: OpaquePointer, _ colonne: Int32) -> String {
        guard let c = sqlite3_column_text(st, colonne) else { return "" }
        return String(cString: c)
    }
}
