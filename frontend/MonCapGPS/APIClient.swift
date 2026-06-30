import Foundation

/// Session locale : jeton et nom d'utilisateur (persistés).
enum Session {
    private static let tokenKey = "moncap.token"
    private static let userKey = "moncap.username"

    private static let avatarKey = "moncap.avatar"

    static var token: String { UserDefaults.standard.string(forKey: tokenKey) ?? "" }
    static var username: String { UserDefaults.standard.string(forKey: userKey) ?? "" }
    static var isAuthenticated: Bool { !token.isEmpty }

    static var avatar: String {
        get { UserDefaults.standard.string(forKey: avatarKey) ?? "green" }
        set { UserDefaults.standard.set(newValue, forKey: avatarKey) }
    }

    static func save(token: String, username: String) {
        UserDefaults.standard.set(token, forKey: tokenKey)
        UserDefaults.standard.set(username, forKey: userKey)
    }

    static func clear() {
        UserDefaults.standard.removeObject(forKey: tokenKey)
        UserDefaults.standard.removeObject(forKey: userKey)
    }
}

/// Réponse d'authentification.
struct AuthResponse: Codable {
    let token: String
    let username: String
}

enum APIError: Error {
    case unauthorized
    case server(String)
}

/// Petit client HTTP vers le backend Axum.
struct APIClient {
    /// Adresse du backend. Par défaut l'app déployée sur Heroku ;
    /// pour un backend local, mets `http://localhost:3000`.
    var baseURL = URL(string: "https://moncap-c41a5aaf07e8.herokuapp.com")!

    /// URL WebSocket avec le jeton en query (pas d'en-tête possible).
    var wsURL: URL {
        var comps = URLComponents(url: baseURL.appendingPathComponent("ws"),
                                  resolvingAgainstBaseURL: false)!
        comps.scheme = baseURL.scheme == "https" ? "wss" : "ws"
        comps.queryItems = [URLQueryItem(name: "token", value: Session.token)]
        return comps.url!
    }

    // MARK: - Helpers

    private func authedRequest(
        _ path: String, method: String = "GET", body: Data? = nil, contentType: String? = nil
    ) -> URLRequest {
        var req = URLRequest(url: baseURL.appendingPathComponent(path))
        req.httpMethod = method
        if !Session.token.isEmpty {
            req.setValue("Bearer \(Session.token)", forHTTPHeaderField: "Authorization")
        }
        if let contentType { req.setValue(contentType, forHTTPHeaderField: "Content-Type") }
        req.httpBody = body
        return req
    }

    private func send<T: Decodable>(_ req: URLRequest) async throws -> T {
        let (data, resp) = try await URLSession.shared.data(for: req)
        if let http = resp as? HTTPURLResponse, http.statusCode == 401 {
            throw APIError.unauthorized
        }
        return try JSONDecoder().decode(T.self, from: data)
    }

    // MARK: - Authentification

    func signup(username: String, password: String) async throws -> AuthResponse {
        try await authenticate("auth/signup", username, password)
    }

    func login(username: String, password: String) async throws -> AuthResponse {
        try await authenticate("auth/login", username, password)
    }

    private func authenticate(_ path: String, _ username: String, _ password: String) async throws
        -> AuthResponse
    {
        var req = URLRequest(url: baseURL.appendingPathComponent(path))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONEncoder().encode(["username": username, "password": password])
        let (data, resp) = try await URLSession.shared.data(for: req)
        guard let http = resp as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw APIError.server(String(data: data, encoding: .utf8) ?? "Échec")
        }
        let auth = try JSONDecoder().decode(AuthResponse.self, from: data)
        Session.save(token: auth.token, username: auth.username)
        return auth
    }

    // MARK: - Positions / itinéraires

    func positions() async throws -> [Position] {
        try await send(authedRequest("positions"))
    }

    func add(_ position: NewPosition) async throws -> Position {
        try await send(
            authedRequest(
                "positions", method: "POST",
                body: try JSONEncoder().encode(position), contentType: "application/json"))
    }

    func update(id: Int, _ position: NewPosition) async throws -> Position {
        try await send(
            authedRequest(
                "positions/\(id)", method: "PUT",
                body: try JSONEncoder().encode(position), contentType: "application/json"))
    }

    func delete(id: Int) async throws {
        _ = try await URLSession.shared.data(for: authedRequest("positions/\(id)", method: "DELETE"))
    }

    func importGPX(_ gpx: String) async throws -> [Position] {
        try await send(
            authedRequest(
                "positions/import", method: "POST",
                body: gpx.data(using: .utf8), contentType: "application/gpx+xml"))
    }

    func stats() async throws -> Stats {
        try await send(authedRequest("stats"))
    }

    func route(from: Coord, to: Coord) async throws -> RouteResponse {
        try await send(
            authedRequest(
                "route", method: "POST",
                body: try JSONEncoder().encode(RouteRequest(from: from, to: to)),
                contentType: "application/json"))
    }

    func multiRoute(_ points: [Coord]) async throws -> MultiRouteResponse {
        try await send(
            authedRequest(
                "route/multi", method: "POST",
                body: try JSONEncoder().encode(MultiRouteRequest(points: points)),
                contentType: "application/json"))
    }

    func nearest(lat: Double, lon: Double) async throws -> NearestResponse {
        var comps = URLComponents(
            url: baseURL.appendingPathComponent("positions/nearest"), resolvingAgainstBaseURL: false)!
        comps.queryItems = [
            URLQueryItem(name: "lat", value: String(lat)),
            URLQueryItem(name: "lon", value: String(lon)),
        ]
        var req = URLRequest(url: comps.url!)
        if !Session.token.isEmpty {
            req.setValue("Bearer \(Session.token)", forHTTPHeaderField: "Authorization")
        }
        return try await send(req)
    }

    /// GET /positions.gpx?token=… — télécharge l'export dans un fichier temporaire.
    func exportGPX() async throws -> URL {
        var comps = URLComponents(
            url: baseURL.appendingPathComponent("positions.gpx"), resolvingAgainstBaseURL: false)!
        comps.queryItems = [URLQueryItem(name: "token", value: Session.token)]
        let (data, _) = try await URLSession.shared.data(from: comps.url!)
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("moncap.gpx")
        try data.write(to: url)
        return url
    }
}
