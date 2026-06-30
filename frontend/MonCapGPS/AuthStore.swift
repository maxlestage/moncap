import Foundation

/// État d'authentification observable par l'UI.
@MainActor
final class AuthStore: ObservableObject {
    @Published var isAuthenticated = Session.isAuthenticated
    @Published var username = Session.username

    private let api = APIClient()

    func login(_ username: String, _ password: String) async throws {
        let auth = try await api.login(username: username, password: password)
        self.username = auth.username
        isAuthenticated = true
    }

    func signup(_ username: String, _ password: String) async throws {
        let auth = try await api.signup(username: username, password: password)
        self.username = auth.username
        isAuthenticated = true
    }

    func logout() {
        Session.clear()
        username = ""
        isAuthenticated = false
    }
}
