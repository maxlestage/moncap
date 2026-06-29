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
    func add(_ position: NewPosition) async throws -> Position {
        var req = URLRequest(url: baseURL.appendingPathComponent("positions"))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONEncoder().encode(position)
        let (data, _) = try await URLSession.shared.data(for: req)
        return try JSONDecoder().decode(Position.self, from: data)
    }

    /// DELETE /positions/:id
    func delete(id: Int) async throws {
        var req = URLRequest(url: baseURL.appendingPathComponent("positions/\(id)"))
        req.httpMethod = "DELETE"
        _ = try await URLSession.shared.data(for: req)
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

    /// POST /route/multi
    func multiRoute(_ points: [Coord]) async throws -> MultiRouteResponse {
        var req = URLRequest(url: baseURL.appendingPathComponent("route/multi"))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONEncoder().encode(MultiRouteRequest(points: points))
        let (data, _) = try await URLSession.shared.data(for: req)
        return try JSONDecoder().decode(MultiRouteResponse.self, from: data)
    }

    /// GET /positions/nearest?lat=&lon=
    func nearest(lat: Double, lon: Double) async throws -> NearestResponse {
        var comps = URLComponents(url: baseURL.appendingPathComponent("positions/nearest"),
                                  resolvingAgainstBaseURL: false)!
        comps.queryItems = [
            URLQueryItem(name: "lat", value: String(lat)),
            URLQueryItem(name: "lon", value: String(lon)),
        ]
        let (data, _) = try await URLSession.shared.data(from: comps.url!)
        return try JSONDecoder().decode(NearestResponse.self, from: data)
    }
}
