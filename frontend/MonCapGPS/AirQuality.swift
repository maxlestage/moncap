import CoreLocation
import Foundation
import SwiftUI

/// Qualité de l'air en temps réel via l'API open-meteo Air Quality (gratuite,
/// sans clé). Indice européen (EAQI). Best effort : silencieux en cas d'échec.
@MainActor
final class AirQualityService: ObservableObject {
    /// Indice européen courant (EAQI, ~0-100+), nil si inconnu.
    @Published private(set) var aqi: Int?

    private var lastFetch = Date.distantPast
    private var lastCoord: CLLocationCoordinate2D?

    /// À appeler à chaque position : rafraîchit au plus toutes les 10 min, ou
    /// dès qu'on s'est déplacé de plus de 3 km.
    func update(_ c: CLLocationCoordinate2D) {
        let movedFar = lastCoord.map {
            CLLocation(latitude: $0.latitude, longitude: $0.longitude)
                .distance(from: CLLocation(latitude: c.latitude, longitude: c.longitude)) > 3000
        } ?? true
        guard movedFar || Date().timeIntervalSince(lastFetch) > 600 else { return }
        lastFetch = Date()
        lastCoord = c
        Task {
            if let v = await Self.fetch(near: c) { aqi = v }
        }
    }

    private static func fetch(near c: CLLocationCoordinate2D) async -> Int? {
        var comps = URLComponents(
            string: "https://air-quality-api.open-meteo.com/v1/air-quality")!
        comps.queryItems = [
            URLQueryItem(name: "latitude", value: String(c.latitude)),
            URLQueryItem(name: "longitude", value: String(c.longitude)),
            URLQueryItem(name: "current", value: "european_aqi"),
        ]
        guard let url = comps.url,
            let (data, _) = try? await URLSession.shared.data(from: url),
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let current = json["current"] as? [String: Any]
        else { return nil }
        if let v = current["european_aqi"] as? Double { return Int(v.rounded()) }
        if let v = current["european_aqi"] as? Int { return v }
        return nil
    }

    /// Libellé + couleur selon l'indice européen (EAQI).
    static func level(_ aqi: Int) -> (label: String, color: Color) {
        switch aqi {
        case ..<20: return ("Bon", .green)
        case ..<40: return ("Correct", Color(red: 0.55, green: 0.78, blue: 0.25))
        case ..<60: return ("Moyen", .yellow)
        case ..<80: return ("Mauvais", .orange)
        case ..<100: return ("Très mauvais", .red)
        default: return ("Extrême", .purple)
        }
    }
}
