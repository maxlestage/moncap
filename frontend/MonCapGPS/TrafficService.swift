import CoreLocation
import Foundation

/// Un bouchon (ou route fermée) en temps réel, remonté par le backend.
struct TrafficJam: Identifiable {
    /// Identité stable (position) pour un rendu / une déduplication fluides.
    var id: String { "\(Int(coordinate.latitude * 1e4))|\(Int(coordinate.longitude * 1e4))" }
    let coordinate: CLLocationCoordinate2D
    /// Retard estimé en secondes (0 si inconnu).
    let delaySeconds: Int
}

/// Bouchons en temps réel (API TomTom Traffic Incidents) servis par **notre
/// backend** (`GET /traffic?bbox=…`). La clé TomTom est détenue côté serveur
/// (Heroku) : l'app n'a rien à configurer. En cas d'échec, l'affichage courant
/// est conservé (best effort, silencieux).
@MainActor
final class TrafficService: ObservableObject {
    @Published private(set) var jams: [TrafficJam] = []

    /// Base de l'API (identique à APIClient).
    private let baseURL = URL(string: "https://moncap-c41a5aaf07e8.herokuapp.com")!

    private var lastFetch = Date.distantPast
    private var lastBBoxKey = ""

    /// Rafraîchit les bouchons dans l'emprise donnée (au plus toutes les 60 s,
    /// ou immédiatement si la zone a nettement changé). Le backend met déjà en
    /// cache 60 s par emprise, inutile d'appeler plus souvent.
    func refresh(minLon: Double, minLat: Double, maxLon: Double, maxLat: Double) {
        let key = String(format: "%.1f,%.1f,%.1f,%.1f", minLon, minLat, maxLon, maxLat)
        guard key != lastBBoxKey || Date().timeIntervalSince(lastFetch) > 60 else { return }
        lastFetch = Date()
        lastBBoxKey = key
        let bbox = "\(minLon),\(minLat),\(maxLon),\(maxLat)"
        Task {
            if let j = await Self.fetch(base: baseURL, bbox: bbox) { jams = j }
        }
    }

    /// Réponse JSON du backend : `[{ "lat":.., "lon":.., "delay":.. }, ...]`.
    private struct Point: Decodable {
        let lat: Double
        let lon: Double
        let delay: Int?
    }

    /// Interroge `GET /traffic`. `nonisolated` : décodage hors du thread
    /// principal. Renvoie nil en cas d'échec (affichage courant gardé).
    nonisolated private static func fetch(base: URL, bbox: String) async -> [TrafficJam]? {
        var comps = URLComponents(
            url: base.appendingPathComponent("traffic"), resolvingAgainstBaseURL: false)!
        comps.queryItems = [URLQueryItem(name: "bbox", value: bbox)]
        guard let url = comps.url,
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let points = try? JSONDecoder().decode([Point].self, from: data)
        else { return nil }
        return points.map {
            TrafficJam(
                coordinate: CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon),
                delaySeconds: $0.delay ?? 0)
        }
    }
}
