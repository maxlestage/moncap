import CoreLocation
import Foundation

/// Prix du carburant en France 🇫🇷 via l'open data officiel
/// (prix-des-carburants, data.economie.gouv.fr) — gratuit, sans clé API.
/// Moyenne des stations autour de la position, avec repli national.
@MainActor
final class FuelPriceService: ObservableObject {
    /// €/L pour le carburant choisi (moyenne des stations proches).
    @Published private(set) var pricePerLiter: Double?
    /// Nombre de stations utilisées pour la moyenne.
    @Published private(set) var stationCount = 0

    private var lastFetch = Date.distantPast
    private var lastType = ""

    /// Prix de repli si l'open data est indisponible (ordres de grandeur).
    static let fallback: [String: Double] = [
        "gazole": 1.72, "sp95": 1.79, "sp98": 1.86, "e10": 1.74, "e85": 0.85,
    ]

    /// Prix effectif : moyenne locale si connue, sinon repli national.
    func effectivePrice(type: String) -> Double {
        pricePerLiter ?? Self.fallback[type] ?? 1.80
    }

    /// Rafraîchit le prix moyen autour de la position (cache 6 h par type).
    func refresh(near c: CLLocationCoordinate2D, type: String) {
        if type == lastType, Date().timeIntervalSince(lastFetch) < 6 * 3600 { return }
        lastType = type
        lastFetch = Date()
        Task {
            if let (price, count) = await Self.fetch(near: c, type: type) {
                pricePerLiter = price
                stationCount = count
            } else {
                pricePerLiter = nil
                stationCount = 0
            }
        }
    }

    /// Interroge l'open data : stations dans un rayon de 20 km, moyenne du
    /// prix du carburant demandé.
    private static func fetch(near c: CLLocationCoordinate2D, type: String)
        async -> (Double, Int)?
    {
        var comps = URLComponents(
            string: "https://data.economie.gouv.fr/api/records/1.0/search/")!
        comps.queryItems = [
            URLQueryItem(name: "dataset",
                         value: "prix-des-carburants-en-france-flux-instantane-v2"),
            URLQueryItem(name: "rows", value: "30"),
            URLQueryItem(name: "geofilter.distance",
                         value: "\(c.latitude),\(c.longitude),20000"),
        ]
        guard let url = comps.url,
            let (data, _) = try? await URLSession.shared.data(from: url),
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let records = json["records"] as? [[String: Any]]
        else { return nil }

        var prices: [Double] = []
        for record in records {
            guard let fields = record["fields"] as? [String: Any] else { continue }
            let raw = fields["\(type)_prix"]
            if let v = raw as? Double {
                prices.append(v)
            } else if let s = raw as? String,
                let v = Double(s.replacingOccurrences(of: ",", with: ".")) {
                prices.append(v)
            }
        }
        // Filtre les valeurs aberrantes (saisies en millièmes, etc.).
        prices = prices.filter { $0 > 0.4 && $0 < 3.5 }
        guard !prices.isEmpty else { return nil }
        return (prices.reduce(0, +) / Double(prices.count), prices.count)
    }
}
