import SwiftUI

/// « 400 m » ou « 1,2 km » : la distance comme le bandeau iPhone l'écrit.
private func distanceCourte(_ m: Double) -> String {
    m >= 1_000
        ? String(format: "%.1f km", m / 1_000)
        : String(format: "%.0f m", m)
}

/// « 18:42 » : l'heure d'arrivée, plus parlante qu'une durée.
private func heureArrivee(_ minutes: Double) -> String {
    let f = DateFormatter()
    f.locale = Locale(identifier: Bundle.main.preferredLocalizations.first ?? "fr")
    f.setLocalizedDateFormatFromTemplate("Hm")
    return f.string(from: Date().addingTimeInterval(minutes * 60))
}

/// L'écran unique de la montre : la manœuvre en grand quand un guidage
/// tourne sur l'iPhone, un message d'attente sinon.
struct MontreVue: View {
    @ObservedObject private var lien = LienTelephone.partage

    var body: some View {
        Group {
            if lien.etat.actif {
                guidage
            } else {
                repos
            }
        }
    }

    private var guidage: some View {
        VStack(spacing: 4) {
            Image(systemName: lien.etat.symbole)
                .font(.system(size: 34, weight: .bold))
                .foregroundStyle(.orange)
            Text(distanceCourte(lien.etat.distanceM))
                .font(.system(.title2, design: .rounded).weight(.bold))
                .monospacedDigit()
            Text(lien.etat.instruction)
                .font(.footnote)
                .multilineTextAlignment(.center)
                .lineLimit(3)
                .foregroundStyle(.secondary)
            Spacer(minLength: 2)
            HStack {
                Label(String(format: "%.1f km", lien.etat.restantKm),
                      systemImage: "road.lanes")
                Spacer()
                Label(heureArrivee(lien.etat.etaMinutes),
                      systemImage: "flag.checkered")
            }
            .font(.caption2)
            .foregroundStyle(.secondary)
            .labelStyle(.titleAndIcon)
        }
        .padding(.horizontal, 4)
    }

    private var repos: some View {
        VStack(spacing: 8) {
            Image(systemName: "iphone.gen3.radiowaves.left.and.right")
                .font(.system(size: 30))
                .foregroundStyle(.orange)
            Text("Lancez un guidage sur l'iPhone : la consigne s'affiche ici.")
                .font(.footnote)
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 6)
    }
}

#Preview {
    MontreVue()
}
