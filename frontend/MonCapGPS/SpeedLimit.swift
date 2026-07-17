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
    /// Voies préchargées le long de l'itinéraire : géométrie complète →
    /// la limite bascule dès qu'on change de route.
    private struct CachedWay {
        let coords: [CLLocationCoordinate2D]
        let limit: Int
    }
    private var cache: [CachedWay] = []
    /// Itinéraire courant (pour re-précharger périodiquement).
    private var routeCoords: [CLLocationCoordinate2D] = []
    private var lastPreload = Date.distantPast

    /// À appeler à chaque position. La limite est appariée à la géométrie de
    /// la voie la plus proche du cache (instantané et précis), avec repli sur
    /// une requête Overpass (au plus toutes les 12 s / 60 m). Le cache
    /// d'itinéraire est re-préchargé toutes les 10 min (limites temporaires).
    func update(_ c: CLLocationCoordinate2D) {
        if !routeCoords.isEmpty, Date().timeIntervalSince(lastPreload) > 600 {
            preload(along: routeCoords)
        }
        if let cached = nearestWayLimit(c, within: 35) {
            limitKmh = cached
            return
        }
        guard !inFlight, Date().timeIntervalSince(lastQuery) > 12 else { return }
        if let last = lastCoord {
            let d = CLLocation(latitude: last.latitude, longitude: last.longitude)
                .distance(from: CLLocation(latitude: c.latitude, longitude: c.longitude))
            guard d > 60 else { return }
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

    /// Précharge les voies (géométrie + limite) le long d'un itinéraire, en
    /// une requête groupée. Re-préchargé toutes les 10 min pendant la
    /// navigation pour suivre les changements (travaux, limites temporaires).
    func preload(along coords: [CLLocationCoordinate2D]) {
        let sampled = Self.sample(coords, max: 25)
        guard !sampled.isEmpty else { return }
        routeCoords = coords
        lastPreload = Date()
        Task {
            if let ways = await Self.fetchWays(at: sampled) {
                cache = ways
            }
        }
    }

    func reset() {
        limitKmh = nil
        lastCoord = nil
        lastQuery = .distantPast
        cache = []
        routeCoords = []
        lastPreload = .distantPast
    }

    /// Vide le cache d'itinéraire (fin de navigation) en gardant la limite
    /// courante affichée.
    func clearRoute() {
        cache = []
        routeCoords = []
        lastPreload = .distantPast
    }

    /// Limite de la voie dont la géométrie passe à moins de `meters` de la
    /// position (la plus proche gagne).
    private func nearestWayLimit(_ c: CLLocationCoordinate2D, within meters: Double) -> Int? {
        var best: (d: Double, limit: Int)?
        for way in cache {
            let d = Self.distanceToPolyline(c, way.coords)
            if d <= meters, best == nil || d < best!.d {
                best = (d, way.limit)
            }
        }
        return best?.limit
    }

    /// Distance minimale (m) d'un point à une polyligne (projection locale).
    nonisolated private static func distanceToPolyline(
        _ p: CLLocationCoordinate2D, _ coords: [CLLocationCoordinate2D]
    ) -> Double {
        guard coords.count >= 2 else {
            guard let only = coords.first else { return .greatestFiniteMagnitude }
            return CLLocation(latitude: p.latitude, longitude: p.longitude)
                .distance(from: CLLocation(latitude: only.latitude, longitude: only.longitude))
        }
        let mLat = 111_320.0
        let mLon = 111_320.0 * cos(p.latitude * .pi / 180)
        func xy(_ c: CLLocationCoordinate2D) -> (Double, Double) {
            ((c.longitude - p.longitude) * mLon, (c.latitude - p.latitude) * mLat)
        }
        var best = Double.greatestFiniteMagnitude
        for i in 0..<(coords.count - 1) {
            let (ax, ay) = xy(coords[i])
            let (bx, by) = xy(coords[i + 1])
            let dx = bx - ax, dy = by - ay
            let len2 = dx * dx + dy * dy
            let t = len2 == 0 ? 0 : max(0, min(1, -(ax * dx + ay * dy) / len2))
            let cx = ax + t * dx, cy = ay + t * dy
            best = min(best, (cx * cx + cy * cy).squareRoot())
        }
        return best
    }

    /// Échantillonne au plus `n` points répartis le long d'un tracé.
    nonisolated private static func sample(_ coords: [CLLocationCoordinate2D], max n: Int)
        -> [CLLocationCoordinate2D]
    {
        guard coords.count > n, n > 1 else { return coords }
        let step = Double(coords.count - 1) / Double(n - 1)
        return (0..<n).map { coords[Int((Double($0) * step).rounded())] }
    }

    /// Requête groupée : voies (géométrie complète + limite) autour de chaque
    /// point échantillonné — maxspeed explicite ou défaut légal français.
    nonisolated private static func fetchWays(at points: [CLLocationCoordinate2D])
        async -> [CachedWay]?
    {
        let clauses = points
            .map { "way(around:25,\($0.latitude),\($0.longitude))[highway];" }
            .joined()
        let q = "[out:json][timeout:15];(\(clauses));out tags geom;"
        var comps = URLComponents(string: "https://overpass-api.de/api/interpreter")!
        comps.queryItems = [URLQueryItem(name: "data", value: q)]
        guard let url = comps.url else { return nil }
        var req = URLRequest(url: url)
        req.timeoutInterval = 15
        guard let (data, _) = try? await URLSession.shared.data(for: req) else { return nil }

        struct Resp: Decodable { let elements: [Element] }
        struct Point: Decodable { let lat: Double; let lon: Double }
        struct Element: Decodable { let tags: [String: String]?; let geometry: [Point]? }
        guard let resp = try? JSONDecoder().decode(Resp.self, from: data) else { return nil }
        var out: [CachedWay] = []
        for el in resp.elements {
            guard let geom = el.geometry, geom.count >= 2,
                let limit = limit(fromTags: el.tags)
            else { continue }
            let coords = geom.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
            out.append(CachedWay(coords: coords, limit: limit))
        }
        return out
    }

    /// Interprète les valeurs `maxspeed` OSM courantes, y compris les codes
    /// nationaux français.
    nonisolated private static func parseMaxspeed(_ raw: String) -> Int? {
        switch raw {
        case "FR:urban": return 50
        case "FR:rural": return 80
        case "FR:motorway": return 130
        case "FR:zone30", "zone30": return 30
        case "FR:walk", "walk": return 20
        default: break
        }
        if let v = Int(raw) { return v }
        if raw.hasSuffix(" km/h"), let v = Int(raw.dropLast(5)) { return v }
        if raw.hasSuffix(" mph"), let v = Int(raw.dropLast(4)) {
            return Int((Double(v) * 1.609).rounded())
        }
        return nil
    }

    /// Cherche la limite de la voie la plus proche (rayon 20 m) : `maxspeed`
    /// OSM si présent, sinon limite légale française selon le type de voie.
    nonisolated private static func fetchLimit(around c: CLLocationCoordinate2D) async -> Int? {
        let q = "[out:json][timeout:8];"
            + "way(around:20,\(c.latitude),\(c.longitude))[highway];"
            + "out tags 5;"
        var comps = URLComponents(string: "https://overpass-api.de/api/interpreter")!
        comps.queryItems = [URLQueryItem(name: "data", value: q)]
        guard let url = comps.url else { return nil }
        var req = URLRequest(url: url)
        req.timeoutInterval = 8
        guard let (data, _) = try? await URLSession.shared.data(for: req) else { return nil }

        struct Resp: Decodable { let elements: [Element] }
        struct Element: Decodable { let tags: [String: String]? }
        guard let resp = try? JSONDecoder().decode(Resp.self, from: data) else { return nil }
        // Priorité aux voies avec maxspeed explicite, puis défauts français.
        for el in resp.elements {
            if let raw = el.tags?["maxspeed"], let v = parseMaxspeed(raw) { return v }
        }
        for el in resp.elements {
            if let v = limit(fromTags: el.tags) { return v }
        }
        return nil
    }

    /// Limite d'une voie : `maxspeed` si présent, sinon défaut français.
    nonisolated private static func limit(fromTags tags: [String: String]?) -> Int? {
        guard let tags else { return nil }
        if let raw = tags["maxspeed"], let v = parseMaxspeed(raw) { return v }
        if let highway = tags["highway"] { return frenchDefault(forHighway: highway) }
        return nil
    }

    /// Limites légales françaises 🇫🇷 par type de voie OSM, appliquées quand
    /// `maxspeed` n'est pas renseigné.
    nonisolated private static func frenchDefault(forHighway highway: String) -> Int? {
        switch highway {
        case "motorway": return 130
        case "trunk": return 110
        case "primary", "secondary", "tertiary", "unclassified": return 80
        case "motorway_link", "trunk_link", "primary_link", "secondary_link": return 70
        case "residential": return 50
        case "living_street": return 20
        case "service": return 30
        default: return nil
        }
    }
}
