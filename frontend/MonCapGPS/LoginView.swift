import SwiftUI

/// Écran de connexion / inscription.
struct LoginView: View {
    @ObservedObject var auth: AuthStore

    @State private var signupMode = false
    @State private var username = ""
    @State private var password = ""
    @State private var errorMessage = ""
    /// Message d'information (vert) : « lien envoyé », par opposition aux
    /// erreurs rouges.
    @State private var infoMessage = ""
    @State private var busy = false
    private let api = APIClient()

    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Text("🛰️ MonCap GPS").font(.largeTitle.bold())
            Text(signupMode ? "Créer un compte" : "Connexion")
                .foregroundStyle(.secondary)

            // L'e-mail comme identifiant — choix de produit assumé, et la
            // politique de confidentialité le dit avec ses limites : aucun
            // envoi, aucune revente. Champs exclus du .textCase(.lowercase)
            // de l'écran : la saisie s'affiche telle qu'elle est tapée
            // (l'auth est sensible à la casse).
            TextField("e-mail", text: $username)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.emailAddress)
                .textContentType(.emailAddress)
                .textFieldStyle(.roundedBorder)
                .textCase(nil)

            // `.newPassword` à l'inscription : le trousseau propose un mot de
            // passe fort et le retient — `.password` à la connexion pour
            // qu'il ressorte le bon.
            SecureField("mot de passe", text: $password)
                .textContentType(signupMode ? .newPassword : .password)
                .textFieldStyle(.roundedBorder)
                .textCase(nil)

            if signupMode {
                Text("Ton e-mail sert d'identifiant — il ne sert qu'à ça, et à retrouver ton mot de passe.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }

            if !errorMessage.isEmpty {
                Text(LocalizedStringKey(errorMessage)).font(.footnote).foregroundStyle(.red)
            }
            if !infoMessage.isEmpty {
                Text(LocalizedStringKey(infoMessage)).font(.footnote).foregroundStyle(.green)
            }

            Button(action: submit) {
                if busy {
                    ProgressView()
                } else {
                    Text(signupMode ? "S'inscrire" : "Se connecter")
                        .frame(maxWidth: .infinity)
                }
            }
            .buttonStyle(.borderedProminent)
            .disabled(busy || username.isEmpty || password.isEmpty)

            Button(signupMode ? "Déjà un compte ? Se connecter" : "Pas de compte ? S'inscrire") {
                signupMode.toggle()
                errorMessage = ""
                infoMessage = ""
            }
            .font(.footnote)

            // Le lien de réinitialisation part par courriel : l'adresse tapée
            // au-dessus suffit. La réponse est la même que le compte existe
            // ou non — l'écran ne peut pas servir à deviner qui est inscrit.
            if !signupMode {
                Button("Mot de passe oublié ?") { motDePasseOublie() }
                    .font(.footnote)
                    .disabled(busy)
            }

            Spacer()
        }
        .padding(24)
        // Tout le texte affiché de l'écran (titres, libellés, boutons,
        // placeholders) est rendu en minuscules. La saisie de l'utilisateur
        // n'est pas modifiée : l'e-mail garde sa casse réelle.
        .textCase(.lowercase)
    }

    /// Demande le lien de réinitialisation pour l'adresse saisie.
    private func motDePasseOublie() {
        let email = AuthStore.normalize(username)
        guard AuthStore.isValidEmail(email) else {
            errorMessage = "Adresse e-mail invalide."
            infoMessage = ""
            return
        }
        busy = true
        errorMessage = ""
        infoMessage = ""
        Task {
            do {
                try await api.motDePasseOublie(email: email)
                infoMessage = "Si un compte existe avec cette adresse, un lien vient d'être envoyé. Regarde ta boîte (et les indésirables)."
            } catch APIError.server(let msg) {
                errorMessage = msg
            } catch {
                errorMessage = "Serveur injoignable. Réessaie."
            }
            busy = false
        }
    }

    private func submit() {
        busy = true
        errorMessage = ""
        infoMessage = ""
        Task {
            do {
                if signupMode {
                    try await auth.signup(username, password)
                } else {
                    try await auth.login(username, password)
                }
            } catch let e as AuthError {
                errorMessage = e.errorDescription ?? "Saisie invalide."
            } catch APIError.unauthorized {
                errorMessage = signupMode
                    ? "Impossible de créer le compte."
                    : "E-mail ou mot de passe incorrect."
            } catch APIError.server(let msg) {
                errorMessage = msg.isEmpty ? "Échec de l'authentification" : msg
            } catch {
                errorMessage = "Serveur injoignable. Réessaie."
            }
            busy = false
        }
    }
}
