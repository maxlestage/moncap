import StoreKit
import SwiftUI

/// L'abonnement MonCap Plus : 3,99 € par mois, mensuel seul.
///
/// Le cœur de l'app reste gratuit — chercher, comparer, être guidé. Plus
/// déverrouille ce qui coûte ou ce qui sert aux pros : les régions hors
/// ligne, l'import GPX, l'optimisation de tournées, le journal kilométrique
/// exportable et la facturation.
///
/// StoreKit 2 tout du long : l'état vient de `Transaction.currentEntitlements`
/// (vérifié par le système, pas par nous), les renouvellements et
/// remboursements arrivent par `Transaction.updates`. Aucun serveur à nous
/// dans la boucle : Apple est la source de vérité, y compris hors ligne où
/// StoreKit ressert le dernier état connu.
@MainActor
final class MonCapPlus: ObservableObject {
    static let shared = MonCapPlus()

    /// L'identifiant du produit, à créer tel quel dans App Store Connect
    /// (abonnement auto-renouvelable, 3,99 €/mois, essai de 14 jours).
    static let produitID = "moncap.plus.mensuel"

    /// Vrai quand Plus est acquis — par abonnement, ou offert. Les vues s'y
    /// accrochent.
    @Published private(set) var actif = false
    /// Le produit chargé depuis l'App Store — nil tant que la boutique n'a
    /// pas répondu (ou que le produit n'existe pas encore côté Apple).
    @Published private(set) var produit: Product?
    /// Plus offert par la maison : le serveur tient une liste d'invités, et
    /// le compte le dit à la connexion. Persisté — l'invitation tient aussi
    /// hors ligne, comme l'abonnement.
    private var offert = UserDefaults.standard.bool(forKey: "moncap.plus.offert")
    /// Fin de la période en cours (essai ou mois payé), datée par Apple —
    /// nil sans abonnement (y compris pour un Plus offert).
    @Published private(set) var renouvellement: Date?
    /// Vrai pendant la période d'essai gratuite.
    @Published private(set) var enEssai = false

    private var veille: Task<Void, Never>?

    private init() {
        veille = Task { [weak self] in
            await self?.charge()
            // Les renouvellements, remboursements et achats faits ailleurs
            // (autre appareil, réglages iOS) arrivent par ce flux.
            for await maj in StoreKit.Transaction.updates {
                if case .verified(let t) = maj { await t.finish() }
                await self?.rafraichitDroit()
            }
        }
    }

    func charge() async {
        produit = try? await Product.products(for: [Self.produitID]).first
        await rafraichitDroit()
    }

    func rafraichitDroit() async {
        var a = false
        var fin: Date?
        var essai = false
        for await droit in StoreKit.Transaction.currentEntitlements {
            if case .verified(let t) = droit,
                t.productID == Self.produitID, t.revocationDate == nil
            {
                a = true
                fin = t.expirationDate
                essai = t.offer?.paymentMode == .freeTrial
            }
        }
        actif = a || offert
        renouvellement = fin
        enEssai = essai
    }

    /// Jours restants de la période en cours, arrondis vers le haut — on dit
    /// « il reste 1 jour » jusqu'au bout, jamais « 0 » avant l'heure.
    var joursRestants: Int? {
        guard let renouvellement else { return nil }
        return max(0, Int((renouvellement.timeIntervalSinceNow / 86_400).rounded(.up)))
    }

    /// Vrai quand Plus vient de la liste d'invités, pas d'un abonnement.
    var estOffert: Bool { actif && renouvellement == nil && offert }

    /// Le serveur a dit si ce compte est invité : on le retient, et l'état
    /// visible suit. Appelé à chaque lecture du compte — être retiré de la
    /// liste se voit donc aussi.
    func majOffert(_ nouveau: Bool) {
        guard nouveau != offert else { return }
        offert = nouveau
        UserDefaults.standard.set(nouveau, forKey: "moncap.plus.offert")
        Task { await rafraichitDroit() }
    }

    /// Lance l'achat. Rend vrai si l'abonnement est actif au retour —
    /// l'annulation de la feuille Apple n'est pas une erreur, juste un non.
    func achete() async -> Bool {
        guard let produit else { return false }
        if let resultat = try? await produit.purchase(),
            case .success(let verif) = resultat,
            case .verified(let t) = verif
        {
            await t.finish()
        }
        await rafraichitDroit()
        return actif
    }

    /// Resynchronise avec l'App Store (« Restaurer les achats »).
    func restaure() async {
        try? await AppStore.sync()
        await rafraichitDroit()
    }

    /// Prix affichable : celui de la boutique quand elle a répondu, la
    /// valeur de lancement sinon — le paywall reste lisible hors ligne.
    var prixAffiche: String {
        if let produit { return produit.displayPrice }
        return "3,99 €"
    }

    /// L'essai gratuit annoncé par la boutique, s'il existe.
    var essaiGratuit: Bool {
        produit?.subscription?.introductoryOffer?.paymentMode == .freeTrial
    }
}

/// Ce que Plus déverrouille — la liste vit ici pour que le paywall et les
/// points de blocage racontent la même chose.
enum PlusAtout: CaseIterable {
    case horsLigne, tournees, journal, facturation, gpx

    var icone: String {
        switch self {
        case .horsLigne: return "arrow.down.circle.fill"
        case .tournees: return "arrow.triangle.swap"
        case .journal: return "doc.text.fill"
        case .facturation: return "eurosign.circle.fill"
        case .gpx: return "point.topleft.down.curvedto.point.bottomright.up"
        }
    }

    var titre: LocalizedStringKey {
        switch self {
        case .horsLigne: return "Les régions hors ligne"
        case .tournees: return "Tournées optimisées"
        case .journal: return "Journal kilométrique exportable"
        case .facturation: return "Facturation des trajets"
        case .gpx: return "Import de traces GPX"
        }
    }

    var detail: LocalizedStringKey {
        switch self {
        case .horsLigne:
            return "212 régions d'Europe téléchargeables : itinéraires, lieux et limites de vitesse sans réseau."
        case .tournees:
            return "L'ordre le plus court calculé pour toutes tes étapes, même nombreuses — hors ligne compris."
        case .journal:
            return "Tes trajets en CSV — dates, distances, durées — prêts pour la comptabilité."
        case .facturation:
            return "Une facture PDF par période, au taux kilométrique de ton choix."
        case .gpx:
            return "Une randonnée ou un parcours d'une autre app devient ton itinéraire, consignes comprises."
        }
    }
}

/// Le paywall : ce que Plus apporte, le prix, l'achat, la restauration.
/// Présenté en feuille quand on touche une fonction verrouillée.
struct PaywallView: View {
    @ObservedObject private var plus = MonCapPlus.shared
    @Environment(\.dismiss) private var ferme
    @State private var enCours = false
    /// Feuille système des codes promo App Store (codes d'offre créés dans
    /// App Store Connect — de quoi inviter sans faire payer).
    @State private var montreCode = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("MonCap Plus")
                            .font(.largeTitle.bold())
                        Text("L'essentiel reste gratuit. Plus déverrouille le hors ligne et les outils pro.")
                            .foregroundStyle(.secondary)
                    }

                    VStack(alignment: .leading, spacing: 14) {
                        ForEach(PlusAtout.allCases, id: \.self) { a in
                            HStack(alignment: .top, spacing: 12) {
                                Image(systemName: a.icone)
                                    .font(.title3)
                                    .foregroundStyle(.indigo)
                                    .frame(width: 30)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(a.titre).font(.subheadline.weight(.semibold))
                                    Text(a.detail)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                    .padding(16)
                    .background(Color.indigo.opacity(0.08), in: RoundedRectangle(cornerRadius: 14))

                    VStack(spacing: 10) {
                        Button {
                            enCours = true
                            Task {
                                if await plus.achete() { ferme() }
                                enCours = false
                            }
                        } label: {
                            VStack(spacing: 2) {
                                if plus.essaiGratuit {
                                    Text("14 jours d'essai gratuit")
                                        .font(.headline)
                                    Text(String(format: NSLocalizedString("puis %@ par mois", comment: ""), plus.prixAffiche))
                                        .font(.caption)
                                } else {
                                    Text(String(format: NSLocalizedString("%@ par mois", comment: ""), plus.prixAffiche))
                                        .font(.headline)
                                }
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 6)
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(.indigo)
                        .disabled(enCours || plus.produit == nil)

                        if plus.produit == nil {
                            Text("La boutique ne répond pas — réessaie avec du réseau.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }

                        HStack(spacing: 16) {
                            Button("Restaurer les achats") {
                                Task { await plus.restaure(); if plus.actif { ferme() } }
                            }
                            Button("J'ai un code") { montreCode = true }
                        }
                        .font(.caption)

                        Text("Abonnement mensuel, renouvelé automatiquement, résiliable à tout moment dans les réglages iOS.")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)

                        HStack(spacing: 14) {
                            Link(
                                "Confidentialité",
                                destination: URL(string: "https://moncap-c41a5aaf07e8.herokuapp.com/confidentialite")!)
                            Link(
                                "Conditions",
                                destination: URL(string: "https://www.apple.com/legal/internet-services/itunes/dev/stdeula/")!)
                        }
                        .font(.caption2)
                    }
                }
                .padding(20)
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { ferme() }
                }
            }
        }
        // Fond opaque : le verre d'iOS 26+ rendrait le paywall illisible sur
        // la carte — et un écran de vente illisible ne vend rien.
        .presentationBackground(Color(.systemBackground))
        .task { if plus.produit == nil { await plus.charge() } }
        // La feuille système de saisie de code (offres App Store Connect) :
        // au retour, on relit les droits — le code vient peut-être d'ouvrir
        // l'abonnement.
        .offerCodeRedemption(isPresented: $montreCode) { _ in
            Task {
                await plus.rafraichitDroit()
                if plus.actif { ferme() }
            }
        }
    }
}
