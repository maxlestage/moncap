import CoreLocation
import SwiftUI

/// La tournée du jour, collée d'un bloc : une adresse par ligne — depuis les
/// notes, un message ou un tableur — chaque ligne devient un arrêt.
///
/// Le géocodage passe par CLGeocoder, en série et sans se presser : trente
/// adresses au plus par collage, pour rester dans les clous du service. Les
/// introuvables sont montrées telles quelles, pas avalées — on corrige une
/// faute de frappe, on ne devine pas un arrêt.
struct CollerAdressesView: View {
    /// Les arrêts trouvés, remis à la tournée dans l'ordre du collage.
    var onAjoute: ([(nom: String, coord: CLLocationCoordinate2D)]) -> Void

    @Environment(\.dismiss) private var ferme
    @State private var texte = ""
    @State private var resultats: [Resultat] = []
    @State private var enCours = false

    struct Resultat: Identifiable {
        let id = UUID()
        let nom: String
        let coord: CLLocationCoordinate2D?
    }

    private static let maxAdresses = 30

    private var lignes: [String] {
        texte.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }

    private var trouvees: [(nom: String, coord: CLLocationCoordinate2D)] {
        resultats.compactMap { r in r.coord.map { (r.nom, $0) } }
    }

    private var introuvables: Int { resultats.filter { $0.coord == nil }.count }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextEditor(text: $texte)
                        .frame(minHeight: 140)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                } header: {
                    Text("Une adresse par ligne")
                } footer: {
                    Text("Choisis d'abord la destination finale, puis colle les arrêts — ville comprise, jusqu'à trente lignes. « Au plus court » les remettra dans le bon ordre.")
                }
                if !resultats.isEmpty {
                    Section("Adresses trouvées") {
                        ForEach(resultats) { r in
                            HStack(spacing: 8) {
                                Image(
                                    systemName: r.coord == nil
                                        ? "xmark.circle.fill" : "checkmark.circle.fill"
                                )
                                .foregroundStyle(r.coord == nil ? .red : .green)
                                Text(verbatim: r.nom).lineLimit(1)
                            }
                        }
                        if introuvables > 0 {
                            Text(
                                String(
                                    format: NSLocalizedString(
                                        "%d adresse(s) introuvable(s) — précise la ville et recommence.",
                                        comment: ""), introuvables))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                Section {
                    if resultats.isEmpty {
                        Button {
                            Task { await geocode() }
                        } label: {
                            if enCours {
                                HStack(spacing: 10) {
                                    ProgressView()
                                    Text("Recherche des adresses…")
                                }
                            } else {
                                Label("Chercher les adresses", systemImage: "magnifyingglass")
                            }
                        }
                        .disabled(lignes.isEmpty || enCours)
                    } else {
                        Button {
                            onAjoute(trouvees)
                            ferme()
                        } label: {
                            Label {
                                Text(
                                    String(
                                        format: NSLocalizedString(
                                            "Ajouter %d arrêt(s) à la tournée", comment: ""),
                                        trouvees.count))
                            } icon: {
                                Image(systemName: "plus.circle.fill")
                            }
                        }
                        .disabled(trouvees.isEmpty)
                        Button("Recommencer") { resultats = [] }
                    }
                }
            }
            .navigationTitle("Coller des adresses")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { ferme() }
                }
            }
        }
        .presentationBackground(Color(.systemBackground))
    }

    /// Géocode les lignes une à une — en série, avec une respiration : le
    /// service de géocodage n'aime pas les rafales, et trente adresses
    /// passent en une dizaine de secondes.
    private func geocode() async {
        enCours = true
        var acc: [Resultat] = []
        let geocodeur = CLGeocoder()
        for ligne in lignes.prefix(Self.maxAdresses) {
            let lieu = try? await geocodeur.geocodeAddressString(ligne).first
            acc.append(
                Resultat(
                    nom: ligne,
                    coord: lieu?.location?.coordinate))
            try? await Task.sleep(nanoseconds: 250_000_000)
        }
        resultats = acc
        enCours = false
    }
}
