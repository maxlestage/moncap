import StoreKit
import SwiftUI

/// Les formules de MonCap : **rien de gratuit passé quatorze jours**, pour
/// personne — c'est la règle de la maison.
///
/// - L'**essai** : quatorze jours, tout ouvert, ancré à la date de création
///   du compte **côté serveur** — l'horloge d'un téléphone se triche, pas la
///   nôtre. La date est persistée : l'essai se compte aussi hors ligne.
/// - **MonCap Plus (3,99 €/mois)** : l'app entière — guidage, comparateur,
///   transports, régions hors ligne, GPX.
/// - **MonCap Pro (9,99 €/mois)** : tout Plus, plus les outils de ceux qui
///   roulent pour travailler — tournées optimisées, journal kilométrique
///   CSV, facturation PDF.
/// - Les **invités** de la maison (liste PLUS_OFFERT du serveur) ont tout,
///   offert : c'est un choix du patron, pas une gratuité.
///
/// StoreKit 2 : l'état vient de `Transaction.currentEntitlements`, les deux
/// produits vivent dans le même groupe d'abonnement — Apple gère seul le
/// passage de l'un à l'autre.
@MainActor
final class MonCapPlus: ObservableObject {
    static let shared = MonCapPlus()

    /// Les identifiants, à créer tels quels dans App Store Connect, dans le
    /// même groupe. Pas d'offre d'essai Apple : l'essai est celui du compte.
    static let produitPlusID = "moncap.plus.mensuel"
    static let produitProID = "moncap.pro.mensuel"

    enum Niveau {
        case aucun, plus, pro
    }

    /// Le niveau d'abonnement StoreKit (hors essai, hors offert).
    @Published private(set) var niveau: Niveau = .aucun
    @Published private(set) var produits: [Product] = []
    /// Fin de la période en cours de l'abonnement, datée par Apple.
    @Published private(set) var renouvellement: Date?
    /// Plus offert par la maison (liste d'invités). Persisté.
    private var offert = UserDefaults.standard.bool(forKey: "moncap.plus.offert")
    /// Création du compte (secondes Unix), apprise du serveur, persistée —
    /// zéro tant que le serveur n'a pas répondu une première fois.
    @Published private(set) var creeLe =
        UserDefaults.standard.double(forKey: "moncap.compte.creeLe")

    private var veille: Task<Void, Never>?

    private init() {
        veille = Task { [weak self] in
            await self?.charge()
            for await maj in StoreKit.Transaction.updates {
                if case .verified(let t) = maj { await t.finish() }
                await self?.rafraichitDroit()
            }
        }
    }

    // MARK: - L'essai et les accès

    private static let dureeEssai: Double = 14 * 86_400

    /// Vrai pendant les quatorze jours. Tant que le serveur n'a jamais dit
    /// la date du compte, on ne bloque pas : la toute première ouverture
    /// hors ligne n'a pas à punir.
    var essaiEnCours: Bool {
        guard creeLe > 0 else { return true }
        return Date().timeIntervalSince1970 < creeLe + Self.dureeEssai
    }

    /// Jours d'essai restants, arrondis vers le haut — « il reste 1 jour »
    /// jusqu'au bout. Nil tant que la date du compte n'est pas connue.
    var joursEssaiRestants: Int? {
        guard creeLe > 0 else { return nil }
        let restant = creeLe + Self.dureeEssai - Date().timeIntervalSince1970
        return max(0, Int((restant / 86_400).rounded(.up)))
    }

    /// L'app entière — guidage, comparateur, transports, hors ligne, GPX,
    /// tournées : l'essai, n'importe quel abonnement, ou l'invitation.
    var accesPlus: Bool { essaiEnCours || niveau != .aucun || offert }

    /// Les outils Pro — journal kilométrique, facturation : l'essai (qui
    /// montre tout), Pro, ou l'invitation.
    var accesPro: Bool { essaiEnCours || niveau == .pro || offert }

    /// Vrai quand tout vient de la liste d'invités, pas d'un abonnement.
    var estOffert: Bool { offert && niveau == .aucun }

    // MARK: - Le compte

    /// Le serveur a répondu : la date de création ancre l'essai, la liste
    /// d'invités dit l'offert. Appelé à chaque lecture du compte.
    func majCompte(creeLe nouveau: Double?, offert nouvelOffert: Bool) {
        if let nouveau, nouveau > 0, nouveau != creeLe {
            creeLe = nouveau
            UserDefaults.standard.set(nouveau, forKey: "moncap.compte.creeLe")
        }
        if nouvelOffert != offert {
            offert = nouvelOffert
            UserDefaults.standard.set(nouvelOffert, forKey: "moncap.plus.offert")
        }
        objectWillChange.send()
    }

    /// La déconnexion : l'appareil oublie ce que le serveur avait dit du
    /// compte — la date d'essai et l'invitation appartiennent au compte,
    /// pas au téléphone. Sans cela, l'offert d'un invité déconnecté ou
    /// l'essai expiré d'un ancien compte collerait au prochain connecté.
    /// Retour à la grâce « serveur jamais entendu » : elle ne dure que
    /// jusqu'à la première lecture du compte suivant, et se connecter
    /// exige de toute façon le réseau.
    func oublieCompte() {
        creeLe = 0
        offert = false
        UserDefaults.standard.removeObject(forKey: "moncap.compte.creeLe")
        UserDefaults.standard.removeObject(forKey: "moncap.plus.offert")
    }

    // MARK: - La boutique

    func charge() async {
        produits =
            (try? await Product.products(for: [Self.produitPlusID, Self.produitProID])) ?? []
        await rafraichitDroit()
    }

    var produitPlus: Product? { produits.first { $0.id == Self.produitPlusID } }
    var produitPro: Product? { produits.first { $0.id == Self.produitProID } }

    /// Prix affichables — ceux de la boutique quand elle a répondu, les
    /// valeurs de lancement sinon : le paywall reste lisible hors ligne.
    var prixPlus: String { produitPlus?.displayPrice ?? "3,99 €" }
    var prixPro: String { produitPro?.displayPrice ?? "9,99 €" }

    func rafraichitDroit() async {
        var n = Niveau.aucun
        var fin: Date?
        for await droit in StoreKit.Transaction.currentEntitlements {
            guard case .verified(let t) = droit, t.revocationDate == nil else { continue }
            if t.productID == Self.produitProID {
                n = .pro
                fin = t.expirationDate
            } else if t.productID == Self.produitPlusID, n != .pro {
                n = .plus
                fin = t.expirationDate
            }
        }
        niveau = n
        renouvellement = fin
    }

    /// Lance l'achat du produit donné. L'annulation n'est pas une erreur.
    func achete(_ produit: Product) async -> Bool {
        if let resultat = try? await produit.purchase(),
            case .success(let verif) = resultat,
            case .verified(let t) = verif
        {
            await t.finish()
        }
        await rafraichitDroit()
        return niveau != .aucun
    }

    func restaure() async {
        try? await AppStore.sync()
        await rafraichitDroit()
    }
}

/// Ce que chaque formule contient — le paywall et le site racontent pareil.
struct FormuleAtout: Identifiable {
    /// L'icône identifie l'atout : le titre est une clé localisée, pas une
    /// chaîne, et chaque liste n'utilise jamais deux fois la même icône.
    var id: String { icone }
    let icone: String
    let titre: LocalizedStringKey
}

enum Formules {
    static let plus: [FormuleAtout] = [
        .init(icone: "map.fill", titre: "L'app entière : guidage, comparateur, transports"),
        .init(icone: "arrow.down.circle.fill", titre: "212 régions hors ligne"),
        .init(
            icone: "point.topleft.down.curvedto.point.bottomright.up",
            titre: "Import de traces GPX"),
    ]
    static let pro: [FormuleAtout] = [
        .init(icone: "checkmark.seal.fill", titre: "Tout MonCap Plus"),
        .init(icone: "arrow.triangle.swap", titre: "Tournées optimisées"),
        .init(icone: "doc.text.fill", titre: "Journal kilométrique exportable (CSV)"),
        .init(icone: "eurosign.circle.fill", titre: "Facturation PDF des trajets"),
    ]
}

/// Le paywall : l'essai qui reste, les deux formules, l'achat, la
/// restauration. En mode « bloquant » (essai terminé), il ne se ferme pas —
/// rien de gratuit passé quatorze jours, pour personne.
struct PaywallView: View {
    var bloquant = false
    /// En mode bloquant, la seule sortie honnête : entrer avec un autre
    /// compte (l'invité, le bon abonné). Sans elle, un mauvais compte serait
    /// un piège sans porte.
    var auth: AuthStore? = nil

    @ObservedObject private var plus = MonCapPlus.shared
    @Environment(\.dismiss) private var ferme
    @State private var enCours = false
    /// Feuille système des codes promo App Store.
    @State private var montreCode = false
    @State private var montreConnexion = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(bloquant ? "Ton essai est terminé" : "Choisis ta formule")
                            .font(.largeTitle.bold())
                        if let jours = plus.joursEssaiRestants, plus.essaiEnCours {
                            Text(
                                String(
                                    format: NSLocalizedString(
                                        "Il reste %d jour(s) d'essai — tout est ouvert.",
                                        comment: ""), jours))
                                .foregroundStyle(.secondary)
                        } else {
                            Text("Quatorze jours pour tout essayer, puis une formule — tout est payant, c'est la règle.")
                                .foregroundStyle(.secondary)
                        }
                    }

                    carte(
                        nom: "MonCap Plus", prix: plus.prixPlus,
                        atouts: Formules.plus, produit: plus.produitPlus,
                        actif: plus.niveau == .plus, teinte: .indigo)
                    carte(
                        nom: "MonCap Pro", prix: plus.prixPro,
                        atouts: Formules.pro, produit: plus.produitPro,
                        actif: plus.niveau == .pro, teinte: .teal)

                    if plus.produits.isEmpty {
                        Text("La boutique ne répond pas — réessaie avec du réseau.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }

                    VStack(spacing: 10) {
                        HStack(spacing: 16) {
                            Button("Restaurer les achats") {
                                Task {
                                    await plus.restaure()
                                    if plus.niveau != .aucun, !bloquant { ferme() }
                                }
                            }
                            Button("J'ai un code") { montreCode = true }
                            if bloquant, auth != nil {
                                Button("Changer de compte") { montreConnexion = true }
                            }
                        }
                        .font(.caption)

                        Text("Abonnement mensuel, renouvelé automatiquement, résiliable à tout moment dans les réglages iOS.")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                            .frame(maxWidth: .infinity)

                        HStack(spacing: 14) {
                            Link(
                                "Confidentialité",
                                destination: URL(
                                    string:
                                        "https://moncap-c41a5aaf07e8.herokuapp.com/confidentialite"
                                )!)
                            Link(
                                "Conditions",
                                destination: URL(
                                    string:
                                        "https://www.apple.com/legal/internet-services/itunes/dev/stdeula/"
                                )!)
                        }
                        .font(.caption2)
                    }
                }
                .padding(20)
            }
            .toolbar {
                if !bloquant {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button("Fermer") { ferme() }
                    }
                }
            }
        }
        .interactiveDismissDisabled(bloquant)
        .presentationBackground(Color(.systemBackground))
        .task { if plus.produits.isEmpty { await plus.charge() } }
        .offerCodeRedemption(isPresented: $montreCode) { _ in
            Task {
                await plus.rafraichitDroit()
                if plus.niveau != .aucun, !bloquant { ferme() }
            }
        }
        .sheet(isPresented: $montreConnexion) {
            if let auth { LoginView(auth: auth) }
        }
    }

    /// La carte d'une formule : nom, prix mensuel, atouts, bouton.
    private func carte(
        nom: String, prix: String, atouts: [FormuleAtout], produit: Product?,
        actif: Bool, teinte: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline) {
                Text(verbatim: nom).font(.title3.bold())
                Spacer()
                Text(String(format: NSLocalizedString("%@ / mois", comment: ""), prix))
                    .font(.headline)
                    .foregroundStyle(teinte)
            }
            ForEach(atouts) { a in
                HStack(spacing: 10) {
                    Image(systemName: a.icone)
                        .font(.subheadline)
                        .foregroundStyle(teinte)
                        .frame(width: 24)
                    Text(a.titre).font(.subheadline)
                }
            }
            Button {
                guard let produit else { return }
                enCours = true
                Task {
                    if await plus.achete(produit), !bloquant { ferme() }
                    enCours = false
                }
            } label: {
                Text(actif ? "Ta formule actuelle" : "Choisir")
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 4)
            }
            .buttonStyle(.borderedProminent)
            .tint(teinte)
            .disabled(enCours || produit == nil || actif)
        }
        .padding(16)
        .background(teinte.opacity(0.08), in: RoundedRectangle(cornerRadius: 14))
    }
}
