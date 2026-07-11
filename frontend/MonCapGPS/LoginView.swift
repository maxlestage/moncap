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

            // Champs exclus du .textCase(.lowercase) de l'écran : le texte
            // saisi s'affiche exactement tel qu'il est tapé (casse respectée),
            // sans quoi le champ montrait « max@x.com » pour « Max@X.com ».
            // Placeholders déjà en minuscules pour garder l'aspect « en petit ».
            TextField("e-mail", text: $username)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.emailAddress)
                .textContentType(.emailAddress)
                .textFieldStyle(.roundedBorder)
                .textCase(nil)

            SecureField("mot de passe", text: $password)
                .textFieldStyle(.roundedBorder)
                .textCase(nil)

            if !errorMessage.isEmpty {
                Text(errorMessage).font(.footnote).foregroundStyle(.red)
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
