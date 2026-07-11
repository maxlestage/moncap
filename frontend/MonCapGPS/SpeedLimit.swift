import CoreLocation
import Foundation

/// Limitation de vitesse de la route courante, via les données OpenStreetMap
/// (API Overpass, gratuite, sans clé). Best effort : silencieux en cas
/// d'échec ou d'absence de donnée.
@MainActor
final class SpeedLimitService: ObservableObject {
    /// Limite connue pour la route courante (km/h), nil si inconnue.
    @Published private(set) var limitKmh: Int?

    private var lastQuery = Date.distantPast
    private var lastCoord: CLLocationCoordinate2D?
    private var inFlight = false

    /// À appeler à chaque position pendant la navigation. Interroge Overpass
    /// au plus toutes les 20 s et après au moins 80 m parcourus.
    func update(_ c: CLLocationCoordinate2D) {
        guard !inFlight, Date().timeIntervalSince(lastQuery) > 20 else { return }
        if let last = lastCoord {
            let d = CLLocation(latitude: last.latitude, longitude: last.longitude)
                .distance(from: CLLocation(latitude: c.latitude, longitude: c.longitude))
            guard d > 80 else { return }
        }
        lastQuery = Date()
        lastCoord = c
        inFlight = true
        Task {
            if let limit = await Self.fetchLimit(around: c) {
                limitKmh = limit
            }
            inFlight = false
        }
    }

    func reset() {
        limitKmh = nil
        lastCoord = nil
        lastQuery = .distantPast
    }

    /// Cherche le `maxspeed` OSM de la voie la plus proche (rayon 20 m).
    private static func fetchLimit(around c: CLLocationCoordinate2D) async -> Int? {
        let q = "[out:json][timeout:8];"
            + "way(around:20,\(c.latitude),\(c.longitude))[highway][maxspeed];"
            + "out tags 1;"
        var comps = URLComponents(string: "https://overpass-api.de/api/interpreter")!
        comps.queryItems = [URLQueryItem(name: "data", value: q)]
        guard let url = comps.url else { return nil }
        var req = URLRequest(url: url)
        req.timeoutInterval = 8
        guard let (data, _) = try? await URLSession.shared.data(for: req) else { return nil }

        struct Resp: Decodable { let elements: [Element] }
        struct Element: Decodable { let tags: [String: String]? }
        guard let resp = try? JSONDecoder().decode(Resp.self, from: data) else { return nil }
        for el in resp.elements {
            guard let raw = el.tags?["maxspeed"] else { continue }
            if let v = Int(raw) { return v }
            // Variantes courantes : "50 km/h", "30 mph".
            if raw.hasSuffix(" km/h"), let v = Int(raw.dropLast(5)) { return v }
            if raw.hasSuffix(" mph"), let v = Int(raw.dropLast(4)) {
                return Int((Double(v) * 1.609).rounded())
            }
        }
        return nil
    }
}
