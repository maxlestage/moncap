import CoreLocation
import Foundation

/// Itinéraire en cours mémorisé sur l'appareil.
///
/// Un itinéraire ne peut être calculé qu'en ligne. En le conservant, on peut
/// reprendre une navigation interrompue — application relancée, téléphone
/// redémarré — **même sans réseau**, puisque le guidage lui-même (distances,
/// consignes, annonces) est calculé localement.
struct SavedRoute: Codable, Sendable {
    struct Point: Codable, Sendable {
        let lat: Double
        let lon: Double
    }
    /// Une étape : consigne du moteur d'itinéraire + point de la manœuvre.
    struct Step: Codable, Sendable {
        let text: String
        let lat: Double
        let lon: Double
    }

    let destination: Point
    let coords: [Point]
    let steps: [Step]
    let km: Double
    let minutes: Double
    let savedAt: Date

    var destinationCoordinate: CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: destination.lat, longitude: destination.lon)
    }
    var coordinates: [CLLocationCoordinate2D] {
        coords.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
    }
    var navSteps: [NavStep] {
        steps.map {
            NavStep(text: $0.text, coord: CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon))
        }
    }
}

/// Lecture et écriture de l'itinéraire mémorisé (un seul fichier JSON).
enum RouteStore {
    /// Au-delà, l'itinéraire est considéré périmé : reprendre un trajet de la
    /// veille n'aurait pas de sens.
    private static let maxAge: TimeInterval = 6 * 3600

    private static var fileURL: URL? {
        FileManager.default
            .urls(for: .documentDirectory, in: .userDomainMask).first?
            .appendingPathComponent("route-en-cours.json")
    }

    static func save(
        destination: CLLocationCoordinate2D, coords: [CLLocationCoordinate2D],
        steps: [NavStep], km: Double, minutes: Double
    ) {
        guard let fileURL, !coords.isEmpty else { return }
        let route = SavedRoute(
            destination: .init(lat: destination.latitude, lon: destination.longitude),
            coords: coords.map { .init(lat: $0.latitude, lon: $0.longitude) },
            steps: steps.map {
                .init(text: $0.text, lat: $0.coord.latitude, lon: $0.coord.longitude)
            },
            km: km, minutes: minutes, savedAt: Date())
        // Écriture hors du thread principal : un long trajet représente des
        // milliers de points, et l'enregistrement a lieu au démarrage de la
        // navigation, au pire moment pour une saccade.
        Task.detached(priority: .utility) {
            guard let data = try? JSONEncoder().encode(route) else { return }
            try? data.write(to: fileURL, options: .atomic)
        }
    }

    /// Itinéraire mémorisé, s'il existe et n'est pas périmé. Un itinéraire
    /// périmé est effacé au passage.
    static func load() -> SavedRoute? {
        guard let fileURL,
            let data = try? Data(contentsOf: fileURL),
            let route = try? JSONDecoder().decode(SavedRoute.self, from: data)
        else { return nil }
        guard Date().timeIntervalSince(route.savedAt) < maxAge else {
            clear()
            return nil
        }
        return route
    }

    static func clear() {
        guard let fileURL else { return }
        try? FileManager.default.removeItem(at: fileURL)
    }
}
