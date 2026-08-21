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
        RaccourcisWidget()
    }
}

// MARK: - Le widget d'écran d'accueil

/// Partir d'un geste : Domicile et Travail depuis l'écran d'accueil.
///
/// Le widget ne porte **aucune donnée** — pas d'App Group, pas d'état
/// partagé : seulement des liens profonds `moncap://favori`, que l'app
/// résout au réveil avec les adresses qu'elle connaît déjà. Un widget qui
/// ne sait rien n'a rien à désynchroniser.
struct RaccourcisEntry: TimelineEntry {
    let date: Date
}

struct RaccourcisProvider: TimelineProvider {
    func placeholder(in context: Context) -> RaccourcisEntry {
        RaccourcisEntry(date: Date())
    }

    func getSnapshot(
        in context: Context, completion: @escaping (RaccourcisEntry) -> Void
    ) {
        completion(RaccourcisEntry(date: Date()))
    }

    func getTimeline(
        in context: Context, completion: @escaping (Timeline<RaccourcisEntry>) -> Void
    ) {
        // Rien ne change jamais : le widget est un bouton, pas un cadran.
        completion(Timeline(entries: [RaccourcisEntry(date: Date())], policy: .never))
    }
}

struct RaccourcisView: View {
    @Environment(\.widgetFamily) private var famille

    var body: some View {
        if famille == .systemSmall {
            // Petit format : un seul geste, le retour à la maison.
            bouton("house.fill", "Domicile")
                .widgetURL(URL(string: "moncap://favori?nom=domicile"))
        } else {
            HStack(spacing: 10) {
                Link(destination: URL(string: "moncap://favori?nom=domicile")!) {
                    bouton("house.fill", "Domicile")
                }
                Link(destination: URL(string: "moncap://favori?nom=travail")!) {
                    bouton("briefcase.fill", "Travail")
                }
            }
        }
    }

    private func bouton(_ icone: String, _ titre: String) -> some View {
        VStack(spacing: 6) {
            Image(systemName: icone)
                .font(.system(size: 26, weight: .bold))
            Text(verbatim: titre)
                .font(.caption.weight(.semibold))
        }
        .foregroundStyle(.white)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.white.opacity(0.14), in: RoundedRectangle(cornerRadius: 14))
    }
}

struct RaccourcisWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(
            kind: "moncap.raccourcis", provider: RaccourcisProvider()
        ) { _ in
            RaccourcisView()
                .containerBackground(for: .widget) {
                    // Le dégradé de l'écran de garde : la marque, pas un fond.
                    LinearGradient(
                        colors: [
                            Color(red: 0.055, green: 0.165, blue: 0.369),
                            Color(red: 0.10, green: 0.34, blue: 0.90),
                        ],
                        startPoint: .top, endPoint: .bottom)
                }
        }
        .configurationDisplayName("MonCap")
        .description("Domicile et Travail, d'un geste.")
        .supportedFamilies([.systemSmall, .systemMedium])
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
