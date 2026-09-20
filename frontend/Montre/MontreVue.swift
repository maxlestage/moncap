import SwiftUI

/// « 400 m » ou « 1,2 km » : la distance comme le bandeau iPhone l'écrit.
private func distanceCourte(_ m: Double) -> String {
    // Seuil sur la valeur arrondie : 999,6 m arrondi à l'unité donnerait
    // « 1000 m », qui est un kilomètre. Même règle que Distance.courte.
    m.rounded() >= 1_000
        ? String(format: "%.1f km", m / 1_000)
        : String(format: "%.0f m", max(0, m))
}

/// « 18:42 » : l'heure d'arrivée, plus parlante qu'une durée.
private func heureArrivee(_ minutes: Double) -> String {
    let f = DateFormatter()
    f.locale = Locale(identifier: Bundle.main.preferredLocalizations.first ?? "fr")
    f.setLocalizedDateFormatFromTemplate("Hm")
    return f.string(from: Date().addingTimeInterval(minutes * 60))
}

/// L'écran unique de la montre : la manœuvre en grand quand un guidage
/// tourne sur l'iPhone, l'écran d'attente sinon. Le fond est une nuit
/// marine — la lueur bleue en haut, le noir de l'OLED en bas.
struct MontreVue: View {
    @ObservedObject private var lien = LienTelephone.partage

    /// Le bleu de la pastille et des halos : celui des tracés de la carte.
    private static let bleu = Color(red: 0.29, green: 0.61, blue: 1.0)

    var body: some View {
        ZStack {
            fond
            if lien.etat.actif {
                // Deux pages. La consigne reste seule sur la sienne : c'est
                // elle qu'on regarde en roulant, et un bouton posé dessus se
                // touche par accident — au poignet, la main qui tient le
                // volant passe devant l'écran à chaque virage. Les commandes
                // sont à un glissement, ce qui demande une intention.
                TabView {
                    guidage
                    commandes
                }
                .tabViewStyle(.page(indexDisplayMode: .automatic))
            } else {
                repos
            }
        }
        .animation(.snappy, value: lien.etat.actif)
    }

    private var fond: some View {
        LinearGradient(
            colors: [Color(red: 0.10, green: 0.15, blue: 0.25), .black],
            startPoint: .top, endPoint: .init(x: 0.5, y: 0.72)
        )
        .ignoresSafeArea()
    }

    private var guidage: some View {
        VStack(spacing: 4) {
            // La manœuvre d'abord : une pastille dégradée qui se voit du coin
            // de l'œil, le glyphe en plein blanc.
            ZStack {
                Circle().fill(
                    LinearGradient(
                        colors: [Self.bleu, Color(red: 0.09, green: 0.16, blue: 0.38)],
                        startPoint: .topLeading, endPoint: .bottomTrailing))
                Image(systemName: lien.etat.symbole)
                    .font(.system(size: 25, weight: .heavy))
                    .foregroundStyle(.white)
            }
            .frame(width: 50, height: 50)
            .shadow(color: Self.bleu.opacity(0.55), radius: 9)

            Text(distanceCourte(lien.etat.distanceM))
                .font(.system(size: 30, weight: .black, design: .rounded))
                .monospacedDigit()
                .contentTransition(.numericText())
                .minimumScaleFactor(0.6)
                .lineLimit(1)

            Text(lien.etat.instruction)
                .font(.footnote.weight(.medium))
                .multilineTextAlignment(.center)
                .lineLimit(2)
                .minimumScaleFactor(0.8)
                .foregroundStyle(.secondary)

            Spacer(minLength: 2)

            // Le pied d'écran : restant et arrivée côte à côte, sur une
            // plaque translucide qui les détache du fond.
            HStack(spacing: 0) {
                // Le restant suit la même règle que la manœuvre : des
                // mètres sous le kilomètre — « 0.2 km » ne se dit pas.
                colonne(distanceCourte(lien.etat.restantKm * 1_000), "restant")
                Divider().frame(height: 22)
                colonne(heureArrivee(lien.etat.etaMinutes), "arrivée")
            }
            .padding(.vertical, 5)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 12))
        }
        .padding(.horizontal, 2)
        .animation(.snappy, value: lien.etat.instruction)
    }

    /// La page des ordres. Deux seulement, et les deux utiles au poignet :
    /// redemander la consigne qu'on n'a pas entendue, et couper court.
    ///
    /// « Arrêter » en rouge et en second : c'est l'ordre qu'on ne veut pas
    /// donner par erreur, il est donc le plus loin du pouce qui arrive.
    private var commandes: some View {
        VStack(spacing: 8) {
            Text("Commandes")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
            Button {
                lien.commande("redit")
            } label: {
                Label("Redire", systemImage: "speaker.wave.2.fill")
                    .font(.footnote.weight(.semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                    .frame(maxWidth: .infinity)
            }
            .tint(Self.bleu)
            Button {
                lien.commande("arrete")
            } label: {
                Label("Arrêter", systemImage: "xmark.circle.fill")
                    .font(.footnote.weight(.semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                    .frame(maxWidth: .infinity)
            }
            .tint(.red)
        }
        .buttonStyle(.borderedProminent)
        .padding(.horizontal, 4)
    }

    private func colonne(_ valeur: String, _ legende: String) -> some View {
        VStack(spacing: 0) {
            Text(valeur)
                .font(.system(.footnote, design: .rounded).weight(.bold))
                .monospacedDigit()
            Text(legende)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
    }

    private var repos: some View {
        VStack(spacing: 8) {
            Image(systemName: "location.north.circle.fill")
                .font(.system(size: 40))
                .foregroundStyle(Self.bleu)
                .shadow(color: Self.bleu.opacity(0.45), radius: 8)
            Text("MonCap")
                .font(.system(.headline, design: .rounded).weight(.heavy))
            Text("Lancez un guidage sur l'iPhone : la consigne s'affiche ici.")
                .font(.caption2)
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 8)
    }
}

#Preview {
    MontreVue()
}
