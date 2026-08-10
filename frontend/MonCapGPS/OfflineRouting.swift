import CoreLocation
import Foundation
import Valhalla

/// Calcul d'itinéraire **sur l'appareil**, sans réseau.
///
/// Apple exige une connexion pour `MKDirections` : rater une sortie hors
/// couverture laissait l'app sans rien à proposer. Valhalla calcule le même
/// genre d'itinéraire localement, à condition de disposer de ses propres
/// tuiles routières — distinctes des tuiles d'affichage, et bien plus légères
/// qu'attendu : 259 Mo pour l'Aquitaine entière.
///
/// L'échange se fait en JSON brut plutôt qu'avec les types Swift du paquet :
/// c'est le format documenté de Valhalla, stable, et cela évite de dépendre
/// de modèles susceptibles de changer d'une version à l'autre.
@MainActor
final class OfflineRouter: ObservableObject {
    static let shared = OfflineRouter()

    /// Une région téléchargeable, telle que publiée par le workflow
    /// « Tuiles Valhalla » : une Release par région, hébergement gratuit et
    /// adresse stable.
    ///
    /// La liste n'est pas écrite ici : l'Europe en compte plus de deux cents,
    /// et une copie embarquée serait périmée dès la construction suivante.
    /// Elle est lue sur les Releases, cf. `loadRegions()`.
    struct Region: Identifiable, Hashable {
        /// Étiquette de la Release, qui sert aussi de nom de fichier.
        let id: String
        /// « France · Corse », tel que nommé par Geofabrik.
        let nom: String
        /// Poids de l'archive, mesuré sur la pièce jointe.
        let octets: Int

        var archive: URL { URL(string: "\(Region.base)/\(id)/\(id).tar")! }
        var config: URL { URL(string: "\(Region.base)/\(id)/\(id).json")! }

        /// Premier segment du nom : de quoi regrouper deux cents régions.
        var pays: String { nom.components(separatedBy: " · ").first ?? nom }

        /// Ce qui reste une fois le pays retiré — vide pour un petit pays
        /// livré d'un bloc, qui n'a pas de sous-région.
        var detail: String {
            let parts = nom.components(separatedBy: " · ")
            return parts.count > 1 ? parts.dropFirst().joined(separator: " · ") : ""
        }

        var poids: String { String(format: "%.0f Mo", Double(octets) / 1_000_000) }

        static let base = "https://github.com/maxlestage/moncap/releases/download"
        /// Les Releases d'avant le découpage européen portent d'autres
        /// étiquettes ; ce préfixe les écarte sans avoir à les supprimer.
        static let prefixe = "tuiles-eu-"
    }

    /// Régions publiées, relevées sur les Releases.
    @Published private(set) var regions: [Region] = []
    @Published private(set) var installed: String?
    /// Nom lisible de la région installée, gardé de côté : hors réseau la
    /// liste est vide, et l'écran n'aurait plus qu'une étiquette à afficher.
    @Published private(set) var installedNom: String?
    @Published private(set) var downloading = false
    @Published private(set) var chargement = false
    @Published private(set) var progress: Double = 0
    @Published private(set) var message: String?

    /// Moteur chargé, construit à la première demande : ouvrir l'archive coûte,
    /// et la plupart des trajets se calculent en ligne.
    private var engine: Valhalla?

    private init() {
        installed = UserDefaults.standard.string(forKey: Self.key)
        installedNom = UserDefaults.standard.string(forKey: Self.keyNom)
        // Une région effacée à la main ne doit pas rester annoncée.
        if let installed, !FileManager.default.fileExists(atPath: Self.archivePath(installed).path) {
            self.installed = nil
            self.installedNom = nil
            UserDefaults.standard.removeObject(forKey: Self.key)
            UserDefaults.standard.removeObject(forKey: Self.keyNom)
        }
    }

    private static let key = "moncap.valhallaRegion"
    private static let keyNom = "moncap.valhallaRegionNom"

    private static var dossier: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let dir = base.appendingPathComponent("itineraire", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private static func archivePath(_ id: String) -> URL {
        dossier.appendingPathComponent("\(id).tar")
    }

    private static func configPath(_ id: String) -> URL {
        dossier.appendingPathComponent("\(id).json")
    }

    /// Un itinéraire peut-il être calculé sans réseau ?
    var ready: Bool { installed != nil }

    /// Régions d'un même pays, pour l'affichage.
    struct Groupe: Identifiable {
        var id: String { pays }
        let pays: String
        let regions: [Region]
    }

    /// Les régions rangées par pays : deux cents entrées à plat ne se
    /// parcourent pas.
    var groupes: [Groupe] {
        Dictionary(grouping: regions, by: \.pays)
            .map { Groupe(pays: $0.key, regions: $0.value) }
            .sorted { $0.pays.localizedStandardCompare($1.pays) == .orderedAscending }
    }

    /// Régions dont le nom contient le texte cherché.
    ///
    /// `localizedStandardContains` ignore casse et accents : « ile de france »
    /// trouve « Île-de-France », ce qu'une comparaison brute manquerait — et
    /// les noms viennent de Geofabrik, donc dans la langue du pays.
    func recherche(_ texte: String) -> [Region] {
        let cherche = texte.trimmingCharacters(in: .whitespaces)
        guard !cherche.isEmpty else { return [] }
        return regions.filter { $0.nom.localizedStandardContains(cherche) }
    }

    /// Relève les régions publiées, avec leur nom et leur poids.
    ///
    /// La liste vit sur les Releases plutôt que dans le code : l'Europe en
    /// compte plus de deux cents, et chaque reconstruction en change les
    /// tailles. Une copie embarquée mentirait dès le lendemain.
    func loadRegions() async {
        guard regions.isEmpty, !chargement else { return }
        chargement = true
        defer { chargement = false }

        var trouvees: [Region] = []
        // Cent par page, et l'Europe en remplit trois.
        for page in 1...5 {
            let url = URL(
                string:
                    "https://api.github.com/repos/maxlestage/moncap/releases?per_page=100&page=\(page)"
            )!
            guard
                let (data, _) = try? await URLSession.shared.data(from: url),
                let liste = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]],
                !liste.isEmpty
            else { break }

            for release in liste {
                guard let tag = release["tag_name"] as? String, tag.hasPrefix(Region.prefixe),
                    let assets = release["assets"] as? [[String: Any]],
                    // Sans archive, rien à télécharger : une construction
                    // interrompue laisse derrière elle la Release et sa
                    // configuration seule.
                    let tar = assets.first(where: {
                        ($0["name"] as? String)?.hasSuffix(".tar") == true
                    })
                else { continue }
                let titre = release["name"] as? String
                trouvees.append(
                    Region(
                        id: tag,
                        nom: (titre?.isEmpty == false ? titre! : tag),
                        octets: (tar["size"] as? Int) ?? 0))
            }
            if liste.count < 100 { break }
        }
        regions = trouvees.sorted {
            $0.nom.localizedStandardCompare($1.nom) == .orderedAscending
        }
    }

    // MARK: - Installation

    /// Télécharge les tuiles d'une région et sa configuration.
    func install(_ region: Region) async {
        guard !downloading else { return }
        downloading = true
        progress = 0
        message = nil
        defer { downloading = false }

        do {
            // La configuration d'abord : légère, et si elle échoue inutile de
            // faire descendre deux cent cinquante mégaoctets.
            let (configData, _) = try await URLSession.shared.data(from: region.config)
            let archive = try await telecharge(region.archive)

            try archive.write(to: Self.archivePath(region.id))
            let corrigee = try Self.corrige(config: configData, pour: region.id)
            try corrigee.write(to: Self.configPath(region.id))

            engine = nil
            installed = region.id
            installedNom = region.nom
            UserDefaults.standard.set(region.id, forKey: Self.key)
            UserDefaults.standard.set(region.nom, forKey: Self.keyNom)
        } catch {
            message = "Téléchargement interrompu."
            try? FileManager.default.removeItem(at: Self.archivePath(region.id))
        }
    }

    /// Efface la région installée.
    func remove() {
        if let installed {
            try? FileManager.default.removeItem(at: Self.archivePath(installed))
            try? FileManager.default.removeItem(at: Self.configPath(installed))
        }
        engine = nil
        installed = nil
        installedNom = nil
        UserDefaults.standard.removeObject(forKey: Self.key)
        UserDefaults.standard.removeObject(forKey: Self.keyNom)
    }

    /// Téléchargement suivi, pour que l'attente ne soit pas muette.
    private func telecharge(_ url: URL) async throws -> Data {
        let (flux, reponse) = try await URLSession.shared.bytes(from: url)
        let attendu = max(1, reponse.expectedContentLength)
        var recu = Data()
        recu.reserveCapacity(Int(attendu))
        var dernier = 0
        for try await octet in flux {
            recu.append(octet)
            // Publier à chaque octet noierait le fil principal.
            if recu.count - dernier > 1_000_000 {
                dernier = recu.count
                progress = Double(recu.count) / Double(attendu)
            }
        }
        progress = 1
        return recu
    }

    /// Réécrit les chemins de la configuration engendrée sur le runner.
    ///
    /// Elle porte des chemins absolus (`/data/…`) qui n'existent pas sur
    /// l'appareil : seuls ceux-là changent, tout le reste — des dizaines de
    /// réglages de coût, de vitesse et de tri — est conservé tel quel.
    static func corrige(config: Data, pour id: String) throws -> Data {
        guard
            var racine = try JSONSerialization.jsonObject(with: config) as? [String: Any],
            var mjolnir = racine["mjolnir"] as? [String: Any]
        else { throw CocoaError(.propertyListReadCorrupt) }

        mjolnir["tile_extract"] = archivePath(id).path
        // Le dossier de tuiles n'a pas lieu d'être : tout est dans l'archive,
        // et un chemin inexistant ferait échouer l'ouverture.
        mjolnir.removeValue(forKey: "tile_dir")
        mjolnir.removeValue(forKey: "admin")
        mjolnir.removeValue(forKey: "timezone")
        racine["mjolnir"] = mjolnir
        return try JSONSerialization.data(withJSONObject: racine)
    }

    // MARK: - Calcul

    /// Itinéraire routier entre deux points, calculé sur l'appareil.
    ///
    /// Renvoie `nil` si aucune région n'est installée ou si le calcul échoue :
    /// l'appelant garde alors son itinéraire précédent.
    func route(from: CLLocationCoordinate2D, to: CLLocationCoordinate2D) -> OfflineRoute? {
        guard let installed else { return nil }
        do {
            if engine == nil {
                // Les fuseaux horaires que réclame Valhalla sont installés par
                // le paquet lui-même : son utilitaire de fichiers n'est pas
                // exposé, donc rien à faire ici.
                engine = try Valhalla(configPath: Self.configPath(installed).path)
            }
            guard let engine else { return nil }
            let demande: [String: Any] = [
                "locations": [
                    ["lat": from.latitude, "lon": from.longitude],
                    ["lat": to.latitude, "lon": to.longitude],
                ],
                "costing": "auto",
                "directions_options": ["units": "kilometers", "language": "fr-FR"],
            ]
            let json = try JSONSerialization.data(withJSONObject: demande)
            let brut = engine.route(rawRequest: String(decoding: json, as: UTF8.self))
            return OfflineRoute(json: brut)
        } catch {
            return nil
        }
    }
}

/// Itinéraire renvoyé par Valhalla, décodé de sa réponse JSON.
struct OfflineRoute {
    let coordinates: [CLLocationCoordinate2D]
    let steps: [NavStep]
    let km: Double
    let minutes: Double

    init?(json: String) {
        guard
            let data = json.data(using: .utf8),
            let racine = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let trip = racine["trip"] as? [String: Any],
            let legs = trip["legs"] as? [[String: Any]], !legs.isEmpty
        else { return nil }

        var points: [CLLocationCoordinate2D] = []
        var etapes: [NavStep] = []
        for leg in legs {
            let forme = (leg["shape"] as? String).map(OfflineRoute.decode) ?? []
            let depart = points.count
            points += forme
            for m in (leg["maneuvers"] as? [[String: Any]]) ?? [] {
                let debut = (m["begin_shape_index"] as? Int) ?? 0
                let position = min(points.count - 1, max(0, depart + debut))
                guard position >= 0, !points.isEmpty else { continue }
                etapes.append(
                    NavStep(
                        text: (m["instruction"] as? String) ?? "",
                        coord: points[position]))
            }
        }
        guard points.count >= 2 else { return nil }

        let resume = trip["summary"] as? [String: Any]
        coordinates = points
        steps = etapes
        km = (resume?["length"] as? Double) ?? 0
        minutes = ((resume?["time"] as? Double) ?? 0) / 60
    }

    /// Décodage de la polyligne Valhalla, précision six décimales.
    ///
    /// Le format est celui de Google à un détail près — le facteur d'échelle —
    /// et c'est précisément ce détail qui placerait l'itinéraire à des
    /// centaines de kilomètres s'il était pris pour l'autre.
    static func decode(_ encoded: String) -> [CLLocationCoordinate2D] {
        var out: [CLLocationCoordinate2D] = []
        var index = encoded.startIndex
        var lat = 0
        var lon = 0
        func morceau() -> Int? {
            var resultat = 0
            var decalage = 0
            while index < encoded.endIndex {
                let valeur = Int(encoded[index].asciiValue ?? 0) - 63
                index = encoded.index(after: index)
                resultat |= (valeur & 0x1F) << decalage
                decalage += 5
                if valeur < 0x20 {
                    return (resultat & 1) != 0 ? ~(resultat >> 1) : (resultat >> 1)
                }
            }
            return nil
        }
        while index < encoded.endIndex {
            guard let dLat = morceau(), let dLon = morceau() else { break }
            lat += dLat
            lon += dLon
            out.append(
                CLLocationCoordinate2D(
                    latitude: Double(lat) / 1e6, longitude: Double(lon) / 1e6))
        }
        return out
    }
}
