import SwiftUI

/// Écran de garde + de chargement affiché au lancement : logo, nom, accroche et
/// indicateur de chargement, sur le fond bleu de marque. Se fond ensuite dans
/// l'application (voir ContentView).
struct SplashView: View {
    /// Animation d'apparition (logo qui grandit + textes en fondu).
    @State private var appeared = false

    var body: some View {
        ZStack {
            // Dégradé de marque : le haut reprend la couleur du launch screen
            // natif (LaunchBackground) pour une continuité sans rupture.
            LinearGradient(
                colors: [
                    Color("LaunchBackground"),
                    Color(red: 0.10, green: 0.34, blue: 0.90),
                ],
                startPoint: .top,
                endPoint: .bottom
            )
            .ignoresSafeArea()

            VStack(spacing: 22) {
                Image("Logo")
                    .resizable()
                    .scaledToFit()
                    .frame(width: 132, height: 132)
                    .clipShape(RoundedRectangle(cornerRadius: 30, style: .continuous))
                    .shadow(color: .black.opacity(0.35), radius: 18, y: 10)
                    .scaleEffect(appeared ? 1 : 0.85)
                    .opacity(appeared ? 1 : 0)

                VStack(spacing: 6) {
                    Text("MonCap GPS")
                        .font(.system(size: 34, weight: .bold, design: .rounded))
                        .foregroundStyle(.white)
                    Text("Navigation en temps réel")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(0.85))
                }
                .opacity(appeared ? 1 : 0)
                .offset(y: appeared ? 0 : 8)

                ProgressView()
                    .progressViewStyle(.circular)
                    .tint(.white)
                    .padding(.top, 10)
                    .opacity(appeared ? 1 : 0)
            }
            .padding()

            // Signature de l'auteur, discrète en bas de l'écran de garde.
            VStack {
                Spacer()
                Text("par Maxime Nathan Lestage")
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(0.7))
                    .padding(.bottom, 28)
                    .opacity(appeared ? 1 : 0)
            }
        }
        .onAppear {
            withAnimation(.easeOut(duration: 0.6)) { appeared = true }
        }
    }
}
