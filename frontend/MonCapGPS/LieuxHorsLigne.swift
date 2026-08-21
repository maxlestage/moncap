import CoreLocation
import Foundation
import MapKit

/// L'annuaire embarqué : les lieux et les limites de vitesse d'une région,
/// lus dans les annexes téléchargées avec elle.
///
/// Le workflow des tuiles produit, du même extrait OpenStreetMap que le graphe
/// routier, deux fichiers par région : `<id>.lieux.zz` — les dix-neuf
/// catégories de lieux de l'app, une ligne tabulée par lieu — et
/// `<id>.vitesses.zz` — des jalons tous les ~120 m le long des voies à limite
/// explicite, neuf octets par jalon. Les deux sont en DEFLATE brut, ce que le
/// cadre Compression décode sous le nom `.zlib`.
///
/// C'est le pendant hors ligne d'Overpass et de MKLocalSearch : chercher une
/// pharmacie ou afficher la limite de vitesse cesse de dépendre du réseau dès
/// qu'une région installée couvre l'endroit.
actor LieuxHorsLigne {
    static let shared = LieuxHorsLigne()

    private struct Entree {
        let cat: String
        let lat: Double
        let lon: Double
        let nom: String
    }

    /// Lieux chargés par région installée. L'ensemble `charges` retient aussi
    /// les régions sans annexe : on ne relit pas le disque à chaque recherche
    /// pour retrouver la même absence.
    private var lieuxParRegion: [String: [Entree]] = [:]
    private var charges: Set<String> = []

    /// À l'installation, la mise à jour ou l'effacement d'une région : ce
    /// qu'on avait lu ne vaut plus.
    func oublie(_ id: String) {
        lieuxParRegion[id] = nil
        charges.remove(id)
    }

    /// Les lieux d'une catégorie dans la zone donnée, les plus proches du
    /// centre d'abord. Vide si aucune région installée ne couvre l'endroit.
    func lieux(_ kind: POIKind, dans region: MKCoordinateRegion, max: Int = 150) async -> [POI] {
        let ids = await MainActor.run { OfflineRouter.shared.installees.map(\.id) }
        for id in ids where !charges.contains(id) {
            charges.insert(id)
            lieuxParRegion[id] = Self.litLieux(OfflineRouter.lieuxPath(id))
        }
        let oLat = region.center.latitude - region.span.latitudeDelta / 2
        let nLat = region.center.latitude + region.span.latitudeDelta / 2
        let oLon = region.center.longitude - region.span.longitudeDelta / 2
        let nLon = region.center.longitude + region.span.longitudeDelta / 2
        let cle = kind.rawValue
        var trouves: [(d: Double, e: Entree)] = []
        for id in ids {
            for e in lieuxParRegion[id] ?? []
            where e.cat == cle && e.lat >= oLat && e.lat <= nLat
                && e.lon >= oLon && e.lon <= nLon
            {
                let dx = e.lon - region.center.longitude
                let dy = e.lat - region.center.latitude
                trouves.append((dx * dx + dy * dy, e))
            }
        }
        trouves.sort { $0.d < $1.d }
        let defaut = NSLocalizedString(kind.label, comment: "")
        return trouves.prefix(max).map {
            POI(
                name: $0.e.nom.isEmpty ? defaut : $0.e.nom,
                coordinate: CLLocationCoordinate2D(latitude: $0.e.lat, longitude: $0.e.lon))
        }
    }

    /// Recherche par **nom** dans l'annuaire embarqué — le repli de la barre
    /// de recherche quand le réseau manque : la pharmacie Dupont se trouve
    /// en mode avion. Plis et accents ignorés, les plus proches d'abord.
    /// Les adresses, elles, restent l'affaire du réseau : l'annuaire ne
    /// porte que des lieux nommés.
    func cherche(
        _ texte: String, autour centre: CLLocationCoordinate2D, max: Int = 20
    ) async -> [POI] {
        let requete = texte
            .folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil)
            .trimmingCharacters(in: .whitespaces)
        guard requete.count >= 2 else { return [] }
        let ids = await MainActor.run { OfflineRouter.shared.installees.map(\.id) }
        for id in ids where !charges.contains(id) {
            charges.insert(id)
            lieuxParRegion[id] = Self.litLieux(OfflineRouter.lieuxPath(id))
        }
        var trouves: [(d: Double, e: Entree)] = []
        for id in ids {
            for e in lieuxParRegion[id] ?? [] {
                guard !e.nom.isEmpty else { continue }
                let nom = e.nom
                    .folding(
                        options: [.diacriticInsensitive, .caseInsensitive], locale: nil)
                guard nom.contains(requete) else { continue }
                let dx = e.lon - centre.longitude
                let dy = e.lat - centre.latitude
                trouves.append((dx * dx + dy * dy, e))
            }
        }
        trouves.sort { $0.d < $1.d }
        return trouves.prefix(max).map {
            POI(
                name: $0.e.nom,
                coordinate: CLLocationCoordinate2D(
                    latitude: $0.e.lat, longitude: $0.e.lon))
        }
    }

    /// Les jalons de vitesse autour d'un tracé (marge ~1 km), toutes régions
    /// installées confondues. Relu du disque à chaque appel — l'appel est
    /// rare (au départ du guidage, puis toutes les dix minutes) et garder le
    /// fichier décompressé en mémoire coûterait des dizaines de mégaoctets.
    func vitesses(pres trace: [CLLocationCoordinate2D]) async -> [(lat: Double, lon: Double, v: Int)] {
        guard let premier = trace.first else { return [] }
        var oLat = premier.latitude, nLat = premier.latitude
        var oLon = premier.longitude, nLon = premier.longitude
        for c in trace {
            oLat = min(oLat, c.latitude)
            nLat = max(nLat, c.latitude)
            oLon = min(oLon, c.longitude)
            nLon = max(nLon, c.longitude)
        }
        let marge = 0.01
        oLat -= marge
        nLat += marge
        oLon -= marge
        nLon += marge

        let ids = await MainActor.run { OfflineRouter.shared.installees.map(\.id) }
        var jalons: [(Double, Double, Int)] = []
        for id in ids {
            guard let brut = try? Data(contentsOf: OfflineRouter.vitessesPath(id)),
                let donnees = Self.degonfle(brut)
            else { continue }
            var i = 0
            while i + 9 <= donnees.count, jalons.count < 30_000 {
                let lat = Double(Self.int32LE(donnees, i)) / 1e5
                let lon = Double(Self.int32LE(donnees, i + 4)) / 1e5
                let v = Int(donnees[i + 8])
                i += 9
                if lat >= oLat, lat <= nLat, lon >= oLon, lon <= nLon {
                    jalons.append((lat, lon, v))
                }
            }
        }
        return jalons
    }

    /// Une ligne par lieu : `catégorie⇥lat⇥lon⇥nom`.
    private static func litLieux(_ url: URL) -> [Entree]? {
        guard let brut = try? Data(contentsOf: url),
            let donnees = degonfle(brut),
            let texte = String(data: donnees, encoding: .utf8)
        else { return nil }
        var entrees: [Entree] = []
        texte.enumerateLines { ligne, _ in
            let champs = ligne.components(separatedBy: "\t")
            guard champs.count >= 4,
                let lat = Double(champs[1]), let lon = Double(champs[2])
            else { return }
            entrees.append(Entree(cat: champs[0], lat: lat, lon: lon, nom: champs[3]))
        }
        return entrees.isEmpty ? nil : entrees
    }

    /// DEFLATE brut → données. `.zlib` du cadre Compression est bien le flux
    /// brut (sans en-tête), celui que le générateur écrit avec wbits -15.
    private static func degonfle(_ brut: Data) -> Data? {
        try? (brut as NSData).decompressed(using: .zlib) as Data
    }

    /// Petit-boutiste, assemblé octet par octet : pas d'exigence d'alignement.
    private static func int32LE(_ d: Data, _ i: Int) -> Int32 {
        let u =
            UInt32(d[i]) | UInt32(d[i + 1]) << 8
            | UInt32(d[i + 2]) << 16 | UInt32(d[i + 3]) << 24
        return Int32(bitPattern: u)
    }
}
