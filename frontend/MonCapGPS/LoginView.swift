import SwiftUI

/// Connexion à un compte e-mail existant, ouverte depuis les réglages.
///
/// L'app n'en a plus besoin pour démarrer : elle tourne sur le compte
/// anonyme de l'appareil (voir `AuthStore`). Cette feuille sert à qui a déjà
/// un compte e-mail et veut retrouver ses trajets et son abonnement. Même
/// décor que l'écran de garde (`FondDeMarque`), formulaire dans une carte
/// claire ; le trousseau d'iOS reconnaît la paire identifiant / mot de passe.
struct LoginView: View {
    @ObservedObject var auth: AuthStore
    @Environment(\.dismiss) private var ferme

    private enum Champ { case email, motDePasse }

    @State private var username = ""
    @State private var password = ""
    @State private var motDePasseVisible = false
    @State private var errorMessage = ""
    /// Message d'information (vert) : « lien envoyé », par opposition aux
    /// erreurs rouges.
    @State private var infoMessage = ""
    @State private var busy = false
    @FocusState private var focus: Champ?
    private let api = APIClient()

    private var peutValider: Bool { !busy && !username.isEmpty && !password.isEmpty }

    var body: some View {
        ZStack {
            FondDeMarque()
            ScrollView {
                VStack(spacing: 28) {
                    enTete
                    carte
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 48)
                .frame(maxWidth: 440)
                .frame(maxWidth: .infinity)
            }
            .scrollBounceBehavior(.basedOnSize)
            .scrollDismissesKeyboard(.interactively)
        }
        .overlay(alignment: .topTrailing) {
            Button {
                ferme()
            } label: {
                Image(systemName: "xmark")
                    .font(.headline)
                    .foregroundStyle(.white)
                    .frame(width: 36, height: 36)
                    .background(.white.opacity(0.18), in: Circle())
            }
            .accessibilityLabel(Text("Fermer"))
            .padding(16)
        }
    }

    // MARK: - En-tête

    private var enTete: some View {
        VStack(spacing: 14) {
            Image("Logo")
                .resizable()
                .scaledToFit()
                .frame(width: 88, height: 88)
                .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
                .shadow(color: .black.opacity(0.3), radius: 14, y: 8)
                .accessibilityHidden(true)
            VStack(spacing: 4) {
                Text("MonCap GPS")
                    .font(.system(size: 32, weight: .bold, design: .rounded))
                    .foregroundStyle(.white)
                Text("Retrouve tes trajets et ton abonnement sur ce téléphone.")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.85))
                    .multilineTextAlignment(.center)
            }
        }
    }

    // MARK: - Formulaire

    private var carte: some View {
        VStack(spacing: 14) {
            Text("Se connecter à un compte existant")
                .font(.headline)
                .frame(maxWidth: .infinity, alignment: .leading)

            // `.username` et non `.emailAddress` : c'est ce que le trousseau
            // apparie au mot de passe pour le proposer.
            champ(icone: "envelope") {
                TextField("e-mail", text: $username)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.emailAddress)
                    .textContentType(.username)
                    .focused($focus, equals: .email)
                    .submitLabel(.next)
                    .onSubmit { focus = .motDePasse }
            }

            champ(icone: "lock") {
                Group {
                    if motDePasseVisible {
                        TextField("mot de passe", text: $password)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                    } else {
                        SecureField("mot de passe", text: $password)
                    }
                }
                .textContentType(.password)
                .focused($focus, equals: .motDePasse)
                .submitLabel(.go)
                .onSubmit { if peutValider { submit() } }

                Button {
                    motDePasseVisible.toggle()
                } label: {
                    Image(systemName: motDePasseVisible ? "eye.slash" : "eye")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(
                    motDePasseVisible
                        ? Text("Masquer le mot de passe") : Text("Afficher le mot de passe"))
            }

            if !errorMessage.isEmpty {
                message(errorMessage, icone: "exclamationmark.triangle.fill", couleur: .red)
            }
            if !infoMessage.isEmpty {
                message(infoMessage, icone: "checkmark.circle.fill", couleur: .green)
            }

            Button(action: submit) {
                ZStack {
                    Text("Se connecter")
                        .font(.headline)
                        .opacity(busy ? 0 : 1)
                    if busy { ProgressView().tint(.white) }
                }
                .frame(maxWidth: .infinity, minHeight: 50)
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.roundedRectangle(radius: 14))
            .disabled(!peutValider)
            .padding(.top, 4)

            // Le lien de réinitialisation part par courriel : l'adresse tapée
            // au-dessus suffit. La réponse est la même que le compte existe
            // ou non — l'écran ne peut pas servir à deviner qui est inscrit.
            Button("Mot de passe oublié ?") { motDePasseOublie() }
                .font(.subheadline)
                .disabled(busy)
        }
        .padding(20)
        .background(
            .background, in: RoundedRectangle(cornerRadius: 24, style: .continuous)
        )
        .shadow(color: .black.opacity(0.18), radius: 24, y: 12)
        .animation(.easeInOut(duration: 0.2), value: errorMessage)
        .animation(.easeInOut(duration: 0.2), value: infoMessage)
    }

    /// Un champ dans sa case : icône à gauche, saisie, accessoires éventuels.
    private func champ<Contenu: View>(
        icone: String, @ViewBuilder _ contenu: () -> Contenu
    ) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icone)
                .foregroundStyle(.secondary)
                .frame(width: 20)
                .accessibilityHidden(true)
            contenu()
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 50)
        .background(
            Color(.secondarySystemBackground),
            in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func message(_ texte: String, icone: String, couleur: Color) -> some View {
        Label {
            Text(LocalizedStringKey(texte))
        } icon: {
            Image(systemName: icone)
        }
        .font(.footnote)
        .foregroundStyle(couleur)
        .frame(maxWidth: .infinity, alignment: .leading)
        .transition(.opacity)
    }

    // MARK: - Actions

    /// Demande le lien de réinitialisation pour l'adresse saisie.
    private func motDePasseOublie() {
        let email = AuthStore.normalize(username)
        guard AuthStore.isValidEmail(email) else {
            errorMessage = "Tape ton adresse e-mail ci-dessus, puis touche « Mot de passe oublié ? »."
            infoMessage = ""
            focus = .email
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
        focus = nil
        busy = true
        errorMessage = ""
        infoMessage = ""
        Task {
            do {
                try await auth.login(username, password)
                ferme()
            } catch let e as AuthError {
                errorMessage = e.errorDescription ?? "Saisie invalide."
            } catch APIError.unauthorized {
                errorMessage = "E-mail ou mot de passe incorrect."
            } catch APIError.server(let msg) {
                errorMessage = msg.isEmpty ? "Échec de l'authentification" : msg
            } catch {
                errorMessage = "Serveur injoignable. Réessaie."
            }
            busy = false
        }
    }
}
