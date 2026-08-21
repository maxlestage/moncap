import SwiftUI
import UIKit

/// « Mon année » : ce que le journal raconte en quatre grands chiffres et
/// deux anecdotes — et une image aux couleurs de la maison, prête à
/// partager. Le bouche-à-oreille le plus honnête : des kilomètres vrais.
struct RecapAnnuelView: View {
    let trajets: [Trip]

    @State private var annee = Calendar.current.component(.year, from: Date())
    @State private var imagePartage: URL?
    @Environment(\.dismiss) private var ferme

    private var annees: [Int] {
        let cal = Calendar.current
        var tout = Set(trajets.map { cal.component(.year, from: $0.date) })
        tout.insert(cal.component(.year, from: Date()))
        return tout.sorted(by: >)
    }

    private var trajetsAnnee: [Trip] {
        let cal = Calendar.current
        return trajets.filter { cal.component(.year, from: $0.date) == annee }
    }

    private var totalKm: Double { trajetsAnnee.reduce(0) { $0 + $1.distance_km } }
    private var totalHeures: Double {
        trajetsAnnee.reduce(0) { $0 + $1.duration_min } / 60
    }
    private var joursRoules: Int {
        let cal = Calendar.current
        return Set(trajetsAnnee.map { cal.startOfDay(for: $0.date) }).count
    }
    private var plusLong: Trip? {
        trajetsAnnee.max { $0.distance_km < $1.distance_km }
    }
    /// (nom du mois, km) du mois le plus roulé — nil si l'année est vide.
    private var meilleurMois: (nom: String, km: Double)? {
        let cal = Calendar.current
        var parMois = [Double](repeating: 0, count: 12)
        for t in trajetsAnnee {
            parMois[cal.component(.month, from: t.date) - 1] += t.distance_km
        }
        guard let maxKm = parMois.max(), maxKm > 0,
            let index = parMois.firstIndex(of: maxKm)
        else { return nil }
        let f = DateFormatter()
        f.locale = Locale(identifier: Bundle.main.preferredLocalizations.first ?? "fr")
        let noms = f.standaloneMonthSymbols ?? []
        let nom = index < noms.count ? noms[index].capitalized : String(index + 1)
        return (nom, maxKm)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 16) {
                    carte
                        .clipShape(RoundedRectangle(cornerRadius: 18))

                    if annees.count > 1 {
                        Picker("Année", selection: $annee) {
                            ForEach(annees, id: \.self) { a in
                                Text(verbatim: String(a)).tag(a)
                            }
                        }
                        .pickerStyle(.segmented)
                    }

                    if trajetsAnnee.isEmpty {
                        Text("Rien au journal cette année-là — roule, on recompte.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else {
                        Button {
                            partage()
                        } label: {
                            Label("Préparer l'image", systemImage: "photo")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                        if let imagePartage {
                            ShareLink(item: imagePartage) {
                                Label("Partager mon année", systemImage: "square.and.arrow.up")
                                    .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(.bordered)
                        }
                    }
                }
                .padding(16)
            }
            .navigationTitle("Mon année")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { ferme() }
                }
            }
        }
        .presentationBackground(Color(.systemBackground))
        .onChange(of: annee) { imagePartage = nil }
    }

    /// La carte façon écran de garde : dégradé marine → bleu, gros chiffres.
    /// C'est elle, telle quelle, qui devient l'image partagée.
    private var carte: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(String(format: NSLocalizedString("Mon année %d", comment: ""), annee))
                .font(.title2.bold())
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(verbatim: String(format: "%.0f", totalKm))
                    .font(.system(size: 52, weight: .heavy, design: .rounded))
                Text(verbatim: "km")
                    .font(.title3.weight(.semibold))
                    .opacity(0.8)
            }
            HStack(spacing: 18) {
                chiffre(String(format: "%.0f", totalHeures), sous: "heures de route")
                chiffre("\(trajetsAnnee.count)", sous: "trajets")
                chiffre("\(joursRoules)", sous: "jours au volant")
            }
            if let plusLong {
                ligne(
                    "arrow.up.right.circle.fill",
                    String(
                        format: NSLocalizedString("Plus long : %@ — %.0f km", comment: ""),
                        plusLong.label, plusLong.distance_km))
            }
            if let meilleurMois {
                ligne(
                    "calendar.circle.fill",
                    String(
                        format: NSLocalizedString(
                            "Mois le plus roulé : %@ (%.0f km)", comment: ""),
                        meilleurMois.nom, meilleurMois.km))
            }
            HStack(spacing: 6) {
                Image(systemName: "location.north.circle.fill")
                Text(verbatim: "MonCap GPS")
                    .font(.footnote.weight(.bold))
            }
            .opacity(0.85)
            .padding(.top, 4)
        }
        .padding(22)
        .frame(maxWidth: .infinity, alignment: .leading)
        .foregroundStyle(.white)
        .background(
            LinearGradient(
                colors: [
                    Color(red: 0.055, green: 0.165, blue: 0.369),
                    Color(red: 0.10, green: 0.34, blue: 0.90),
                ],
                startPoint: .top, endPoint: .bottom))
    }

    private func chiffre(_ valeur: String, sous: LocalizedStringKey) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(verbatim: valeur)
                .font(.title2.weight(.bold))
            Text(sous)
                .font(.caption2)
                .opacity(0.8)
        }
    }

    private func ligne(_ icone: String, _ texte: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: icone)
            Text(verbatim: texte)
                .font(.subheadline.weight(.medium))
                .lineLimit(1)
        }
        .opacity(0.95)
    }

    /// Rend la carte en PNG net (échelle 3) dans un fichier temporaire.
    @MainActor
    private func partage() {
        let rendu = ImageRenderer(content: carte.frame(width: 430))
        rendu.scale = 3
        guard let ui = rendu.uiImage, let donnees = ui.pngData() else { return }
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("mon-annee-\(annee).png")
        try? donnees.write(to: url)
        imagePartage = url
    }
}
