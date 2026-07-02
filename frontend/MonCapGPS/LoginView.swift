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

            TextField("E-mail", text: $username)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.emailAddress)
                .textContentType(.emailAddress)
                .textFieldStyle(.roundedBorder)

            SecureField("Mot de passe", text: $password)
                .textFieldStyle(.roundedBorder)

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
            } catch APIError.server(let msg) {
                errorMessage = msg.isEmpty ? "Échec de l'authentification" : msg
            } catch {
                errorMessage = "Échec de l'authentification"
            }
            busy = false
        }
    }
}
