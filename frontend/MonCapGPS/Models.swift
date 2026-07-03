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

/// Un trajet parcouru et enregistré en base.
struct Trip: Codable, Identifiable {
    let id: Int
    let label: String
    let distance_km: Double
    let duration_min: Double
    /// Tracé encodé `lat,lon;lat,lon;…`.
    let polyline: String
    /// Date d'enregistrement (secondes Unix).
    let created_at: Double

    var date: Date { Date(timeIntervalSince1970: created_at) }

    /// Décode le tracé en coordonnées exploitables par MapKit.
    var coordinates: [Coord] {
        polyline.split(separator: ";").compactMap { part in
            let xy = part.split(separator: ",")
            guard xy.count == 2, let lat = Double(xy[0]), let lon = Double(xy[1]) else {
                return nil
            }
            return Coord(lat: lat, lon: lon)
        }
    }
}

/// Données envoyées pour enregistrer un trajet parcouru.
struct NewTrip: Codable {
    let label: String
    let points: [Coord]
    let duration_min: Double
}

/// Une recherche de destination récente (mémorisée sur l'appareil).
struct RecentSearch: Codable, Identifiable {
    var id: String { "\(name)|\(lat),\(lon)" }
    let name: String
    let subtitle: String
    let lat: Double
    let lon: Double
}

/// Réponse « position la plus proche ».
struct NearestResponse: Codable {
    let position: Position
    let distance_km: Double
}

/// Boîte englobante des positions.
struct BBox: Codable {
    let min_lat: Double
    let min_lon: Double
    let max_lat: Double
    let max_lon: Double
}

/// Vue d'ensemble des positions.
struct Stats: Codable {
    let count: Int
    let total_km: Double
    let bbox: BBox?
    let centroid: Coord?
}

// MARK: - Temps réel (WebSocket)

/// Position GPS en direct d'un utilisateur (voiture).
struct LiveUser: Identifiable {
    let id: Int
    let lat: Double
    let lon: Double
    let label: String
    var avatar: String = "green"
    var lastSeen: Date = Date()
}

/// Signalement façon Waze.
struct Alert: Identifiable, Codable {
    let id: Int
    let category: String
    let lat: Double
    let lon: Double
    let label: String
    let ts: Double
}

/// Événement reçu du serveur via WebSocket (enum « tagué » par `kind`).
enum ServerEvent {
    case positionsChanged
    case live(LiveUser)
    case liveGone(Int)
    case alert(Alert)
    case alerts([Alert])
}

extension ServerEvent: Decodable {
    enum CodingKeys: String, CodingKey {
        case kind, id, lat, lon, label, alerts, avatar
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "positions_changed":
            self = .positionsChanged
        case "live":
            self = .live(
                LiveUser(
                    id: try c.decode(Int.self, forKey: .id),
                    lat: try c.decode(Double.self, forKey: .lat),
                    lon: try c.decode(Double.self, forKey: .lon),
                    label: try c.decode(String.self, forKey: .label),
                    avatar: (try? c.decode(String.self, forKey: .avatar)) ?? "green"
                ))
        case "live_gone":
            self = .liveGone(try c.decode(Int.self, forKey: .id))
        case "alert":
            // Les champs de l'alerte sont au même niveau que `kind`.
            self = .alert(try Alert(from: decoder))
        case "alerts":
            self = .alerts(try c.decode([Alert].self, forKey: .alerts))
        default:
            self = .positionsChanged
        }
    }
}
