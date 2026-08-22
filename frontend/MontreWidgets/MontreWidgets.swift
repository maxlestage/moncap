import SwiftUI
import WidgetKit

/// La complication MonCap : l'écran du guidage à un toucher du cadran.
///
/// Elle ne porte aucune donnée — c'est un lanceur, comme le widget
/// Domicile/Travail de l'iPhone : toucher une complication ouvre toujours
/// son app, et c'est l'app montre (MontreVue) qui montre le guidage en
/// grand. Un lanceur qui ne sait rien n'a rien à désynchroniser.
@main
struct MontreWidgetsBundle: WidgetBundle {
    var body: some Widget {
        LanceurMontre()
    }
}

struct LanceurEntree: TimelineEntry {
    let date: Date
}

struct LanceurProvider: TimelineProvider {
    func placeholder(in context: Context) -> LanceurEntree {
        LanceurEntree(date: Date())
    }

    func getSnapshot(
        in context: Context, completion: @escaping (LanceurEntree) -> Void
    ) {
        completion(LanceurEntree(date: Date()))
    }

    func getTimeline(
        in context: Context, completion: @escaping (Timeline<LanceurEntree>) -> Void
    ) {
        // Rien ne change jamais : la complication est un bouton, pas un cadran.
        completion(Timeline(entries: [LanceurEntree(date: Date())], policy: .never))
    }
}

/// Le bleu de la maison, celui de la pastille de manœuvre.
private let bleuMontre = Color(red: 0.29, green: 0.61, blue: 1.0)

struct LanceurVue: View {
    @Environment(\.widgetFamily) private var famille

    var body: some View {
        switch famille {
        case .accessoryRectangular:
            HStack(spacing: 8) {
                Image(systemName: "location.north.circle.fill")
                    .font(.title2)
                    .foregroundStyle(bleuMontre)
                    .widgetAccentable()
                VStack(alignment: .leading, spacing: 0) {
                    Text(verbatim: "MonCap")
                        .font(.headline)
                    Text(verbatim: "Guidage au poignet")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
            }
        case .accessoryInline:
            Label {
                Text(verbatim: "MonCap")
            } icon: {
                Image(systemName: "location.north.circle.fill")
            }
        default:
            // Cercle et coin de cadran : le glyphe seul, accentuable pour
            // suivre la teinte du cadran choisi.
            Image(systemName: "location.north.circle.fill")
                .font(.title2)
                .foregroundStyle(bleuMontre)
                .widgetAccentable()
        }
    }
}

struct LanceurMontre: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(
            kind: "moncap.montre.lanceur", provider: LanceurProvider()
        ) { _ in
            LanceurVue()
                .containerBackground(for: .widget) {
                    LinearGradient(
                        colors: [
                            Color(red: 0.10, green: 0.15, blue: 0.25),
                            Color(red: 0.02, green: 0.03, blue: 0.05),
                        ],
                        startPoint: .top, endPoint: .bottom)
                }
        }
        .configurationDisplayName("MonCap")
        .description("Le guidage au poignet, d'un toucher.")
        .supportedFamilies([
            .accessoryCircular, .accessoryCorner, .accessoryInline,
            .accessoryRectangular,
        ])
    }
}
