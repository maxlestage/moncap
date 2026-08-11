import SwiftUI

/// Barre de progression d'un téléchargement.
///
/// `ProgressView` linéaire d'iOS fait quatre points de haut, ne porte aucun
/// repère chiffré, et son animation ne dit rien du volume. Or ici un
/// téléchargement dure des minutes — cinq cents mégaoctets de graphe routier,
/// des milliers de tuiles — et l'attente n'est supportable que si l'on voit
/// qu'elle avance et où elle en est.
///
/// Trois choix méritent d'être dits :
///
/// Le pourcentage est en **chiffres à chasse fixe**. Sans cela « 9 % » puis
/// « 10 % » changent la largeur du texte, et le nombre tressaute à chaque
/// rafraîchissement — un défaut qu'on ne voit pas sur une capture, seulement
/// en regardant la barre vivre.
///
/// Le remplissage garde une **largeur minimale** : à 0,3 %, une largeur
/// strictement proportionnelle donne moins d'un point, donc rien du tout. Une
/// barre vide au démarrage se lit comme une barre en panne.
///
/// L'animation est portée par la **valeur**, pas par une boucle : le système
/// interpole entre deux relevés, ce qui absorbe les à-coups d'un transfert
/// réseau sans rien inventer sur son avancement.
struct BarreProgression: View {
    /// Avancement, de 0 à 1.
    let valeur: Double
    /// Ce qui se télécharge.
    let titre: LocalizedStringKey
    /// Précision facultative sous la barre — « 128 Mo sur 522 Mo ».
    var detail: String?

    /// Hauteur : assez épaisse pour se voir d'un coup d'œil au volant, assez
    /// fine pour ne pas écraser le panneau.
    private let epaisseur: CGFloat = 10

    private var borne: Double { min(max(valeur, 0), 1) }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(titre)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Text(verbatim: "\(Int(borne * 100)) %")
                    .font(.subheadline.weight(.semibold))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }

            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(Color.primary.opacity(0.12))
                    Capsule()
                        .fill(
                            LinearGradient(
                                colors: [Color.accentColor, Color.accentColor.opacity(0.65)],
                                startPoint: .leading,
                                endPoint: .trailing)
                        )
                        .frame(width: max(epaisseur, geo.size.width * borne))
                }
            }
            .frame(height: epaisseur)
            .animation(.easeInOut(duration: 0.35), value: borne)

            if let detail {
                // Donnée mesurée, déjà mise en forme : `Text(String)` est
                // verbatim, et c'est voulu.
                Text(detail)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
        // Une barre annoncée comme un simple décor serait muette au lecteur
        // d'écran ; VoiceOver doit annoncer l'avancement, pas le dessin.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(titre))
        .accessibilityValue(Text(verbatim: "\(Int(borne * 100)) %"))
    }
}
