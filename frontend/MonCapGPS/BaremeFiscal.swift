import SwiftUI
import UIKit

/// Le barème kilométrique de l'administration fiscale, appliqué au journal
/// des trajets : une attestation annuelle PDF, prête pour la déclaration.
///
/// Tout se passe sur l'appareil, comme la facturation : les trajets sont
/// déjà là, le PDF est dessiné par UIKit. L'app applique le barème en
/// vigueur (inchangé depuis l'imposition des revenus 2023) et le dit sur le
/// document — elle produit une pièce propre, pas un conseil fiscal.
enum BaremeFiscal {
    /// La puissance fiscale du véhicule, telle que la carte grise la dit.
    enum Puissance: Int, CaseIterable, Identifiable {
        case cv3 = 0, cv4, cv5, cv6, cv7

        var id: Int { rawValue }

        var libelle: LocalizedStringKey {
            switch self {
            case .cv3: return "3 CV et moins"
            case .cv4: return "4 CV"
            case .cv5: return "5 CV"
            case .cv6: return "6 CV"
            case .cv7: return "7 CV et plus"
            }
        }

        /// Les trois tranches du barème : jusqu'à 5 000 km, de 5 001 à
        /// 20 000 km (coefficient + constante), au-delà de 20 000 km.
        var tranches: (basse: Double, moyenne: Double, constante: Double, haute: Double) {
            switch self {
            case .cv3: return (0.529, 0.316, 1065, 0.370)
            case .cv4: return (0.606, 0.340, 1330, 0.407)
            case .cv5: return (0.636, 0.357, 1395, 0.427)
            case .cv6: return (0.665, 0.374, 1457, 0.447)
            case .cv7: return (0.697, 0.394, 1515, 0.470)
            }
        }
    }

    /// Le montant déductible pour `km` kilomètres sur l'année. Les véhicules
    /// électriques ont droit à une majoration de 20 %.
    static func montant(km: Double, puissance: Puissance, electrique: Bool) -> Double {
        let t = puissance.tranches
        let base: Double
        if km <= 5000 {
            base = km * t.basse
        } else if km <= 20000 {
            base = km * t.moyenne + t.constante
        } else {
            base = km * t.haute
        }
        return electrique ? base * 1.2 : base
    }
}

/// L'attestation : l'année, la puissance fiscale, le total du journal, et le
/// PDF qui va avec.
struct AttestationFiscaleView: View {
    let trajets: [Trip]

    // Partagés avec la facturation : on ne retape pas son identité.
    @AppStorage("moncap.facture.emetteur") private var emetteur = ""
    @AppStorage("moncap.facture.adresse") private var adresse = ""
    // Propres à l'attestation, persistés : la carte grise ne change pas.
    @AppStorage("moncap.fiscal.cv") private var cvBrut = BaremeFiscal.Puissance.cv5.rawValue
    @AppStorage("moncap.fiscal.electrique") private var electrique = false

    @State private var annee = Calendar.current.component(.year, from: Date())
    @State private var pdf: URL?

    @Environment(\.dismiss) private var ferme

    private var puissance: BaremeFiscal.Puissance {
        BaremeFiscal.Puissance(rawValue: cvBrut) ?? .cv5
    }

    /// Les années où le journal a quelque chose à dire, la plus récente
    /// d'abord — et au moins l'année en cours, pour que le sélecteur existe.
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

    private var montant: Double {
        BaremeFiscal.montant(km: totalKm, puissance: puissance, electrique: electrique)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Émetteur") {
                    TextField("Nom ou entreprise", text: $emetteur)
                    TextField("Adresse", text: $adresse)
                }
                Section("Véhicule") {
                    Picker("Puissance fiscale", selection: $cvBrut) {
                        ForEach(BaremeFiscal.Puissance.allCases) { p in
                            Text(p.libelle).tag(p.rawValue)
                        }
                    }
                    Toggle("Véhicule électrique (+20 %)", isOn: $electrique)
                }
                Section("Attestation") {
                    Picker("Année", selection: $annee) {
                        ForEach(annees, id: \.self) { a in
                            Text(verbatim: String(a)).tag(a)
                        }
                    }
                    LabeledContent {
                        Text(verbatim: String(format: "%.0f km · %.2f €", totalKm, montant))
                            .font(.headline)
                    } label: {
                        Text(
                            String(
                                format: NSLocalizedString(
                                    "%d trajet(s) au journal", comment: ""),
                                trajetsAnnee.count))
                    }
                    Button {
                        pdf = generePDF()
                    } label: {
                        Label("Générer l'attestation PDF", systemImage: "doc.badge.arrow.up")
                    }
                    .disabled(trajetsAnnee.isEmpty || emetteur.isEmpty)
                    if let pdf {
                        ShareLink(item: pdf) {
                            Label("Partager l'attestation", systemImage: "square.and.arrow.up")
                        }
                    }
                }
                Section {
                    Text("Barème kilométrique de l'administration fiscale, majoration électrique comprise le cas échéant. Le document se reporte dans la déclaration selon ta situation — ce n'est pas un conseil fiscal.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Attestation fiscale")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { ferme() }
                }
            }
        }
        .presentationBackground(Color(.systemBackground))
    }

    /// L'attestation en PDF A4 : identité, année, kilomètres par mois, total
    /// et montant au barème — même gabarit de dessin que la facture.
    private func generePDF() -> URL? {
        let locale = Locale(identifier: Bundle.main.preferredLocalizations.first ?? "fr")
        let cal = Calendar.current

        // Les kilomètres mois par mois : le détail qui rend le total lisible.
        var parMois = [Double](repeating: 0, count: 12)
        for t in trajetsAnnee {
            parMois[cal.component(.month, from: t.date) - 1] += t.distance_km
        }
        let nomsMois: [String] = {
            let f = DateFormatter()
            f.locale = locale
            return f.standaloneMonthSymbols ?? []
        }()

        let page = CGRect(x: 0, y: 0, width: 595, height: 842)
        let rendu = UIGraphicsPDFRenderer(bounds: page)
        let marge: CGFloat = 48

        let donnees = rendu.pdfData { ctx in
            ctx.beginPage()
            var y: CGFloat = marge

            func ligne(
                _ texte: String, taille: CGFloat = 11, graisse: UIFont.Weight = .regular,
                gris: Bool = false, x: CGFloat = marge, avance: Bool = true
            ) {
                let attrs: [NSAttributedString.Key: Any] = [
                    .font: UIFont.systemFont(ofSize: taille, weight: graisse),
                    .foregroundColor: gris ? UIColor.darkGray : UIColor.black,
                ]
                (texte as NSString).draw(at: CGPoint(x: x, y: y), withAttributes: attrs)
                if avance { y += taille + 7 }
            }

            ligne(
                NSLocalizedString("ATTESTATION DE FRAIS KILOMÉTRIQUES", comment: ""),
                taille: 20, graisse: .bold)
            ligne(
                String(format: NSLocalizedString("Année %d", comment: ""), annee),
                taille: 12, gris: true)
            y += 14

            ligne(emetteur, taille: 12, graisse: .semibold)
            if !adresse.isEmpty { ligne(adresse, gris: true) }
            y += 16

            ligne(NSLocalizedString("Mois", comment: ""), taille: 10, graisse: .semibold, avance: false)
            ligne("km", taille: 10, graisse: .semibold, x: 420)
            y += 2
            for (i, km) in parMois.enumerated() where km > 0 {
                let nom = i < nomsMois.count ? nomsMois[i].capitalized : String(i + 1)
                ligne(nom, taille: 10, avance: false)
                ligne(String(format: "%.1f", km), taille: 10, x: 420)
            }

            y += 12
            let libellePuissance: String = {
                switch puissance {
                case .cv3: return NSLocalizedString("3 CV et moins", comment: "")
                case .cv4: return "4 CV"
                case .cv5: return "5 CV"
                case .cv6: return "6 CV"
                case .cv7: return NSLocalizedString("7 CV et plus", comment: "")
                }
            }()
            ligne(
                String(
                    format: NSLocalizedString(
                        "Total : %.1f km · barème %@%@ = %.2f €", comment: ""),
                    totalKm, libellePuissance,
                    electrique ? NSLocalizedString(", électrique (+20 %)", comment: "") : "",
                    montant),
                taille: 13, graisse: .bold)
            y += 8
            ligne(
                NSLocalizedString(
                    "Kilomètres relevés par l'application MonCap GPS, barème kilométrique en vigueur. Ne vaut pas conseil fiscal.",
                    comment: ""),
                taille: 9, gris: true)
        }

        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("attestation-\(annee).pdf")
        do {
            try donnees.write(to: url)
            return url
        } catch {
            return nil
        }
    }
}
