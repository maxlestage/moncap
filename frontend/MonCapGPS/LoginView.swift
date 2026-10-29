import SwiftUI

/// Écran de connexion / inscription.
///
/// Il prend la suite de l'écran de garde dans le même décor (`FondDeMarque`) :
/// le logo et le nom restent sur le bleu, le formulaire vient dans une carte
/// claire. Le clavier enchaîne les champs — « suivant » puis « aller » — et
/// le trousseau d'iOS reconnaît la paire identifiant / mot de passe.
struct LoginView: View {
    @ObservedObject var auth: AuthStore

    private enum Champ { case email, motDePasse }

    @State private var signupMode = false
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
                Text("Navigation en temps réel")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.85))
            }
        }
    }

    // MARK: - Formulaire

    private var carte: some View {
        VStack(spacing: 14) {
            Picker("Mode", selection: $signupMode) {
                Text("Connexion").tag(false)
                Text("Inscription").tag(true)
            }
            .pickerStyle(.segmented)
            .padding(.bottom, 4)
            .onChange(of: signupMode) {
                errorMessage = ""
                infoMessage = ""
            }

            // L'e-mail comme identifiant — choix de produit assumé, et la
            // politique de confidentialité le dit avec ses limites : aucun
            // envoi, aucune revente. `.username` et non `.emailAddress` : c'est
            // ce que le trousseau apparie au mot de passe pour le proposer.
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

            // `.newPassword` à l'inscription : le trousseau propose un mot de
            // passe fort et le retient — `.password` à la connexion pour
            // qu'il ressorte le bon.
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
                .textContentType(signupMode ? .newPassword : .password)
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

            if signupMode {
                Text("Ton e-mail sert d'identifiant — il ne sert qu'à ça, et à retrouver ton mot de passe.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            if !errorMessage.isEmpty {
                message(errorMessage, icone: "exclamationmark.triangle.fill", couleur: .red)
            }
            if !infoMessage.isEmpty {
                message(infoMessage, icone: "checkmark.circle.fill", couleur: .green)
            }

            Button(action: submit) {
                ZStack {
                    (signupMode ? Text("Créer mon compte") : Text("Se connecter"))
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
            if !signupMode {
                Button("Mot de passe oublié ?") { motDePasseOublie() }
                    .font(.subheadline)
                    .disabled(busy)
            }
        }
        .padding(20)
        .background(
            .background, in: RoundedRectangle(cornerRadius: 24, style: .continuous)
        )
        .shadow(color: .black.opacity(0.18), radius: 24, y: 12)
        .animation(.easeInOut(duration: 0.2), value: signupMode)
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
