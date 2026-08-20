import Foundation

/// Erreurs de saisie détectées côté app, avant tout appel réseau. Les mêmes
/// règles que le serveur (pseudo ≥ 3, mot de passe ≥ 6) : ce que l'app
/// laisse passer, le serveur l'accepte.
enum AuthError: LocalizedError {
    case invalidUsername
    case weakPassword

    var errorDescription: String? {
        switch self {
        case .invalidUsername: return "Pseudonyme : 3 caractères minimum."
        case .weakPassword: return "Mot de passe : 6 caractères minimum."
        }
    }
}

/// État d'authentification observable par l'UI.
@MainActor
final class AuthStore: ObservableObject {
    @Published var isAuthenticated = Session.isAuthenticated
    @Published var username = Session.username

    private let api = APIClient()
    private var expiredObserver: NSObjectProtocol?

    init() {
        // Jeton rejeté par le backend (expiré ou invalidé) : on revient
        // automatiquement à l'écran de connexion au lieu de rester bloqué
        // dans un état « connecté » où tous les appels échouent.
        expiredObserver = NotificationCenter.default.addObserver(
            forName: Session.expiredNotification, object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in self?.handleExpired() }
        }
    }

    deinit {
        if let expiredObserver {
            NotificationCenter.default.removeObserver(expiredObserver)
        }
    }

    private func handleExpired() {
        username = ""
        isAuthenticated = false
    }

    func login(_ identifiant: String, _ password: String) async throws {
        let user = Self.normalize(identifiant)
        guard !user.isEmpty, !password.isEmpty else { throw AuthError.invalidUsername }
        let auth = try await api.login(username: user, password: password)
        username = auth.username
        isAuthenticated = true
    }

    /// L'inscription demande un **pseudonyme**, pas un e-mail : le serveur
    /// n'a jamais stocké d'adresse, et la politique de confidentialité le
    /// promet noir sur blanc. Le formulaire exigeait pourtant un e-mail —
    /// une validation de trop, qui contredisait la promesse. Les comptes
    /// historiques créés avec un e-mail continuent de se connecter : la
    /// connexion ne valide pas la forme de l'identifiant.
    func signup(_ identifiant: String, _ password: String) async throws {
        let user = Self.normalize(identifiant)
        guard user.count >= 3 else { throw AuthError.invalidUsername }
        guard password.count >= 6 else { throw AuthError.weakPassword }
        let auth = try await api.signup(username: user, password: password)
        username = auth.username
        isAuthenticated = true
    }

    func logout() {
        Session.clear()
        username = ""
        isAuthenticated = false
    }

    /// Nettoie un identifiant : espaces superflus retirés. La casse est
    /// conservée car l'auth est sensible à la casse côté backend.
    static func normalize(_ s: String) -> String {
        s.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
