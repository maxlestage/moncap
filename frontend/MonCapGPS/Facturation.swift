import SwiftUI
import UIKit

/// La facturation des trajets : une facture PDF par période, bâtie sur le
/// journal des trajets enregistrés, au taux kilométrique choisi.
///
/// Tout se passe sur l'appareil : les trajets sont déjà là, le PDF est
/// dessiné par UIKit, le partage passe par la feuille iOS. Aucun serveur, et
/// donc utilisable hors ligne comme le reste — un livreur ne facture pas
/// moins parce que la vallée n'a pas de réseau.
///
/// L'app ne prétend pas être un logiciel de comptabilité : elle produit un
/// document propre — émetteur, client, période, détail des trajets, total —
/// et laisse les mentions réglementaires à un champ libre, parce qu'elles
/// dépendent du statut de chacun (TVA, franchise, etc.).
struct FacturationView: View {
    let trajets: [Trip]

    // L'émetteur, persisté : on ne retape pas son SIRET à chaque facture.
    @AppStorage("moncap.facture.emetteur") private var emetteur = ""
    @AppStorage("moncap.facture.adresse") private var adresse = ""
    @AppStorage("moncap.facture.siret") private var siret = ""
    @AppStorage("moncap.facture.taux") private var taux = 0.5
    @AppStorage("moncap.facture.mention") private var mention = ""

    @State private var client = ""
    @State private var periode = Date()
    @State private var pdf: URL?

    @Environment(\.dismiss) private var ferme

    /// Les trajets du mois choisi, du plus ancien au plus récent.
    private var trajetsDuMois: [Trip] {
        let cal = Calendar.current
        return
            trajets
            .filter { cal.isDate($0.date, equalTo: periode, toGranularity: .month) }
            .sorted { $0.created_at < $1.created_at }
    }

    private var totalKm: Double { trajetsDuMois.reduce(0) { $0 + $1.distance_km } }

    var body: some View {
        NavigationStack {
            Form {
                Section("Émetteur") {
                    TextField("Nom ou entreprise", text: $emetteur)
                    TextField("Adresse", text: $adresse)
                    TextField("SIRET (optionnel)", text: $siret)
                }
                Section("Facture") {
                    TextField("Client", text: $client)
                    DatePicker(
                        "Mois facturé", selection: $periode,
                        displayedComponents: [.date])
                    HStack {
                        Text("Taux par kilomètre")
                        Spacer()
                        TextField(
                            "", value: $taux,
                            format: .number.precision(.fractionLength(2))
                        )
                        .keyboardType(.decimalPad)
                        .multilineTextAlignment(.trailing)
                        .frame(width: 70)
                        Text(verbatim: "€/km").foregroundStyle(.secondary)
                    }
                    TextField("Mention libre (TVA, franchise…)", text: $mention, axis: .vertical)
                }
                Section {
                    LabeledContent {
                        Text(verbatim: String(format: "%.1f km · %.2f €", totalKm, totalKm * taux))
                            .font(.headline)
                    } label: {
                        Text(String(format: NSLocalizedString("%d trajet(s) sur la période", comment: ""), trajetsDuMois.count))
                    }
                    Button {
                        pdf = generePDF()
                    } label: {
                        Label("Générer la facture PDF", systemImage: "doc.badge.arrow.up")
                    }
                    .disabled(trajetsDuMois.isEmpty || emetteur.isEmpty)
                    if let pdf {
                        ShareLink(item: pdf) {
                            Label("Partager la facture", systemImage: "square.and.arrow.up")
                        }
                    }
                }
            }
            .navigationTitle("Facturation")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { ferme() }
                }
            }
        }
        // Fond opaque : pas de verre sur un document qu'on remplit.
        .presentationBackground(Color(.systemBackground))
    }

    /// Dessine la facture en PDF A4 et l'écrit dans un fichier temporaire.
    private func generePDF() -> URL? {
        let format = DateFormatter()
        format.locale = Locale(identifier: Bundle.main.preferredLocalizations.first ?? "fr")
        format.dateFormat = "LLLL yyyy"
        let moisTitre = format.string(from: periode).capitalized
        let numero = {
            let f = DateFormatter()
            f.dateFormat = "yyyy-MM"
            return f.string(from: periode)
        }()
        let jour = DateFormatter()
        jour.locale = format.locale
        jour.dateFormat = "dd/MM"

        // A4 en points (72 dpi).
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

            ligne(NSLocalizedString("FACTURE", comment: ""), taille: 24, graisse: .bold)
            ligne(
                String(format: NSLocalizedString("N° %@ · %@", comment: ""), numero, moisTitre),
                taille: 12, gris: true)
            y += 14

            ligne(emetteur, taille: 12, graisse: .semibold)
            if !adresse.isEmpty { ligne(adresse, gris: true) }
            if !siret.isEmpty {
                ligne(String(format: NSLocalizedString("SIRET %@", comment: ""), siret), gris: true)
            }
            y += 10
            if !client.isEmpty {
                ligne(NSLocalizedString("Facturé à :", comment: ""), gris: true)
                ligne(client, taille: 12, graisse: .semibold)
            }
            y += 16

            // Le tableau : date, trajet, distance, montant.
            ligne(NSLocalizedString("Date", comment: ""), taille: 10, graisse: .semibold, avance: false)
            ligne(NSLocalizedString("Trajet", comment: ""), taille: 10, graisse: .semibold, x: 110, avance: false)
            ligne("km", taille: 10, graisse: .semibold, x: 420, avance: false)
            ligne("€", taille: 10, graisse: .semibold, x: 490)
            y += 2

            for t in trajetsDuMois {
                // Nouvelle page quand la place manque.
                if y > page.height - marge - 80 {
                    ctx.beginPage()
                    y = marge
                }
                let libelle = t.label.count > 52 ? String(t.label.prefix(52)) + "…" : t.label
                ligne(jour.string(from: t.date), taille: 10, avance: false)
                ligne(libelle, taille: 10, x: 110, avance: false)
                ligne(String(format: "%.1f", t.distance_km), taille: 10, x: 420, avance: false)
                ligne(String(format: "%.2f", t.distance_km * taux), taille: 10, x: 490)
            }

            y += 12
            ligne(
                String(
                    format: NSLocalizedString("Total : %.1f km × %.2f €/km = %.2f €", comment: ""),
                    totalKm, taux, totalKm * taux),
                taille: 13, graisse: .bold)
            if !mention.isEmpty {
                y += 8
                ligne(mention, taille: 9, gris: true)
            }
        }

        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("facture-\(numero).pdf")
        do {
            try donnees.write(to: url)
            return url
        } catch {
            return nil
        }
    }
}
