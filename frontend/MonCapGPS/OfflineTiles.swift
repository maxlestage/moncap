import Foundation
import MapKit

/// Fond de carte : celui par défaut, ou celui que l'utilisateur héberge.
///
/// Apple Plans ne permet ni de télécharger ni de conserver ses tuiles : les
/// cartes viennent donc d'OpenStreetMap, rendues par MapLibre, seule façon de
/// les garder pour rouler sans réseau.
enum TileSource {
    /// Ancien réglage de tuiles images, conservé pour pouvoir l'effacer chez
    /// ceux qui l'avaient renseigné.
    static let defaultsKey = "moncap.tileURL"
    /// Style vectoriel MapLibre hébergé par l'utilisateur.
    static let styleKey = "moncap.styleURL"

    /// Style par défaut : OpenFreeMap sert des tuiles vectorielles OpenStreetMap
    /// libres, sans clé ni inscription, polices et icônes comprises.
    ///
    /// C'est ce qui rend la carte utilisable sans rien installer. Un serveur
    /// personnel (archive PMTiles) reste possible : il suffit de le renseigner
    /// dans les réglages, il prend alors le pas sur cette valeur.
    static let defaultStyle = "https://tiles.openfreemap.org/styles/liberty"

    /// Style effectivement utilisé : celui de l'utilisateur, sinon le nôtre.
    static var styleURL: URL {
        let custom = (UserDefaults.standard.string(forKey: styleKey) ?? "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if custom.hasPrefix("http"), let url = URL(string: custom) { return url }
        return URL(string: defaultStyle)!
    }

    /// Fond à utiliser : celui de l'utilisateur s'il répond vraiment, sinon le
    /// nôtre.
    ///
    /// Une adresse enregistrée qui ne sert aucune carte donnait un écran vide.
    /// La vérifier avant de la retenir évite d'y rester bloqué, et se fait ici
    /// une seule fois pour tous les écrans.
    static func resolvedStyle(custom: String) async -> URL {
        let trimmed = custom.trimmingCharacters(in: .whitespacesAndNewlines)
        let fallback = URL(string: defaultStyle)!
        guard trimmed.hasPrefix("http"), let url = URL(string: trimmed) else { return fallback }
        // Injoignable n'est pas la même chose qu'inexistant : sans réseau,
        // tout échoue, et basculer sur le fond par défaut rendrait
        // inutilisables les zones téléchargées, qui sont liées au style avec
        // lequel elles ont été enregistrées. On ne l'écarte que si le serveur
        // répond bel et bien qu'il n'a pas de cartes.
        return await StyleProbe.check(url) == .notConfigured ? fallback : url
    }
}

/// État d'un style vectoriel, vérifié avant de rendre la carte.
///
/// Sans cette vérification, un serveur sans cartes configurées donne une carte
/// **grise et muette** : MapLibre échoue en silence. On préfère le dire.
enum StyleStatus: Equatable {
    case checking
    case ok
    /// Le serveur répond, mais aucune carte n'y est installée.
    case notConfigured
    /// Serveur injoignable (pas de réseau, ou en panne).
    case unreachable

    /// Message à afficher, ou `nil` si tout va bien.
    ///
    /// `hasStored` : des zones sont déjà enregistrées. Sans réseau elles
    /// restent consultables — inutile d'alarmer l'utilisateur.
    func message(hasStored: Bool) -> String? {
        switch self {
        case .checking, .ok:
            return nil
        case .notConfigured:
            // Le fond personnel injoignable est écarté en amont : rester ici
            // signifie que même le fond par défaut ne sert rien.
            return "Fond de carte indisponible pour le moment."
        case .unreachable:
            return hasStored
                ? nil
                : "Serveur de cartes injoignable. Les zones déjà enregistrées restent consultables."
        }
    }

    /// Télécharger n'a de sens que si le serveur a bien des cartes.
    var canDownload: Bool { self == .ok }
}

/// Vérifie qu'une feuille de style est réellement servie et exploitable.
enum StyleProbe {
    /// Classe une réponse HTTP. Séparé du réseau pour rester vérifiable.
    static func classify(status: Int, body: Data) -> StyleStatus {
        // 404 : notre backend répond ainsi tant que ni TILES_PMTILES ni
        // TILES_UPSTREAM ne sont renseignés.
        guard status == 200 else {
            return status == 404 ? .notConfigured : .unreachable
        }
        // Un style utilisable a des sources : sans elles, la carte serait vide.
        guard
            let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any],
            let sources = json["sources"] as? [String: Any], !sources.isEmpty
        else { return .notConfigured }
        return .ok
    }

    static func check(_ url: URL) async -> StyleStatus {
        var req = URLRequest(url: url)
        req.timeoutInterval = 10
        do {
            let (data, response) = try await URLSession.shared.data(for: req)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            return classify(status: status, body: data)
        } catch {
            return .unreachable
        }
    }
}

/// Volume d'une zone à télécharger, estimé avant de commencer.
///
/// MapLibre n'annonce rien tant qu'il n'a pas commencé : sans cette estimation
/// un simple appui pouvait lancer plusieurs gigaoctets en silence — une région
/// entière au niveau « Rue » en représente une vingtaine.
struct OfflineEstimate {
    let tiles: Int
    let bytes: Int

    /// Plafonds : de quoi couvrir une ville rue par rue, une agglomération au
    /// niveau « Ville » ou une région au niveau « Région » — mais pas un pays
    /// entier, qui se compterait en gigaoctets.
    ///
    /// Le poids prime sur le nombre de tuiles : c'est lui qui remplit le
    /// téléphone et le forfait.
    static let maxBytes = 500_000_000
    static let maxTiles = 12_000

    var toolarge: Bool { bytes > Self.maxBytes || tiles > Self.maxTiles }

    /// Nombre de tuiles et poids attendus pour une zone et une plage de zooms.
    static func of(
        sw: CLLocationCoordinate2D, ne: CLLocationCoordinate2D, zooms: ClosedRange<Int>
    ) -> OfflineEstimate {
        let dLat = abs(ne.latitude - sw.latitude)
        let dLon = abs(ne.longitude - sw.longitude)
        let lat = (ne.latitude + sw.latitude) / 2
        var tiles = 0
        var bytes = 0
        for z in zooms {
            let n = pow(2.0, Double(z))
            let across = dLon / 360 * n
            // Une tuile couvre d'autant moins de latitude qu'on s'éloigne de
            // l'équateur : la projection étire les hautes latitudes.
            let tall = dLat / (360 / n * cos(lat * .pi / 180))
            let count = (Int(across.rounded(.up)) + 1) * (Int(tall.rounded(.up)) + 1)
            tiles += count
            bytes += count * weight(z)
        }
        return OfflineEstimate(tiles: tiles, bytes: bytes)
    }

    /// Poids moyen d'une tuile vectorielle, mesuré sur OpenFreeMap : les bas
    /// niveaux couvrent large et pèsent lourd, les hauts sont plus légers.
    private static func weight(_ z: Int) -> Int {
        z < 10 ? 250_000 : (z < 13 ? 200_000 : 120_000)
    }
}
