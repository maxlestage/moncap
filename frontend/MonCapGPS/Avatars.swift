import SwiftUI
import UIKit

/// Les avatars : ce qui vous représente sur la carte, et ce que vos amis
/// voient de vous pendant un partage en direct.
///
/// Ils étaient quatorze, tous des voitures. À pied ou à vélo, l'app basculait
/// sur un pictogramme unique codé en dur — un bonhomme vert, un vélo bleu — et
/// les transports empruntaient celui du piéton. Choisir sa Ferrari puis partir
/// courir donnait le même bonhomme à tout le monde.
///
/// Chaque mode a maintenant les siens, et le choix se retient **par mode** :
/// la voiture garde sa Ferrari pendant que la marche garde son randonneur.
/// C'est la suite logique de la détection automatique — le mode change tout
/// seul, l'avatar doit suivre sans qu'on ait à y penser.
///
/// Deux natures cohabitent. Les voitures sont des illustrations du catalogue ;
/// les autres sont des symboles système posés sur un disque de couleur. Un
/// symbole n'est pas un pis-aller : il reste net à toutes les tailles, suit le
/// contraste élevé et le mode sombre, et ne pèse rien. Le jour où de vraies
/// illustrations arriveront pour la marche ou le vélo, elles prendront la
/// place du symbole sans que rien d'autre ne bouge.
///
/// Les identifiants voyagent jusqu'au serveur et jusqu'aux autres appareils :
/// **aucun ne change jamais**. On n'en ajoute que de nouveaux, et `walk` et
/// `bike` restent valides — un ami resté sur une version plus ancienne
/// continue de les envoyer.
enum Avatars {

    /// La teinte d'un avatar à symbole. Un type plutôt qu'une couleur brute :
    /// le disque se dessine en UIKit sur la carte et en SwiftUI dans les
    /// listes, et il faut la même couleur des deux côtés.
    enum Teinte {
        case vert, bleu, orange, rouge, indigo, violet, brun, sarcelle, rose, gris

        var ui: UIColor {
            switch self {
            case .vert: return .systemGreen
            case .bleu: return .systemBlue
            case .orange: return .systemOrange
            case .rouge: return .systemRed
            case .indigo: return .systemIndigo
            case .violet: return .systemPurple
            case .brun: return .systemBrown
            case .sarcelle: return .systemTeal
            case .rose: return .systemPink
            case .gris: return .systemGray
            }
        }

        var couleur: Color { Color(ui) }
    }

    /// Un avatar : son identifiant durable, son nom, le mode auquel il
    /// appartient, et de quoi le dessiner.
    struct Modele: Identifiable, Hashable {
        let id: String
        let libelle: String
        let mode: TravelMode
        /// Symbole système, ou nil quand l'avatar est une illustration du
        /// catalogue (voir `asset`).
        let symbole: String?
        let teinte: Teinte

        static func == (a: Modele, b: Modele) -> Bool { a.id == b.id }
        func hash(into h: inout Hasher) { h.combine(id) }
    }

    /// Tous les avatars, groupés par mode dans l'ordre où ils s'affichent.
    static let tous: [Modele] = voitures + marche + velos + transports

    // MARK: - Les quatre familles

    /// Les illustrations d'origine. Identifiants inchangés : ils sont déjà
    /// posés sur des comptes et voyagent entre appareils.
    private static let voitures: [Modele] = [
        img("green", "Combi vert"), img("orange", "Combi orange"),
        img("blue", "Combi bleu"), img("mint", "Combi menthe"),
        img("ferrari", "Ferrari"), img("alpine", "Alpine"),
        img("merc1", "Mercedes argent"), img("merc2", "Mercedes noire"),
        img("red1", "F1 rouge"),
        img("yarisGrey", "GR Yaris gris"), img("yarisWhite", "GR Yaris blanc"),
        img("yarisBlue", "GR Yaris bleu"),
        img("abarthWhite", "Abarth 500 blanc"), img("abarthRed", "Abarth 595 rouge"),
        sym("auto", "Voiture", .car, "car.fill", .rouge),
        sym("electrique", "Électrique", .car, "bolt.car.fill", .vert),
        sym("utilitaire", "Utilitaire", .car, "truck.box.fill", .gris),
    ]

    /// `walk` garde son identifiant d'origine : il est déjà diffusé.
    private static let marche: [Modele] = [
        sym("walk", "Marcheur", .walk, "figure.walk", .vert),
        sym("coureur", "Coureur", .walk, "figure.run", .orange),
        sym("randonneur", "Randonneur", .walk, "figure.hiking", .brun),
        sym("poussette", "Avec un enfant", .walk, "figure.and.child.holdinghands", .rose),
        sym("fauteuil", "Fauteuil roulant", .walk, "figure.roll", .indigo),
    ]

    private static let velos: [Modele] = [
        sym("bike", "Vélo", .bike, "bicycle", .bleu),
        sym("cycliste", "Cycliste", .bike, "figure.outdoor.cycle", .sarcelle),
        sym("trottinette", "Trottinette", .bike, "scooter", .violet),
    ]

    private static let transports: [Modele] = [
        sym("bus", "Bus", .transit, "bus.fill", .rouge),
        sym("tram", "Tram", .transit, "tram.fill", .orange),
        sym("train", "Train", .transit, "train.side.front.car", .indigo),
        sym("bateau", "Bateau", .transit, "ferry.fill", .bleu),
        sym("telepherique", "Téléphérique", .transit, "cablecar.fill", .vert),
    ]

    private static func img(_ id: String, _ libelle: String) -> Modele {
        Modele(id: id, libelle: libelle, mode: .car, symbole: nil, teinte: .gris)
    }

    private static func sym(
        _ id: String, _ libelle: String, _ mode: TravelMode,
        _ symbole: String, _ teinte: Teinte
    ) -> Modele {
        Modele(id: id, libelle: libelle, mode: mode, symbole: symbole, teinte: teinte)
    }

    // MARK: - Consultation

    private static let parID: [String: Modele] = Dictionary(
        uniqueKeysWithValues: tous.map { ($0.id, $0) })

    static func modele(_ id: String) -> Modele? { parID[id] }

    /// Les avatars proposés pour un mode, dans l'ordre d'affichage.
    static func pour(_ mode: TravelMode) -> [Modele] { tous.filter { $0.mode == mode } }

    /// L'avatar par défaut d'un mode — le premier de sa famille.
    static func defaut(_ mode: TravelMode) -> String {
        pour(mode).first?.id ?? "green"
    }

    /// Le nom affiché, ou l'identifiant brut s'il vient d'une version plus
    /// récente que la nôtre : mieux vaut un mot technique qu'un vide.
    static func libelle(_ id: String) -> String { parID[id]?.libelle ?? id }

    /// Nom de l'image du catalogue pour un avatar illustré.
    ///
    /// Le repli sur le combi vert couvre deux cas : un identifiant inconnu
    /// venu d'un ami mieux à jour, et un avatar à symbole demandé par erreur
    /// à cette fonction.
    static func asset(_ id: String) -> String {
        guard let m = parID[id], m.symbole == nil else { return "vanGreen" }
        return Self.assets[id] ?? "vanGreen"
    }

    private static let assets: [String: String] = [
        "green": "vanGreen", "orange": "vanOrange", "blue": "vanBlue", "mint": "vanMint",
        "ferrari": "carFerrari", "alpine": "carAlpine", "merc1": "carMerc1",
        "merc2": "carMerc2", "red1": "carRed1",
        "yarisGrey": "carYarisGrey", "yarisWhite": "carYarisWhite",
        "yarisBlue": "carYarisBlue", "abarthWhite": "carAbarthWhite",
        "abarthRed": "carAbarthRed",
    ]
}
