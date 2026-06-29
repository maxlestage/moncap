import Foundation

/// Petit client HTTP vers le backend Axum.
struct APIClient {
    /// Adresse du backend. Par défaut l'app déployée sur Heroku ;
    /// pour un backend local, mets `http://localhost:3000`.
    var baseURL = URL(string: "https://moncap-c41a5aaf07e8.herokuapp.com")!

    /// URL WebSocket dérivée de `baseURL` (http→ws, https→wss).
    var wsURL: URL {
        var comps = URLComponents(url: baseURL.appendingPathComponent("ws"),
                                  resolvingAgainstBaseURL: false)!
        comps.scheme = baseURL.scheme == "https" ? "wss" : "ws"
        return comps.url!
    }

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

    /// PUT /positions/:id — met à jour une position.
    func update(id: Int, _ position: NewPosition) async throws -> Position {
        var req = URLRequest(url: baseURL.appendingPathComponent("positions/\(id)"))
        req.httpMethod = "PUT"
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

    /// POST /positions/import — importe des positions depuis un document GPX.
    func importGPX(_ gpx: String) async throws -> [Position] {
        var req = URLRequest(url: baseURL.appendingPathComponent("positions/import"))
        req.httpMethod = "POST"
        req.setValue("application/gpx+xml", forHTTPHeaderField: "Content-Type")
        req.httpBody = gpx.data(using: .utf8)
        let (data, _) = try await URLSession.shared.data(for: req)
        return try JSONDecoder().decode([Position].self, from: data)
    }

    /// GET /stats — vue d'ensemble des positions.
    func stats() async throws -> Stats {
        let (data, _) = try await URLSession.shared.data(from: baseURL.appendingPathComponent("stats"))
        return try JSONDecoder().decode(Stats.self, from: data)
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

    /// GET /positions.gpx — télécharge l'export GPX dans un fichier temporaire.
    func exportGPX() async throws -> URL {
        let (data, _) = try await URLSession.shared.data(from: baseURL.appendingPathComponent("positions.gpx"))
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("moncap.gpx")
        try data.write(to: url)
        return url
    }
}
