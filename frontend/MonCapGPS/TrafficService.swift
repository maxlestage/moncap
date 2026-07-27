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
    /// ou immédiatement si la zone a nettement changé).
    ///
    /// Si `key` (clé TomTom saisie dans les réglages) est non vide, l'app
    /// interroge TomTom **directement** ; sinon elle passe par notre backend
    /// (qui détient éventuellement une clé serveur). Le backend met déjà en
    /// cache 60 s par emprise, inutile d'appeler plus souvent.
    func refresh(minLon: Double, minLat: Double, maxLon: Double, maxLat: Double, key: String) {
        let k = key.trimmingCharacters(in: .whitespacesAndNewlines)
        let bboxKey = String(format: "%.1f,%.1f,%.1f,%.1f", minLon, minLat, maxLon, maxLat)
        let cacheKey = "\(bboxKey)|\(k.isEmpty ? "srv" : "dev")"
        guard cacheKey != lastBBoxKey || Date().timeIntervalSince(lastFetch) > 60 else { return }
        lastFetch = Date()
        lastBBoxKey = cacheKey
        let bbox = "\(minLon),\(minLat),\(maxLon),\(maxLat)"
        let base = baseURL
        Task {
            let j =
                k.isEmpty
                ? await Self.fetchBackend(base: base, bbox: bbox)
                : await Self.fetchTomTom(key: k, bbox: bbox)
            if let j { jams = j }
        }
    }

    /// Réponse JSON du backend : `[{ "lat":.., "lon":.., "delay":.. }, ...]`.
    private struct Point: Decodable {
        let lat: Double
        let lon: Double
        let delay: Int?
    }

    /// Interroge notre backend `GET /traffic`. `nonisolated` : décodage hors du
    /// thread principal. Renvoie nil en cas d'échec (affichage courant gardé).
    nonisolated private static func fetchBackend(base: URL, bbox: String) async -> [TrafficJam]? {
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

    /// Interroge directement l'API TomTom Traffic Incidents (v5) avec la clé de
    /// l'appareil : bouchons (catégorie 6) et routes fermées (8) dans l'emprise.
    /// `nonisolated` : parsing hors du thread principal.
    nonisolated private static func fetchTomTom(key: String, bbox: String) async -> [TrafficJam]? {
        let fields = "{incidents{geometry{type,coordinates},properties{iconCategory,delay}}}"
        var comps = URLComponents(
            string: "https://api.tomtom.com/traffic/services/5/incidentDetails")!
        comps.queryItems = [
            URLQueryItem(name: "key", value: key),
            URLQueryItem(name: "bbox", value: bbox),
            URLQueryItem(name: "fields", value: fields),
            URLQueryItem(name: "language", value: "fr-FR"),
            URLQueryItem(name: "categoryFilter", value: "6,8"),
            URLQueryItem(name: "timeValidityFilter", value: "present"),
        ]
        guard let url = comps.url,
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let incidents = json["incidents"] as? [[String: Any]]
        else { return nil }

        var out: [TrafficJam] = []
        for inc in incidents {
            guard let geom = inc["geometry"] as? [String: Any],
                let type = geom["type"] as? String
            else { continue }
            // Point représentatif : le point lui-même, ou le milieu de la ligne.
            var lonLat: [Double]?
            if type == "Point" {
                lonLat = geom["coordinates"] as? [Double]
            } else if type == "LineString", let cs = geom["coordinates"] as? [[Double]], !cs.isEmpty
            {
                lonLat = cs[cs.count / 2]
            }
            guard let ll = lonLat, ll.count >= 2 else { continue }
            let delay = ((inc["properties"] as? [String: Any])?["delay"] as? Double).map { Int($0) }
            out.append(
                TrafficJam(
                    coordinate: CLLocationCoordinate2D(latitude: ll[1], longitude: ll[0]),
                    delaySeconds: delay ?? 0))
            if out.count >= 300 { break }
        }
        return out
    }
}
