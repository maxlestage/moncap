import SwiftUI

/// La découverte du premier lancement : trois écrans, trente secondes — la
/// valeur avant la carte nue. C'est ici qu'un nouvel arrivant comprend ce
/// qu'il essaie pendant quatorze jours, et pourquoi il paiera ensuite.
struct DecouverteView: View {
    /// Appelé quand la découverte est finie ou passée — une seule fois.
    var surFin: () -> Void

    @State private var page = 0

    private static let derniere = 2

    var body: some View {
        ZStack {
            LinearGradient(
                colors: [
                    Color(red: 0.055, green: 0.165, blue: 0.369),
                    Color(red: 0.10, green: 0.34, blue: 0.90),
                ],
                startPoint: .top, endPoint: .bottom
            )
            .ignoresSafeArea()

            VStack(spacing: 18) {
                TabView(selection: $page) {
                    ecran(
                        icone: "map.fill",
                        titre: "Compare, puis choisis",
                        texte: "Jusqu'à 15 itinéraires côte à côte — durée, virages, dénivelé, carburant, péages — et tu choisis en connaissance."
                    )
                    .tag(0)
                    ecran(
                        icone: "arrow.down.circle.fill",
                        titre: "Roule sans réseau",
                        texte: "Télécharge ta région une fois : itinéraires, lieux et limites de vitesse se calculent sur le téléphone — vallées, tunnels, étranger."
                    )
                    .tag(1)
                    ecran(
                        icone: "star.fill",
                        titre: "Quatorze jours pour tout essayer",
                        texte: "Tout est ouvert pendant l'essai. Ensuite, une formule au mois : Plus pour l'app entière, Pro pour ceux qui roulent pour travailler."
                    )
                    .tag(2)
                }
                .tabViewStyle(.page)

                Button {
                    if page < Self.derniere {
                        withAnimation { page += 1 }
                    } else {
                        surFin()
                    }
                } label: {
                    Text(page < Self.derniere ? "Continuer" : "C'est parti")
                        .font(.headline)
                        .foregroundStyle(Color(red: 0.055, green: 0.165, blue: 0.369))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                }
                .buttonStyle(.borderedProminent)
                .tint(.white)

                // Passer reste possible : une découverte qui séquestre
                // apprend surtout à détester l'app.
                Button("Passer") { surFin() }
                    .font(.caption)
                    .foregroundStyle(.white.opacity(page < Self.derniere ? 0.8 : 0))
                    .disabled(page == Self.derniere)
            }
            .padding(24)
        }
    }

    private func ecran(
        icone: String, titre: LocalizedStringKey, texte: LocalizedStringKey
    ) -> some View {
        VStack(spacing: 20) {
            Image(systemName: icone)
                .font(.system(size: 64, weight: .bold))
                .opacity(0.95)
            Text(titre)
                .font(.largeTitle.bold())
                .multilineTextAlignment(.center)
            Text(texte)
                .font(.body)
                .multilineTextAlignment(.center)
                .opacity(0.9)
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 12)
        // La pagination du TabView vit en bas : sans marge, elle mord le texte.
        .padding(.bottom, 40)
    }
}
