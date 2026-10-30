import Foundation

/// Session locale : jeton et nom d'utilisateur (persistés).
enum Session {
    private static let tokenKey = "moncap.token"
    private static let userKey = "moncap.username"

    private static let avatarKey = "moncap.avatar"

    static var token: String { UserDefaults.standard.string(forKey: tokenKey) ?? "" }
    static var username: String { UserDefaults.standard.string(forKey: userKey) ?? "" }
    static var isAuthenticated: Bool { !token.isEmpty }

    /// Le nom choisi dans les réglages, montré à ceux qui suivent la
    /// position (carte partagée, lien de suivi).
    static let nomKey = "moncap.nom"
    /// Longueur maximale d'un nom : de quoi tenir un prénom et un nom (le
    /// serveur, lui, coupe à 120).
    static let nomMax = 40

    /// Le nom à montrer aux autres : celui choisi, sinon l'e-mail d'un compte
    /// e-mail — jamais l'identifiant tiré au hasard du compte anonyme. Vide
    /// quand il n'y a rien à montrer : à l'appelant de dire « Moi » ou
    /// « Un ami ».
    static func nomAffiche(_ saisi: String) -> String {
        let nom = saisi.trimmingCharacters(in: .whitespacesAndNewlines)
        if !nom.isEmpty { return String(nom.prefix(nomMax)) }
        if username.isEmpty || CompteAnonyme.estAnonyme(username) { return "" }
        return username
    }

    static var avatar: String {
        get { UserDefaults.standard.string(forKey: avatarKey) ?? "green" }
        set { UserDefaults.standard.set(newValue, forKey: avatarKey) }
    }

    // MARK: - Favoris Domicile / Travail

    private static func favorite(forKey key: String) -> FavoritePlace? {
        guard let data = UserDefaults.standard.data(forKey: key) else { return nil }
        return try? JSONDecoder().decode(FavoritePlace.self, from: data)
    }

    private static func setFavorite(_ place: FavoritePlace?, forKey key: String) {
        if let place, let data = try? JSONEncoder().encode(place) {
            UserDefaults.standard.set(data, forKey: key)
        } else {
            UserDefaults.standard.removeObject(forKey: key)
        }
    }

    static var home: FavoritePlace? {
        get { favorite(forKey: "moncap.home") }
        set { setFavorite(newValue, forKey: "moncap.home") }
    }

    static var work: FavoritePlace? {
        get { favorite(forKey: "moncap.work") }
        set { setFavorite(newValue, forKey: "moncap.work") }
    }

    // MARK: - Destinations

    /// L'ancienne liste des réglages (douze au plus), reprise une fois.
    private static let recentsKey = "moncap.recents"

    /// Toutes les destinations choisies, les plus récentes d'abord — gardées
    /// sur l'appareil dans SQLite (voir `Destinations`).
    static var recentSearches: [RecentSearch] {
        repriseAnciennes()
        return Destinations.shared.toutes()
    }

    /// Enregistre une destination : en tête, sans doublon, sans plafond.
    static func addRecent(_ r: RecentSearch) {
        repriseAnciennes()
        Destinations.shared.enregistre(r)
    }

    /// Fusionne la liste du serveur, sans rien effacer de l'appareil.
    static func mergeRecents(_ distantes: [RecentSearch]) {
        repriseAnciennes()
        Destinations.shared.fusionne(distantes)
    }

    static func removeRecent(_ r: RecentSearch) {
        Destinations.shared.supprime(r)
    }

    static func clearRecents() {
        UserDefaults.standard.removeObject(forKey: recentsKey)
        Destinations.shared.efface()
    }

    /// Les douze de l'ancienne version passent dans SQLite, une seule fois.
    private static func repriseAnciennes() {
        guard let data = UserDefaults.standard.data(forKey: recentsKey) else { return }
        UserDefaults.standard.removeObject(forKey: recentsKey)
        if let anciennes = try? JSONDecoder().decode([RecentSearch].self, from: data) {
            // Les plus anciennes d'abord, sans leur identifiant : il venait
            // de l'ancien serveur.
            for r in anciennes.reversed() {
                Destinations.shared.enregistre(RecentSearch(
                    name: r.name, subtitle: r.subtitle, lat: r.lat, lon: r.lon))
            }
        }
    }

    static func save(token: String, username: String) {
        UserDefaults.standard.set(token, forKey: tokenKey)
        UserDefaults.standard.set(username, forKey: userKey)
    }

    static func clear() {
        UserDefaults.standard.removeObject(forKey: tokenKey)
        UserDefaults.standard.removeObject(forKey: userKey)
    }

    /// Émise quand le backend rejette le jeton (401) : jeton expiré ou
    /// invalidé. L'app efface la session et revient à l'écran de connexion.
    static let expiredNotification = Notification.Name("moncap.session.expired")

    /// Invalide la session locale et prévient l'app (une seule fois) qu'il faut
    /// se reconnecter. Ne fait rien si aucune session n'est active (ex. échec
    /// de connexion, où l'utilisateur n'était pas encore authentifié).
    static func markExpired() {
        guard isAuthenticated else { return }
        clear()
        NotificationCenter.default.post(name: expiredNotification, object: nil)
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
            Session.markExpired()
            throw APIError.unauthorized
        }
        return try JSONDecoder().decode(T.self, from: data)
    }

    /// Envoie une requête sans corps de réponse à décoder (DELETE, etc.) en
    /// gérant le 401 comme les autres appels (retour à l'écran de connexion).
    @discardableResult
    private func sendRaw(_ req: URLRequest) async throws -> Data {
        let (data, resp) = try await URLSession.shared.data(for: req)
        if let http = resp as? HTTPURLResponse, http.statusCode == 401 {
            Session.markExpired()
            throw APIError.unauthorized
        }
        return data
    }

    // MARK: - Authentification

    func signup(username: String, password: String) async throws -> AuthResponse {
        try await authenticate("auth/signup", username, password)
    }

    func login(username: String, password: String) async throws -> AuthResponse {
        try await authenticate("auth/login", username, password)
    }

    /// Demande un lien de réinitialisation. Le serveur répond 204 que le
    /// compte existe ou non (il ne dit jamais si une adresse a un compte),
    /// et 503 quand l'envoi de courriels n'est pas configuré.
    func motDePasseOublie(email: String) async throws {
        var req = URLRequest(url: baseURL.appendingPathComponent("auth/forgot"))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONEncoder().encode(["username": email])
        let (data, resp) = try await URLSession.shared.data(for: req)
        guard let http = resp as? HTTPURLResponse else {
            throw APIError.server("Serveur injoignable. Réessaie.")
        }
        // Notre 503 à nous a un corps vide ; celui d'Heroku (dyno à l'arrêt)
        // est une page HTML — c'est alors le serveur entier qui manque.
        if http.statusCode == 503 && data.isEmpty {
            throw APIError.server("La réinitialisation n'est pas encore configurée.")
        }
        guard (200..<300).contains(http.statusCode) else {
            throw APIError.server(Self.messageDeRefus(data, http))
        }
    }

    /// Ce qu'on peut montrer d'un refus. Nos erreurs sont du texte brut court,
    /// en français, affichable tel quel ; tout le reste — page HTML d'Heroku
    /// quand le dyno est arrêté, 5xx, corps vide — devient un message clair
    /// plutôt qu'un pavé de balises à l'écran.
    static func messageDeRefus(_ data: Data, _ http: HTTPURLResponse) -> String {
        if http.statusCode >= 500 {
            return "Serveur indisponible pour le moment. Réessaie dans quelques minutes."
        }
        let type = http.value(forHTTPHeaderField: "Content-Type") ?? ""
        if type.hasPrefix("text/plain"), let texte = String(data: data, encoding: .utf8),
            !texte.isEmpty, texte.count <= 200
        {
            return texte
        }
        return "Échec de l'authentification"
    }

    private func authenticate(_ path: String, _ username: String, _ password: String) async throws
        -> AuthResponse
    {
        var req = URLRequest(url: baseURL.appendingPathComponent(path))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONEncoder().encode(["username": username, "password": password])
        let (data, resp) = try await URLSession.shared.data(for: req)
        guard let http = resp as? HTTPURLResponse else {
            throw APIError.server("Serveur injoignable. Réessaie.")
        }
        // 401 sur /login = identifiants incorrects (message clair côté vue).
        if http.statusCode == 401 { throw APIError.unauthorized }
        guard (200..<300).contains(http.statusCode) else {
            throw APIError.server(Self.messageDeRefus(data, http))
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
        try await sendRaw(authedRequest("positions/\(id)", method: "DELETE"))
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

    // MARK: - Trajets parcourus (historique)

    func trips() async throws -> [Trip] {
        try await send(authedRequest("trips"))
    }

    func saveTrip(_ trip: NewTrip) async throws -> Trip {
        try await send(
            authedRequest(
                "trips", method: "POST",
                body: try JSONEncoder().encode(trip), contentType: "application/json"))
    }

    /// Requalifie un trajet : nature professionnelle, motif, ou les deux.
    ///
    /// Les deux champs sont indépendants et n'entrent dans la charge que
    /// lorsqu'ils sont demandés : cocher « professionnel » ne doit pas
    /// effacer un motif déjà écrit, et le serveur ne touche qu'à ce qu'on
    /// lui envoie.
    func majTrip(id: Int, pro: Bool? = nil, motif: String? = nil) async throws -> Trip {
        var champs: [String: Any] = [:]
        if let pro { champs["pro"] = pro }
        if let motif { champs["motif"] = motif }
        return try await send(
            authedRequest(
                "trips/\(id)", method: "PATCH",
                body: try JSONSerialization.data(withJSONObject: champs),
                contentType: "application/json"))
    }

    func deleteTrip(id: Int) async throws {
        try await sendRaw(authedRequest("trips/\(id)", method: "DELETE"))
    }

    // MARK: - Recherches récentes (synchronisées)

    func searches() async throws -> [RecentSearch] {
        try await send(authedRequest("searches"))
    }

    func saveSearch(_ s: NewSearch) async throws -> RecentSearch {
        try await send(
            authedRequest(
                "searches", method: "POST",
                body: try JSONEncoder().encode(s), contentType: "application/json"))
    }

    func deleteSearch(id: Int) async throws {
        try await sendRaw(authedRequest("searches/\(id)", method: "DELETE"))
    }

    func clearSearches() async throws {
        try await sendRaw(authedRequest("searches", method: "DELETE"))
    }

    // MARK: - Compte

    /// Page de politique de confidentialité (servie par le backend).
    var privacyURL: URL { baseURL.appendingPathComponent("privacy") }

    /// Modèle d'adresse des tuiles relayées par notre serveur, à coller dans
    /// le réglage des cartes hors ligne. Les accolades doivent rester telles
    /// quelles : c'est MapKit qui les remplace.
    var tilesTemplate: String { baseURL.absoluteString + "/tiles/{z}/{x}/{y}.png" }

    /// Style vectoriel servi par notre backend, à charger par MapLibre.
    var tilesStyleURL: String { baseURL.absoluteString + "/tiles/style.json" }

    func accountInfo() async throws -> AccountInfo {
        try await send(authedRequest("account"))
    }

    /// Les préférences du compte : Domicile et Travail suivent le compte,
    /// pas l'appareil — un nouvel iPhone les retrouve.
    struct Prefs: Codable {
        var home: FavoritePlace?
        var work: FavoritePlace?
    }

    func prefs() async throws -> Prefs {
        try await send(authedRequest("prefs"))
    }

    /// La portabilité RGPD : tout ce que le compte détient, en un JSON brut.
    func exportData() async throws -> Data {
        try await sendRaw(authedRequest("account/export"))
    }

    func savePrefs(_ p: Prefs) async throws {
        let corps = try JSONEncoder().encode(p)
        _ = try await sendRaw(
            authedRequest(
                "prefs", method: "PUT", body: corps,
                contentType: "application/json"))
    }

    /// Supprime définitivement le compte et toutes ses données.
    func deleteAccount() async throws {
        try await sendRaw(authedRequest("account", method: "DELETE"))
    }

    /// Top des contributeurs (noms pseudonymisés).
    func leaderboard() async throws -> [LeaderEntry] {
        try await send(authedRequest("leaderboard"))
    }

    // MARK: - Signalements (persistés, votes)

    func alerts() async throws -> [Alert] {
        try await send(authedRequest("alerts"))
    }

    /// Vote « toujours là » (up) ou « plus là » sur un signalement.
    func voteAlert(id: Int, up: Bool) async throws {
        try await sendRaw(
            authedRequest(
                "alerts/\(id)/vote", method: "POST",
                body: try JSONEncoder().encode(["up": up]), contentType: "application/json"))
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
