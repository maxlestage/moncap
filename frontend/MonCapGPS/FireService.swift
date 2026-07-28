import CoreLocation
import Foundation

/// Un point d'incendie détecté par satellite (NASA FIRMS).
struct Fire: Identifiable {
    /// Identité stable (position) pour un rendu de carte fluide.
    var id: String { "\(Int(coordinate.latitude * 1e4))|\(Int(coordinate.longitude * 1e4))" }
    let coordinate: CLLocationCoordinate2D
}

/// Incendies actifs sur la France métropolitaine, servis par **notre backend**
/// (`GET /fires`) — détections satellite VIIRS (375 m) des dernières 24 h via
/// l'API NASA FIRMS.
///
/// La clé FIRMS est détenue côté serveur (Heroku) : l'app n'a aucune clé à
/// configurer, tous les utilisateurs profitent des feux automatiquement. En
/// cas d'échec, l'affichage courant est conservé (best effort, silencieux).
@MainActor
final class FireService: ObservableObject {
    @Published private(set) var fires: [Fire] = []

    /// Base de l'API (identique à APIClient).
    private let baseURL = URL(string: "https://moncap-c41a5aaf07e8.herokuapp.com")!

    private var lastFetch = Date.distantPast
    private var lastKey = ""

    /// Rafraîchit la liste des feux (au plus toutes les 15 min ; immédiat si la
    /// clé change). Si `key` (clé FIRMS saisie dans les réglages) est non vide,
    /// l'app interroge la NASA **directement** ; sinon elle passe par notre
    /// backend. Les données FIRMS NRT ne changent que quelques fois par jour.
    func refresh(key: String) {
        let k = key.trimmingCharacters(in: .whitespacesAndNewlines)
        let changed = k != lastKey
        guard changed || Date().timeIntervalSince(lastFetch) > 900 else { return }
        lastFetch = Date()
        lastKey = k
        let base = baseURL
        Task {
            let f = k.isEmpty ? await Self.fetchBackend(base: base) : await Self.fetchNASA(key: k)
            if let f { fires = f }
        }
    }

    /// Réponse JSON du backend : `[{ "lat": .., "lon": .. }, ...]`.
    private struct Point: Decodable {
        let lat: Double
        let lon: Double
    }

    /// Interroge notre backend `GET /fires`. `nonisolated` : décodage hors du
    /// thread principal. Renvoie nil en cas d'échec (affichage gardé).
    nonisolated private static func fetchBackend(base: URL) async -> [Fire]? {
        let url = base.appendingPathComponent("fires")
        guard
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let points = try? JSONDecoder().decode([Point].self, from: data)
        else { return nil }
        return points.map {
            Fire(coordinate: CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon))
        }
    }

    /// Interroge directement l'API NASA FIRMS (CSV) avec la clé de l'appareil :
    /// France métropolitaine (Corse incluse), VIIRS S-NPP, dernières 24 h.
    nonisolated private static func fetchNASA(key: String) async -> [Fire]? {
        let area = "-5.5,41,10,51.5"  // ouest,sud,est,nord
        guard
            let url = URL(
                string:
                    "https://firms.modaps.eosdis.nasa.gov/api/area/csv/\(key)/VIIRS_SNPP_NRT/\(area)/1"
            ),
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let text = String(data: data, encoding: .utf8)
        else { return nil }
        // CSV : latitude,longitude,... (première ligne = en-tête).
        var out: [Fire] = []
        for line in text.split(whereSeparator: \.isNewline).dropFirst() {
            let cols = line.split(separator: ",", omittingEmptySubsequences: false)
            guard cols.count >= 2, let lat = Double(cols[0]), let lon = Double(cols[1]) else {
                continue
            }
            out.append(Fire(coordinate: CLLocationCoordinate2D(latitude: lat, longitude: lon)))
            if out.count >= 1000 { break }
        }
        return out
    }
}
