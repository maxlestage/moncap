import Foundation

/// Petit client HTTP vers le backend Axum.
struct APIClient {
    /// Adresse du backend. En simulateur iOS, localhost pointe vers le Mac.
    var baseURL = URL(string: "http://localhost:3000")!

    /// GET /positions
    func positions() async throws -> [Position] {
        let (data, _) = try await URLSession.shared.data(from: baseURL.appendingPathComponent("positions"))
        return try JSONDecoder().decode([Position].self, from: data)
    }

    /// POST /positions
    func add(_ position: Position) async throws -> Position {
        var req = URLRequest(url: baseURL.appendingPathComponent("positions"))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONEncoder().encode(position)
        let (data, _) = try await URLSession.shared.data(for: req)
        return try JSONDecoder().decode(Position.self, from: data)
    }

    /// POST /route
    func route(from: Coord, to: Coord) async throws -> RouteResponse {
        var req = URLRequest(url: baseURL.appendingPathComponent("route"))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONEncoder().encode(RouteRequest(from: from, to: to))
        let (data, _) = try await URLSession.shared.data(for: req)
        return try JSONDecoder().decode(RouteResponse.self, from: data)
    }
}
