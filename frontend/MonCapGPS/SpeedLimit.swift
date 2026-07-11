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
    /// Limites préchargées le long de l'itinéraire (centre de voie → limite).
    private var cache: [(coord: CLLocationCoordinate2D, limit: Int)] = []

    /// À appeler à chaque position. Consulte d'abord le cache préchargé le
    /// long de l'itinéraire (instantané), sinon interroge Overpass au plus
    /// toutes les 20 s et après au moins 80 m parcourus.
    func update(_ c: CLLocationCoordinate2D) {
        if let cached = nearestCachedLimit(c, within: 300) {
            limitKmh = cached
            return
        }
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

    /// Rafraîchissement immédiat (au lancement de l'app) : la bonne vitesse
    /// s'affiche sans attendre le premier déplacement.
    func refreshNow(_ c: CLLocationCoordinate2D) {
        guard !inFlight else { return }
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

    /// Précharge les limites le long d'un itinéraire (une seule requête
    /// groupée) : pendant la navigation, la limite est servie depuis ce cache.
    func preload(along coords: [CLLocationCoordinate2D]) {
        let sampled = Self.sample(coords, max: 25)
        guard !sampled.isEmpty else { return }
        Task {
            if let segments = await Self.fetchLimits(at: sampled) {
                cache = segments
            }
        }
    }

    func reset() {
        limitKmh = nil
        lastCoord = nil
        lastQuery = .distantPast
        cache = []
    }

    /// Vide le cache d'itinéraire (fin de navigation) en gardant la limite
    /// courante affichée.
    func clearRoute() {
        cache = []
    }

    /// Limite du point de cache le plus proche (m), si assez proche.
    private func nearestCachedLimit(_ c: CLLocationCoordinate2D, within meters: Double) -> Int? {
        var best: (d: Double, limit: Int)?
        for entry in cache {
            let d = CLLocation(latitude: entry.coord.latitude, longitude: entry.coord.longitude)
                .distance(from: CLLocation(latitude: c.latitude, longitude: c.longitude))
            if d <= meters, best == nil || d < best!.d {
                best = (d, entry.limit)
            }
        }
        return best?.limit
    }

    /// Échantillonne au plus `n` points répartis le long d'un tracé.
    private static func sample(_ coords: [CLLocationCoordinate2D], max n: Int)
        -> [CLLocationCoordinate2D]
    {
        guard coords.count > n, n > 1 else { return coords }
        let step = Double(coords.count - 1) / Double(n - 1)
        return (0..<n).map { coords[Int((Double($0) * step).rounded())] }
    }

    /// Requête groupée : limites des voies autour de chaque point échantillonné.
    private static func fetchLimits(at points: [CLLocationCoordinate2D])
        async -> [(coord: CLLocationCoordinate2D, limit: Int)]?
    {
        let clauses = points
            .map { "way(around:25,\($0.latitude),\($0.longitude))[highway][maxspeed];" }
            .joined()
        let q = "[out:json][timeout:15];(\(clauses));out tags center;"
        var comps = URLComponents(string: "https://overpass-api.de/api/interpreter")!
        comps.queryItems = [URLQueryItem(name: "data", value: q)]
        guard let url = comps.url else { return nil }
        var req = URLRequest(url: url)
        req.timeoutInterval = 15
        guard let (data, _) = try? await URLSession.shared.data(for: req) else { return nil }

        struct Resp: Decodable { let elements: [Element] }
        struct Center: Decodable { let lat: Double; let lon: Double }
        struct Element: Decodable { let tags: [String: String]?; let center: Center? }
        guard let resp = try? JSONDecoder().decode(Resp.self, from: data) else { return nil }
        var out: [(CLLocationCoordinate2D, Int)] = []
        for el in resp.elements {
            guard let center = el.center, let raw = el.tags?["maxspeed"],
                let limit = parseMaxspeed(raw)
            else { continue }
            out.append((CLLocationCoordinate2D(latitude: center.lat, longitude: center.lon), limit))
        }
        return out
    }

    /// Interprète les valeurs `maxspeed` OSM courantes.
    private static func parseMaxspeed(_ raw: String) -> Int? {
        if let v = Int(raw) { return v }
        if raw.hasSuffix(" km/h"), let v = Int(raw.dropLast(5)) { return v }
        if raw.hasSuffix(" mph"), let v = Int(raw.dropLast(4)) {
            return Int((Double(v) * 1.609).rounded())
        }
        return nil
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
            if let raw = el.tags?["maxspeed"], let v = parseMaxspeed(raw) { return v }
        }
        return nil
    }
}
