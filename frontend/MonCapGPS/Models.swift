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

/// Requête d'itinéraire à plusieurs points.
struct MultiRouteRequest: Codable {
    let points: [Coord]
}

/// Réponse d'itinéraire : distance totale, détail par segment, durée estimée.
struct MultiRouteResponse: Codable {
    let total_km: Double
    let legs_km: [Double]
    let duration_min: Double
}

/// Réponse « position la plus proche ».
struct NearestResponse: Codable {
    let position: Position
    let distance_km: Double
}
