import SwiftUI
import UserNotifications

/// Les rappels d'entretien du véhicule — vidange, contrôle technique,
/// pneus — déclenchés par les kilomètres **réellement parcourus**, ceux du
/// journal des trajets. Pas de date arbitraire : une voiture s'use en
/// roulant, pas en attendant.
///
/// Tout vit sur l'appareil (UserDefaults) ; quand le journal franchit une
/// échéance, l'app le remarque à la prochaine lecture et pose une
/// notification locale, une seule fois.
struct RappelEntretien: Codable, Identifiable, Equatable {
    var id = UUID()
    /// Libellé du rappel (« Vidange », « Contrôle technique »…).
    var nom: String
    /// Total du journal (km) auquel l'échéance tombe.
    var seuilKm: Double
    /// Notification déjà posée : on ne harcèle pas.
    var prevenu = false
}

@MainActor
final class Entretien: ObservableObject {
    static let shared = Entretien()

    @Published private(set) var rappels: [RappelEntretien] = []

    private static let cle = "moncap.entretien.rappels"

    private init() {
        if let donnees = UserDefaults.standard.data(forKey: Self.cle),
            let lus = try? JSONDecoder().decode([RappelEntretien].self, from: donnees)
        {
            rappels = lus
        }
    }

    private func sauve() {
        if let donnees = try? JSONEncoder().encode(rappels) {
            UserDefaults.standard.set(donnees, forKey: Self.cle)
        }
    }

    /// Pose un rappel « dans tant de kilomètres » à partir du compteur
    /// actuel du journal.
    func ajoute(nom: String, dansKm: Double, kmJournal: Double) {
        let propre = nom.trimmingCharacters(in: .whitespaces)
        guard !propre.isEmpty, dansKm > 0 else { return }
        rappels.append(RappelEntretien(nom: propre, seuilKm: kmJournal + dansKm))
        sauve()
    }

    func retire(_ id: UUID) {
        rappels.removeAll { $0.id == id }
        sauve()
    }

    /// À chaque lecture du journal : marque les échéances franchies et pose
    /// la notification locale — une fois par rappel, pas une par lecture.
    func verifie(kmJournal: Double) {
        var changement = false
        for i in rappels.indices
        where !rappels[i].prevenu && kmJournal >= rappels[i].seuilKm {
            rappels[i].prevenu = true
            changement = true
            let contenu = UNMutableNotificationContent()
            contenu.title = NSLocalizedString("Entretien du véhicule", comment: "")
            contenu.body = String(
                format: NSLocalizedString(
                    "%@ : les kilomètres y sont — c'est le moment.", comment: ""),
                rappels[i].nom)
            contenu.sound = .default
            UNUserNotificationCenter.current().add(
                UNNotificationRequest(
                    identifier: "entretien-\(rappels[i].id)",
                    content: contenu, trigger: nil))
        }
        if changement { sauve() }
    }
}

/// La feuille des rappels : l'état de chacun face au compteur du journal,
/// et le formulaire pour en poser un nouveau.
struct EntretienView: View {
    /// Total des kilomètres du journal — le compteur de référence.
    let kmJournal: Double

    @ObservedObject private var entretien = Entretien.shared
    @Environment(\.dismiss) private var ferme

    @State private var choix = "Vidange"
    @State private var libre = ""
    @State private var dansKm = 8000

    private static let types = ["Vidange", "Contrôle technique", "Pneus", "Autre"]

    private var nomChoisi: String {
        choix == "Autre" ? libre : NSLocalizedString(choix, comment: "")
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    LabeledContent {
                        Text(verbatim: String(format: "%.0f km", kmJournal))
                            .font(.headline)
                    } label: {
                        Text("Compteur du journal")
                    }
                } footer: {
                    Text("Les kilomètres comptés sont ceux des trajets enregistrés : garde le journal vivant pour que les rappels le soient aussi.")
                }
                if !entretien.rappels.isEmpty {
                    Section("Rappels") {
                        ForEach(entretien.rappels) { r in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(verbatim: r.nom)
                                        .font(.subheadline.weight(.semibold))
                                    if kmJournal >= r.seuilKm {
                                        Text("C'est le moment !")
                                            .font(.caption)
                                            .foregroundStyle(.red)
                                    } else {
                                        Text(
                                            String(
                                                format: NSLocalizedString(
                                                    "Encore %.0f km", comment: ""),
                                                r.seuilKm - kmJournal))
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                Spacer()
                                Button {
                                    entretien.retire(r.id)
                                } label: {
                                    Image(systemName: "trash")
                                        .foregroundStyle(.red)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                Section("Nouveau rappel") {
                    Picker("Quoi ?", selection: $choix) {
                        ForEach(Self.types, id: \.self) { t in
                            Text(LocalizedStringKey(t)).tag(t)
                        }
                    }
                    if choix == "Autre" {
                        TextField("Nom du rappel", text: $libre)
                    }
                    Stepper(value: $dansKm, in: 500...50000, step: 500) {
                        Text(
                            String(
                                format: NSLocalizedString("Dans %d km", comment: ""),
                                dansKm))
                    }
                    Button {
                        entretien.ajoute(
                            nom: nomChoisi, dansKm: Double(dansKm), kmJournal: kmJournal)
                        libre = ""
                    } label: {
                        Label("Poser le rappel", systemImage: "bell.badge")
                    }
                    .disabled(choix == "Autre" && libre.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            .navigationTitle("Entretien")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { ferme() }
                }
            }
        }
        .presentationBackground(Color(.systemBackground))
    }
}
