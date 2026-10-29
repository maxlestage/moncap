import Foundation
import Security

/// Erreurs de saisie détectées côté app, avant tout appel réseau.
enum AuthError: LocalizedError {
    case invalidEmail

    var errorDescription: String? {
        switch self {
        case .invalidEmail: return "Adresse e-mail invalide."
        }
    }
}

/// Le compte anonyme de l'appareil, gardé dans le trousseau.
///
/// Le trousseau plutôt que les réglages : il survit à la désinstallation,
/// si bien qu'une réinstallation retrouve le même compte — ses trajets, ses
/// favoris, et la date de son essai. Pas de synchronisation iCloud : chaque
/// appareil a le sien.
enum CompteAnonyme {
    /// Préfixe des identifiants anonymes : les distingue des e-mails.
    static let prefixe = "appareil-"
    private static let service = "moncap.compte-anonyme"

    static func estAnonyme(_ nom: String) -> Bool { nom.hasPrefix(prefixe) }

    static func lit() -> (nom: String, motDePasse: String)? {
        let requete: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecReturnAttributes as String: true,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var resultat: AnyObject?
        guard SecItemCopyMatching(requete as CFDictionary, &resultat) == errSecSuccess,
            let item = resultat as? [String: Any],
            let nom = item[kSecAttrAccount as String] as? String,
            let donnees = item[kSecValueData as String] as? Data,
            let mdp = String(data: donnees, encoding: .utf8)
        else { return nil }
        return (nom, mdp)
    }

    static func enregistre(nom: String, motDePasse: String) {
        efface()
        let item: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: nom,
            kSecValueData as String: Data(motDePasse.utf8),
            // L'app peut être réveillée en arrière-plan (téléchargements de
            // fond) : le trousseau doit rester lisible après le premier
            // déverrouillage.
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        SecItemAdd(item as CFDictionary, nil)
    }

    static func efface() {
        let requete: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
        ]
        SecItemDelete(requete as CFDictionary)
    }

    /// Un nouveau couple identifiant / mot de passe, tirés au hasard.
    static func nouveau() -> (nom: String, motDePasse: String) {
        var octets = [UInt8](repeating: 0, count: 32)
        _ = SecRandomCopyBytes(kSecRandomDefault, octets.count, &octets)
        let mdp = octets.map { String(format: "%02x", $0) }.joined()
        return (prefixe + UUID().uuidString.lowercased(), mdp)
    }
}

/// État d'authentification observable par l'UI.
///
/// Plus d'écran de connexion à l'ouverture : sans session, l'app ouvre
/// toute seule celle du compte anonyme de l'appareil (et le crée la première
/// fois). Se connecter à un compte e-mail existant reste possible depuis les
/// réglages ; s'en déconnecter ramène au compte anonyme.
@MainActor
final class AuthStore: ObservableObject {
    @Published var isAuthenticated = Session.isAuthenticated
    @Published var username = Session.username

    /// Le compte en cours est-il celui, anonyme, de l'appareil ?
    var estAnonyme: Bool { CompteAnonyme.estAnonyme(username) }

    private let api = APIClient()
    private var expiredObserver: NSObjectProtocol?
    /// L'ouverture de session anonyme en cours (une seule à la fois).
    private var ouverture: Task<Void, Never>?

    init() {
        // Jeton rejeté par le backend (expiré ou invalidé) : on rouvre
        // aussitôt une session au lieu de rester dans un état « connecté »
        // où tous les appels échouent.
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
        MonCapPlus.shared.oublieCompte()
        assureSession()
    }

    /// Ouvre une session si l'app n'en a pas : celle du compte anonyme.
    /// Sans réseau, réessaie en espaçant (5 s, 10 s… jusqu'à 2 min) ; la
    /// carte reste utilisable pendant ce temps.
    func assureSession() {
        guard !isAuthenticated, ouverture == nil else { return }
        ouverture = Task {
            var attente: UInt64 = 5
            while !isAuthenticated, !Task.isCancelled {
                do {
                    try await ouvreSessionAnonyme()
                } catch {
                    try? await Task.sleep(nanoseconds: attente * 1_000_000_000)
                    attente = min(attente * 2, 120)
                }
            }
            // Une ouverture annulée a déjà été remplacée : ne pas l'effacer.
            if !Task.isCancelled { ouverture = nil }
        }
    }

    private func ouvreSessionAnonyme() async throws {
        if let compte = CompteAnonyme.lit() {
            do {
                adopte(try await api.login(username: compte.nom, password: compte.motDePasse))
                return
            } catch APIError.unauthorized {
                // Le compte n'existe plus côté serveur : on en crée un autre.
                CompteAnonyme.efface()
            }
        }
        // Enregistré avant l'inscription : si la réponse se perd, le compte
        // créé reste joignable au prochain essai.
        let (nom, mdp) = CompteAnonyme.nouveau()
        CompteAnonyme.enregistre(nom: nom, motDePasse: mdp)
        adopte(try await api.signup(username: nom, password: mdp))
    }

    private func adopte(_ auth: AuthResponse) {
        guard !Task.isCancelled else { return }
        if auth.username != username {
            // L'essai et l'invitation suivent le compte, pas l'appareil.
            MonCapPlus.shared.oublieCompte()
        }
        username = auth.username
        isAuthenticated = true
    }

    /// Connexion à un compte e-mail existant, depuis les réglages.
    func login(_ email: String, _ password: String) async throws {
        let user = Self.normalize(email)
        guard !user.isEmpty, !password.isEmpty else { throw AuthError.invalidEmail }
        ouverture?.cancel()
        ouverture = nil
        adopte(try await api.login(username: user, password: password))
    }

    /// Quitte le compte e-mail : l'app revient au compte anonyme.
    func logout() {
        ouverture?.cancel()
        ouverture = nil
        Session.clear()
        username = ""
        isAuthenticated = false
        MonCapPlus.shared.oublieCompte()
        assureSession()
    }

    /// Après la suppression du compte côté serveur. Si c'était le compte
    /// anonyme, l'appareil l'oublie : la prochaine session en crée un neuf.
    func compteSupprime() {
        if estAnonyme { CompteAnonyme.efface() }
        logout()
    }

    /// Nettoie un identifiant : espaces superflus retirés. La casse est
    /// conservée car l'auth est sensible à la casse côté backend.
    static func normalize(_ s: String) -> String {
        s.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Validation d'e-mail volontairement simple : `local@domaine.tld`.
    static func isValidEmail(_ s: String) -> Bool {
        s.range(of: #"^[^@\s]+@[^@\s]+\.[^@\s]+$"#, options: .regularExpression) != nil
    }
}
