import Foundation

/// Une position renvoyée par le backend (avec son identifiant).
struct Position: Codable, Identifiable {
    let id: Int
    let lat: Double
    let lon: Double
    let label: String
}

/// Données envoyées pour créer une position (sans identifiant).
struct NewPosition: Codable {
    let lat: Double
    let lon: Double
    let label: String
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
