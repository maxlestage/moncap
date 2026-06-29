import Foundation

/// Une position GPS, identique au modèle du backend Rust.
struct Position: Codable, Identifiable {
    var id = UUID()
    let lat: Double
    let lon: Double
    let label: String

    enum CodingKeys: String, CodingKey {
        case lat, lon, label
    }
}

/// Une coordonnée simple envoyée au backend.
struct Coord: Codable {
    let lat: Double
    let lon: Double
}

/// Requête de calcul de trajet.
struct RouteRequest: Codable {
    let from: Coord
    let to: Coord
}

/// Réponse du backend : distance et cap.
struct RouteResponse: Codable {
    let distance_km: Double
    let bearing_deg: Double
}
