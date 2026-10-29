import SwiftUI

/// Écran de garde + de chargement affiché au lancement : logo, nom, accroche et
/// indicateur de chargement, sur le fond bleu de marque. Se fond ensuite dans
/// l'application (voir ContentView).
///
/// Trois partis pris :
///
/// 1. **Pas de roue standard.** Celle d'iOS dit « une application charge » ;
///    elle ne dit pas *laquelle*. Un cap qui s'oriente, lui, ne peut venir que
///    d'ici — c'est le nom de l'app, et c'est ce qu'elle fait.
/// 2. **Ce qui bouge doit vouloir dire quelque chose.** L'aiguille balaie puis
///    se pose au nord, le halo respire : le regard a un point d'arrivée, au
///    lieu de tourner indéfiniment.
/// 3. **Rien ne bouge si l'on a demandé que rien ne bouge.** Sous « Réduire les
///    animations », l'écran s'affiche posé, aiguille au nord — un écran de
///    garde n'est pas un endroit où passer outre un réglage d'accessibilité.
struct SplashView: View {
    @Environment(\.accessibilityReduceMotion) private var animationsReduites

    /// Apparition : le logo grandit, les textes montent en fondu.
    @State private var apparu = false
    /// Balayage de l'aiguille, puis repos au nord.
    @State private var capPose = false
    /// Respiration du halo, seule animation qui dure.
    @State private var respire = false

    var body: some View {
        ZStack {
            FondDeMarque()
            VStack(spacing: 24) {
                boussole
                titres
            }
            .padding()
            signature
        }
        .onAppear(perform: demarre)
    }

    // MARK: - Boussole

    /// Le logo dans une rose des vents : halo, graduations, aiguille.
    private var boussole: some View {
        ZStack {
            // Le halo qui respire, sous tout le reste.
            Circle()
                .fill(.white.opacity(0.12))
                .frame(width: 190, height: 190)
                .blur(radius: 22)
                .scaleEffect(respire ? 1.08 : 0.94)

            graduations

            Image("Logo")
                .resizable()
                .scaledToFit()
                .frame(width: 118, height: 118)
                .clipShape(RoundedRectangle(cornerRadius: 27, style: .continuous))
                .shadow(color: .black.opacity(0.35), radius: 18, y: 10)
                .scaleEffect(apparu ? 1 : 0.86)
                .opacity(apparu ? 1 : 0)

            aiguille
        }
        .frame(width: 190, height: 190)
        .accessibilityElement()
        .accessibilityLabel(Text("MonCap GPS"))
    }

    /// L'anneau et ses graduations : le cadran sur lequel l'aiguille se pose.
    ///
    /// Trente-six traits, un tous les dix degrés, les quatre cardinaux plus
    /// longs et plus francs. Dessinés plutôt qu'imagés : nets à toute
    /// définition, et ils suivent la couleur du texte sans qu'on y pense.
    private var graduations: some View {
        ZStack {
            Circle()
                .stroke(.white.opacity(0.22), lineWidth: 1)
                .frame(width: 178, height: 178)

            ForEach(0..<36, id: \.self) { i in
                let cardinal = i % 9 == 0
                Capsule()
                    .fill(.white.opacity(cardinal ? 0.75 : 0.30))
                    .frame(width: cardinal ? 2 : 1, height: cardinal ? 11 : 6)
                    .offset(y: -89)
                    .rotationEffect(.degrees(Double(i) * 10))
            }
        }
        .opacity(apparu ? 1 : 0)
    }

    /// L'aiguille : elle balaie une fois, puis se pose au nord.
    ///
    /// Un seul balayage, et non une rotation sans fin. Une roue qui tourne
    /// toujours finit par dire « c'est bloqué » ; une aiguille qui se pose dit
    /// « c'est prêt », ce qui est précisément la nouvelle qu'on attend.
    private var aiguille: some View {
        Image(systemName: "location.north.fill")
            .font(.system(size: 15, weight: .bold))
            .foregroundStyle(.white)
            .shadow(color: .black.opacity(0.3), radius: 3, y: 1)
            .offset(y: -89)
            .rotationEffect(.degrees(capPose ? 0 : -290))
            .opacity(apparu ? 1 : 0)
    }

    // MARK: - Textes

    private var titres: some View {
        VStack(spacing: 6) {
            Text("MonCap GPS")
                .font(.system(size: 34, weight: .bold, design: .rounded))
                .foregroundStyle(.white)
            Text("Navigation en temps réel")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(0.85))
        }
        .opacity(apparu ? 1 : 0)
        .offset(y: apparu ? 0 : 10)
    }

    private var signature: some View {
        VStack {
            Spacer()
            Text("par Maxime Nathan Lestage")
                .font(.footnote)
                .foregroundStyle(.white.opacity(0.7))
                .padding(.bottom, 28)
                .opacity(apparu ? 1 : 0)
        }
    }

    // MARK: - Animation

    /// Lance les trois mouvements, ou aucun.
    private func demarre() {
        guard !animationsReduites else {
            apparu = true
            capPose = true
            return
        }

        withAnimation(.easeOut(duration: 0.55)) { apparu = true }
        // L'aiguille part une fraction de seconde après le logo : elle se pose
        // sur un cadran déjà là, au lieu d'apparaître avec lui.
        withAnimation(.spring(response: 1.1, dampingFraction: 0.72).delay(0.15)) {
            capPose = true
        }
        withAnimation(.easeInOut(duration: 2.2).repeatForever(autoreverses: true)) {
            respire = true
        }
    }
}

/// Le dégradé de marque, réchauffé de deux halos.
///
/// Le haut reprend la couleur du launch screen natif (`LaunchBackground`)
/// pour que le passage de l'écran système au nôtre ne se voie pas. Les
/// halos sont flous et très peu opaques : ils donnent de la profondeur sans
/// qu'on puisse les nommer, et surtout sans concurrencer le logo.
///
/// Partagé avec l'écran de connexion, qui suit l'écran de garde : l'un
/// se fond dans l'autre sans changer de décor.
struct FondDeMarque: View {
    var body: some View {
        ZStack {
            LinearGradient(
                colors: [
                    Color("LaunchBackground"),
                    Color(red: 0.10, green: 0.34, blue: 0.90),
                ],
                startPoint: .top,
                endPoint: .bottom
            )

            Circle()
                .fill(Color(red: 0.35, green: 0.72, blue: 1).opacity(0.30))
                .frame(width: 340, height: 340)
                .blur(radius: 90)
                .offset(x: -110, y: -180)

            Circle()
                .fill(Color(red: 0.55, green: 0.35, blue: 1).opacity(0.24))
                .frame(width: 300, height: 300)
                .blur(radius: 90)
                .offset(x: 130, y: 160)
        }
        .ignoresSafeArea()
    }
}
