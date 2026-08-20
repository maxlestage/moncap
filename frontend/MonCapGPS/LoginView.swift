import SwiftUI

/// Écran de connexion / inscription.
struct LoginView: View {
    @ObservedObject var auth: AuthStore

    @State private var signupMode = false
    @State private var username = ""
    @State private var password = ""
    @State private var errorMessage = ""
    @State private var busy = false

    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Text("🛰️ MonCap GPS").font(.largeTitle.bold())
            Text(signupMode ? "Créer un compte" : "Connexion")
                .foregroundStyle(.secondary)

            // Un pseudonyme, pas un e-mail : le serveur n'a jamais stocké
            // d'adresse, et la confidentialité le promet. Champs exclus du
            // .textCase(.lowercase) de l'écran : la saisie s'affiche telle
            // qu'elle est tapée (l'auth est sensible à la casse).
            TextField("pseudonyme", text: $username)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .textContentType(.username)
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
                Text("Un pseudonyme suffit — aucun e-mail demandé.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }

            if !errorMessage.isEmpty {
                Text(LocalizedStringKey(errorMessage)).font(.footnote).foregroundStyle(.red)
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
            }
            .font(.footnote)

            Spacer()
        }
        .padding(24)
        // Tout le texte affiché de l'écran (titres, libellés, boutons,
        // placeholders) est rendu en minuscules. La saisie de l'utilisateur
        // n'est pas modifiée : l'e-mail garde sa casse réelle.
        .textCase(.lowercase)
    }

    private func submit() {
        busy = true
        errorMessage = ""
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
                    : "Identifiant ou mot de passe incorrect."
            } catch APIError.server(let msg) {
                errorMessage = msg.isEmpty ? "Échec de l'authentification" : msg
            } catch {
                errorMessage = "Serveur injoignable. Réessaie."
            }
            busy = false
        }
    }
}
