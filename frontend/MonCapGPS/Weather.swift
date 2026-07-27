import CoreLocation
import Foundation

/// Météo en temps réel via l'API open-meteo (gratuite, sans clé). Température
/// courante + code temps (WMO). Best effort : silencieux en cas d'échec.
@MainActor
final class WeatherService: ObservableObject {
    /// Température courante en °C, nil si inconnue.
    @Published private(set) var temperature: Int?
    /// Code temps WMO courant, nil si inconnu.
    @Published private(set) var code: Int?

    private var lastFetch = Date.distantPast
    private var lastCoord: CLLocationCoordinate2D?

    /// À appeler à chaque position : rafraîchit au plus toutes les 10 min, ou
    /// dès qu'on s'est déplacé de plus de 3 km.
    func update(_ c: CLLocationCoordinate2D) {
        let movedFar =
            lastCoord.map {
                CLLocation(latitude: $0.latitude, longitude: $0.longitude)
                    .distance(from: CLLocation(latitude: c.latitude, longitude: c.longitude)) > 3000
            } ?? true
        guard movedFar || Date().timeIntervalSince(lastFetch) > 600 else { return }
        lastFetch = Date()
        lastCoord = c
        Task {
            if let w = await Self.fetch(near: c) {
                temperature = w.temperature
                code = w.code
            }
        }
    }

    /// `nonisolated` : décodage hors du thread principal.
    nonisolated private static func fetch(near c: CLLocationCoordinate2D) async -> (
        temperature: Int, code: Int
    )? {
        var comps = URLComponents(string: "https://api.open-meteo.com/v1/forecast")!
        comps.queryItems = [
            URLQueryItem(name: "latitude", value: String(c.latitude)),
            URLQueryItem(name: "longitude", value: String(c.longitude)),
            URLQueryItem(name: "current", value: "temperature_2m,weather_code"),
        ]
        guard let url = comps.url,
            let (data, _) = try? await URLSession.shared.data(from: url),
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let current = json["current"] as? [String: Any]
        else { return nil }
        let temp = (current["temperature_2m"] as? Double).map { Int($0.rounded()) }
        let code = (current["weather_code"] as? Double).map { Int($0) } ?? (current["weather_code"] as? Int)
        guard let temp, let code else { return nil }
        return (temp, code)
    }

    /// Émoji + libellé court d'après le code temps WMO.
    static func describe(_ code: Int) -> (emoji: String, label: String) {
        switch code {
        case 0: return ("☀️", "Ensoleillé")
        case 1, 2: return ("⛅️", "Éclaircies")
        case 3: return ("☁️", "Couvert")
        case 45, 48: return ("🌫️", "Brouillard")
        case 51, 53, 55, 56, 57: return ("🌦️", "Bruine")
        case 61, 63, 65, 66, 67: return ("🌧️", "Pluie")
        case 71, 73, 75, 77, 85, 86: return ("🌨️", "Neige")
        case 80, 81, 82: return ("🌦️", "Averses")
        case 95, 96, 99: return ("⛈️", "Orage")
        default: return ("🌡️", "Météo")
        }
    }
}
