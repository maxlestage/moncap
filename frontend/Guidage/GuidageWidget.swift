import ActivityKit
import SwiftUI
import WidgetKit

// Copie conforme de la structure côté app (GuidageActivite.swift) :
// ActivityKit apparie l'app et cette extension par le **nom** du type et ses
// clés JSON — les deux copies doivent rester identiques mot pour mot.
struct GuidageAttributs: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        var instruction: String
        var distanceM: Double
        var etaMinutes: Double
        var restantKm: Double
        var symbole: String
    }
}

/// « 400 m » ou « 1,2 km » : la distance comme le bandeau l'écrit.
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

@main
struct GuidageWidgets: WidgetBundle {
    var body: some Widget {
        GuidageWidget()
    }
}

/// Le guidage sur l'écran verrouillé et dans la Dynamic Island : la flèche,
/// la consigne, la distance au prochain virage, le restant et l'heure
/// d'arrivée — l'essentiel du bandeau, sans déverrouiller.
struct GuidageWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: GuidageAttributs.self) { contexte in
            // Écran verrouillé : une carte compacte, lisible d'un regard.
            HStack(spacing: 12) {
                Image(systemName: contexte.state.symbole)
                    .font(.system(size: 30, weight: .bold))
                    .foregroundStyle(.green)
                VStack(alignment: .leading, spacing: 2) {
                    Text(distanceCourte(contexte.state.distanceM))
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Text(contexte.state.instruction)
                        .font(.headline)
                        .lineLimit(2)
                }
                Spacer()
                VStack(alignment: .trailing, spacing: 2) {
                    Text(heureArrivee(contexte.state.etaMinutes))
                        .font(.headline)
                    Text(String(format: "%.0f km", contexte.state.restantKm))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(14)
            .activityBackgroundTint(Color.black.opacity(0.6))
            .activitySystemActionForegroundColor(.white)
        } dynamicIsland: { contexte in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Image(systemName: contexte.state.symbole)
                        .font(.system(size: 26, weight: .bold))
                        .foregroundStyle(.green)
                }
                DynamicIslandExpandedRegion(.center) {
                    Text(contexte.state.instruction)
                        .font(.headline)
                        .lineLimit(2)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack {
                        Text(distanceCourte(contexte.state.distanceM))
                            .font(.subheadline.weight(.semibold))
                        Spacer()
                        Text(
                            String(
                                format: "%.0f km · %@", contexte.state.restantKm,
                                heureArrivee(contexte.state.etaMinutes)))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                }
            } compactLeading: {
                Image(systemName: contexte.state.symbole)
                    .foregroundStyle(.green)
            } compactTrailing: {
                Text(distanceCourte(contexte.state.distanceM))
                    .font(.caption2.weight(.semibold))
            } minimal: {
                Image(systemName: contexte.state.symbole)
                    .foregroundStyle(.green)
            }
        }
    }
}
