import CoreLocation
import MapKit

/// Une cellule de la grille de qualité de l'air (un point échantillonné + son
/// indice européen EAQI).
struct AQICell: Identifiable {
    let id: Int
    let coordinate: CLLocationCoordinate2D
    let aqi: Int
}

/// Couche « qualité de l'air » colorée sur la carte : échantillonne une grille
/// N×N de valeurs EAQI sur la zone visible en **une seule** requête open-meteo
/// (gratuite, sans clé), pour dessiner des zones colorées façon carte de
/// chaleur. Activable / désactivable.
@MainActor
final class AirQualityLayerService: ObservableObject {
    @Published private(set) var cells: [AQICell] = []
    /// Rayon d'un disque de cellule (m) ≈ pas de la grille, pour un rendu
    /// continu (léger recouvrement entre cellules voisines).
    @Published private(set) var radius: Double = 6000

    private var lastKey = ""
    private var lastFetch = Date.distantPast

    /// Vide la couche (désactivation).
    func clear() {
        cells = []
        lastKey = ""
    }

    /// Rafraîchit la grille sur la région visible. Ne refait rien si la zone n'a
    /// pas bougé et que la dernière mesure a moins de 5 min (les indices varient
    /// lentement).
    func refresh(region: MKCoordinateRegion) {
        let key = String(
            format: "%.2f,%.2f,%.2f,%.2f",
            region.center.latitude, region.center.longitude,
            region.span.latitudeDelta, region.span.longitudeDelta)
        guard key != lastKey || Date().timeIntervalSince(lastFetch) > 300 else { return }
        lastKey = key
        lastFetch = Date()

        let n = 8
        // Borne l'étendue pour garder des cellules utiles (évite une grille sur
        // la moitié du globe quand on est dézoomé à fond).
        let latSpan = min(region.span.latitudeDelta, 6.0)
        let lonSpan = min(region.span.longitudeDelta, 8.0)
        let lat0 = region.center.latitude - latSpan / 2
        let lon0 = region.center.longitude - lonSpan / 2
        var coords: [CLLocationCoordinate2D] = []
        for i in 0..<n {
            for j in 0..<n {
                let lat = lat0 + latSpan * Double(i) / Double(n - 1)
                let lon = lon0 + lonSpan * Double(j) / Double(n - 1)
                coords.append(CLLocationCoordinate2D(latitude: lat, longitude: lon))
            }
        }
        // Rayon ≈ pas de la grille × 0,75 → cellules qui se touchent, sans trop
        // se chevaucher (évite de mélanger les couleurs).
        let stepMeters = (latSpan / Double(n - 1)) * 111_320
        let r = stepMeters * 0.75

        Task {
            if let c = await Self.fetch(coords: coords) {
                cells = c
                radius = r
            }
        }
    }

    /// `nonisolated` : décodage hors du thread principal. Une requête open-meteo
    /// groupée renvoie un tableau d'objets (un par point, dans l'ordre).
    nonisolated private static func fetch(coords: [CLLocationCoordinate2D]) async -> [AQICell]? {
        let lats = coords.map { String(format: "%.4f", $0.latitude) }.joined(separator: ",")
        let lons = coords.map { String(format: "%.4f", $0.longitude) }.joined(separator: ",")
        var comps = URLComponents(string: "https://air-quality-api.open-meteo.com/v1/air-quality")!
        comps.queryItems = [
            URLQueryItem(name: "latitude", value: lats),
            URLQueryItem(name: "longitude", value: lons),
            URLQueryItem(name: "current", value: "european_aqi"),
        ]
        guard let url = comps.url,
            let (data, _) = try? await URLSession.shared.data(from: url)
        else { return nil }

        // Plusieurs points → tableau ; un seul → objet.
        let objs: [[String: Any]]
        if let arr = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] {
            objs = arr
        } else if let one = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            objs = [one]
        } else {
            return nil
        }

        var out: [AQICell] = []
        for (i, obj) in objs.enumerated() where i < coords.count {
            guard let current = obj["current"] as? [String: Any] else { continue }
            let v =
                (current["european_aqi"] as? Double).map { Int($0.rounded()) }
                ?? (current["european_aqi"] as? Int)
            guard let aqi = v else { continue }
            out.append(AQICell(id: i, coordinate: coords[i], aqi: aqi))
        }
        return out.isEmpty ? nil : out
    }
}
