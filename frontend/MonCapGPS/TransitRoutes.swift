import CoreLocation
import Foundation
import SwiftUI

/// Itinéraires en transports en commun, servis par notre backend
/// (`GET /transit`).
///
/// Le calcul vient de Transitous, service communautaire bâti sur MOTIS et les
/// GTFS publics, sans clé. L'app passe par le serveur et non en direct : la
/// réponse brute pèse deux cents kilo-octets là où l'écran en montre quelques
/// uns, et le fournisseur se remplace d'un seul côté.
///
/// Pas de transports hors ligne, et ce n'est pas un oubli : un itinéraire en
/// tram dépend des horaires du moment, une donnée qui n'existe pas dans un
/// paquet téléchargé la semaine dernière.

/// Un tronçon : une marche, ou un véhicule et sa ligne.
struct TransitLeg: Decodable, Hashable {
    /// WALK, TRAM, BUS, REGIONAL_RAIL, RAIL, SUBWAY, FERRY…
    let mode: String
    /// « E », « 27 » — absent pour la marche.
    let ligne: String?
    /// Couleur publiée par le réseau, hexadécimal sans croisillon.
    let couleur: String?
    /// La girouette du véhicule.
    let direction: String?
    let de: String
    let a: String
    let depart: String
    let arrivee: String
    /// Polyligne encodée, précision sept — celle de MOTIS.
    let trace: String
    /// Retard temps réel en secondes, quand le réseau le publie. Absent :
    /// horaire théorique — un vieux serveur n'envoie pas le champ, et l'app
    /// ne doit pas s'en trouver muette.
    let retard_s: Int?

    var estVehicule: Bool { mode != "WALK" }

    /// Couleur d'affichage : celle du réseau, ou une teinte par mode.
    var teinte: Color {
        if let couleur, let c = Color(hexa: couleur) { return c }
        switch mode {
        case "TRAM": return .purple
        case "BUS": return .teal
        case "SUBWAY": return .orange
        case "RAIL", "REGIONAL_RAIL", "HIGHSPEED_RAIL": return .indigo
        case "FERRY": return .blue
        default: return .gray
        }
    }

    /// Pictogramme SF Symbols du mode.
    var symbole: String {
        switch mode {
        case "TRAM": return "tram.fill"
        case "BUS": return "bus.fill"
        case "SUBWAY": return "tram.tunnel.fill"
        case "RAIL", "REGIONAL_RAIL", "HIGHSPEED_RAIL": return "train.side.front.car"
        case "FERRY": return "ferry.fill"
        default: return "figure.walk"
        }
    }

    /// Le mode, nommé comme on le dit.
    var nomDuMode: String {
        switch mode {
        case "TRAM": return NSLocalizedString("Tram", comment: "")
        case "BUS": return NSLocalizedString("Bus", comment: "")
        case "SUBWAY": return NSLocalizedString("Métro", comment: "")
        case "RAIL", "REGIONAL_RAIL", "HIGHSPEED_RAIL":
            return NSLocalizedString("Train", comment: "")
        case "FERRY": return NSLocalizedString("Bac", comment: "")
        default: return NSLocalizedString("Marche", comment: "")
        }
    }
}

/// Un itinéraire complet, tel que le serveur le taille.
struct TransitItineraire: Decodable {
    let duree_s: Int
    let depart: String
    let arrivee: String
    let correspondances: Int
    let legs: [TransitLeg]
}

enum TransitService {
    /// Demande les itinéraires au backend. `nil` : injoignable ou refusé —
    /// distinct d'une liste vide, qui veut dire « aucun trajet possible ».
    ///
    /// `depart` : l'heure souhaitée — de départ, ou d'arrivée au plus tard si
    /// `arriveeAvant`. Absente : maintenant. Envoyée en UTC terminé par « Z »,
    /// le seul format que le serveur accepte — un « +02:00 » serait mangé par
    /// l'encodage d'URL.
    static func fetch(
        from: CLLocationCoordinate2D, to: CLLocationCoordinate2D,
        depart: Date? = nil, arriveeAvant: Bool = false
    ) async -> [TransitItineraire]? {
        var comps = URLComponents(
            url: APIClient().baseURL.appendingPathComponent("transit"),
            resolvingAgainstBaseURL: false)!
        comps.queryItems = [
            URLQueryItem(name: "from", value: String(format: "%.5f,%.5f", from.latitude, from.longitude)),
            URLQueryItem(name: "to", value: String(format: "%.5f,%.5f", to.latitude, to.longitude)),
        ]
        if let depart {
            let f = ISO8601DateFormatter()
            f.formatOptions = [.withInternetDateTime]
            f.timeZone = TimeZone(identifier: "UTC")
            comps.queryItems?.append(URLQueryItem(name: "time", value: f.string(from: depart)))
            if arriveeAvant {
                comps.queryItems?.append(URLQueryItem(name: "arrive", value: "1"))
            }
        }
        guard let url = comps.url,
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let its = try? JSONDecoder().decode([TransitItineraire].self, from: data)
        else { return nil }
        return its
    }

    /// Décode une polyligne encodée. Même algorithme que `OfflineRoute.decode`,
    /// mais la précision est un paramètre : Valhalla encode à six décimales,
    /// MOTIS à sept — et confondre les deux placerait le tracé à des centaines
    /// de kilomètres.
    static func decode(_ encoded: String, precision: Double = 1e7) -> [CLLocationCoordinate2D] {
        var out: [CLLocationCoordinate2D] = []
        var index = encoded.startIndex
        var lat = 0, lon = 0
        while index < encoded.endIndex {
            for est in [false, true] {
                var resultat = 0
                var decalage = 0
                while true {
                    guard index < encoded.endIndex else { return out }
                    let b = Int(encoded[index].asciiValue ?? 63) - 63
                    index = encoded.index(after: index)
                    resultat |= (b & 0x1F) << decalage
                    decalage += 5
                    if b < 0x20 { break }
                }
                let delta = (resultat & 1) != 0 ? ~(resultat >> 1) : (resultat >> 1)
                if est { lon += delta } else { lat += delta }
            }
            out.append(
                CLLocationCoordinate2D(
                    latitude: Double(lat) / precision, longitude: Double(lon) / precision))
        }
        return out
    }

    /// « 18:40 » local depuis l'horodatage ISO du serveur.
    static func heure(_ iso: String) -> String? {
        let f = ISO8601DateFormatter()
        guard let date = f.date(from: iso) else { return nil }
        let sortie = DateFormatter()
        sortie.dateFormat = "HH:mm"
        return sortie.string(from: date)
    }
}

extension Color {
    /// Couleur depuis « RRGGBB » sans croisillon, comme les GTFS l'écrivent.
    init?(hexa: String) {
        var v: UInt64 = 0
        guard hexa.count == 6, Scanner(string: hexa).scanHexInt64(&v) else { return nil }
        self.init(
            red: Double((v & 0xFF0000) >> 16) / 255,
            green: Double((v & 0x00FF00) >> 8) / 255,
            blue: Double(v & 0x0000FF) / 255)
    }
}
