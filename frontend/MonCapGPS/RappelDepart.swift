import SwiftUI
import UserNotifications

/// « Pars maintenant » : on donne l'heure d'arrivée voulue, l'app cale
/// l'heure de départ sur le trajet estimé — cinq minutes de marge — et pose
/// une notification locale. Tout sur l'appareil, rien à surveiller.
///
/// La limite est dite dans la feuille : l'estimation est celle du moment.
/// Si les bouchons grossissent d'ici là, il faudra partir un peu avant —
/// l'app ne recalcule pas en arrière-plan, iOS ne l'y autorise pas.
struct RappelDepartView: View {
    /// Durée estimée du trajet choisi, en minutes.
    let minutesTrajet: Double

    @Environment(\.dismiss) private var ferme
    @State private var arrivee = Date().addingTimeInterval(3600)
    @State private var pose = false

    private static let margeMinutes: Double = 5

    private var depart: Date {
        arrivee.addingTimeInterval(-(minutesTrajet + Self.margeMinutes) * 60)
    }

    private var heureCourt: DateFormatter {
        let f = DateFormatter()
        f.locale = Locale(identifier: Bundle.main.preferredLocalizations.first ?? "fr")
        f.setLocalizedDateFormatFromTemplate("Hm")
        return f
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    DatePicker(
                        "Arriver avant", selection: $arrivee,
                        displayedComponents: [.hourAndMinute])
                    LabeledContent {
                        Text(verbatim: String(format: "%.0f min", minutesTrajet))
                    } label: {
                        Text("Trajet estimé")
                    }
                } footer: {
                    Text("L'estimation est celle du moment : si ça bouchonne d'ici là, pars un peu avant. L'app ne recalcule pas en arrière-plan.")
                }
                Section {
                    if depart <= Date() {
                        Label {
                            Text("Pars maintenant — il est déjà l'heure.")
                                .font(.subheadline.weight(.semibold))
                        } icon: {
                            Image(systemName: "figure.run")
                                .foregroundStyle(.orange)
                        }
                    } else if pose {
                        Label {
                            Text(
                                String(
                                    format: NSLocalizedString(
                                        "Rappel posé pour %@.", comment: ""),
                                    heureCourt.string(from: depart)))
                        } icon: {
                            Image(systemName: "checkmark.circle.fill")
                                .foregroundStyle(.green)
                        }
                    } else {
                        Button {
                            planifie()
                        } label: {
                            Label {
                                Text(
                                    String(
                                        format: NSLocalizedString(
                                            "Me prévenir à %@", comment: ""),
                                        heureCourt.string(from: depart)))
                            } icon: {
                                Image(systemName: "bell.badge")
                            }
                        }
                    }
                }
            }
            .navigationTitle("Partir à l'heure")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { ferme() }
                }
            }
        }
        .presentationDetents([.medium])
        .presentationBackground(Color(.systemBackground))
        .onChange(of: arrivee) { pose = false }
    }

    private func planifie() {
        let contenu = UNMutableNotificationContent()
        contenu.title = NSLocalizedString("Il est l'heure de partir", comment: "")
        contenu.body = String(
            format: NSLocalizedString(
                "Pars maintenant pour arriver vers %@ (~%.0f min de route).",
                comment: ""),
            heureCourt.string(from: arrivee), minutesTrajet)
        contenu.sound = .default
        let composants = Calendar.current.dateComponents(
            [.year, .month, .day, .hour, .minute], from: depart)
        let centre = UNUserNotificationCenter.current()
        centre.requestAuthorization(options: [.alert, .sound]) { accorde, _ in
            guard accorde else { return }
            centre.add(
                UNNotificationRequest(
                    identifier: "depart-\(Int(depart.timeIntervalSince1970))",
                    content: contenu,
                    trigger: UNCalendarNotificationTrigger(
                        dateMatching: composants, repeats: false)))
        }
        pose = true
    }
}
