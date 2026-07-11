import Foundation

/// Erreurs de saisie détectées côté app, avant tout appel réseau.
enum AuthError: LocalizedError {
    case invalidEmail
    case weakPassword

    var errorDescription: String? {
        switch self {
        case .invalidEmail: return "Adresse e-mail invalide."
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

    func login(_ email: String, _ password: String) async throws {
        let user = Self.normalize(email)
        guard !user.isEmpty, !password.isEmpty else { throw AuthError.invalidEmail }
        let auth = try await api.login(username: user, password: password)
        username = auth.username
        isAuthenticated = true
    }

    func signup(_ email: String, _ password: String) async throws {
        let user = Self.normalize(email)
        guard Self.isValidEmail(user) else { throw AuthError.invalidEmail }
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

    /// Normalise un identifiant : sans espaces superflus et en minuscules, pour
    /// que la casse de l'e-mail n'empêche jamais la connexion.
    static func normalize(_ s: String) -> String {
        s.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    /// Validation d'e-mail volontairement simple : `local@domaine.tld`.
    static func isValidEmail(_ s: String) -> Bool {
        s.range(of: #"^[^@\s]+@[^@\s]+\.[^@\s]+$"#, options: .regularExpression) != nil
    }
}
