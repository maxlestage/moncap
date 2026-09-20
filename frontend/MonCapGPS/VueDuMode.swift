import Foundation

/// Ce que montre la vue de chaque mode de déplacement.
///
/// Les quatre modes partageaient une seule carte d'itinéraire, donc un seul
/// discours : « 23 min · 18.4 km · 12 virages · arr. 14:05 ». C'est le
/// discours de la voiture, servi à tout le monde.
///
/// Un piéton ne compte pas ses virages — il en fait trente et n'y pense pas —
/// mais une côte de quatre-vingts mètres décide de son chemin. Un cycliste
/// juge un trajet au dénivelé avant tout. Un automobiliste veut le carburant,
/// les péages et le nombre de virages, qui dit la simplicité du trajet. Et en
/// transports, ni les kilomètres ni les virages ne veulent dire quoi que ce
/// soit : seules comptent les lignes, les correspondances et l'heure.
///
/// Ce fichier tient la table de ces décisions au même endroit, plutôt que de
/// les laisser en conditions éparpillées dans la carte d'itinéraire — où
/// l'oubli d'un mode ne se voit pas.
extension TravelMode {

    /// Le nombre de virages dit la simplicité d'un trajet en voiture. À pied
    /// ou à vélo il ne dit rien : on tourne sans y penser.
    var montreVirages: Bool { self == .car }

    /// Le dénivelé décide du chemin à pied et à vélo. En voiture, quarante
    /// mètres de côte sont du bruit — le moteur ne s'en aperçoit pas.
    var montreDenivele: Bool { self == .walk || self == .bike }

    /// Carburant et péages : la voiture seule les paie.
    var montreCout: Bool { self == .car }

    /// Les étapes se chaînent partout sauf en transports : MOTIS ne sait pas
    /// enchaîner des arrêts intermédiaires.
    var montreEtapes: Bool { self != .transit }

    /// En transports, l'heure est un paramètre du calcul, pas un détail.
    var montreHoraires: Bool { self == .transit }

    /// Une ligne qui dit, sous le sélecteur, ce que cette vue-là met en avant.
    /// Le mode change, la carte change de discours : autant le nommer.
    var resumeDuMode: String {
        switch self {
        case .walk: return "Distances courtes et dénivelé"
        case .bike: return "Profil vélo : côtes et descentes"
        case .car: return "Carburant, péages et virages"
        case .transit: return "Lignes, correspondances et horaires"
        }
    }
}

/// La ligne qui résume un itinéraire, dans la langue de son mode.
///
/// Les transports ont la leur, bâtie ailleurs sur les correspondances et les
/// horaires réels — ici on ne traite que ce qui se mesure en distance.
enum ResumeItineraire {

    /// « 23 min · 1,8 km · arr. 14:05 », avec ou sans virages selon le mode.
    ///
    /// La distance passe par `Distance`, la règle de la maison : des mètres
    /// sous le kilomètre. C'est indifférent en voiture et décisif à pied —
    /// « 0.8 km » pour huit cents mètres de marche ne se dit pas.
    static func ligne(
        mode: TravelMode,
        minutes: Double,
        km: Double,
        virages: Int,
        arrivee: String
    ) -> String {
        let distance = Distance.courteKm(km)
        if mode.montreVirages {
            return String(
                format: NSLocalizedString("%.0f min · %@ · %d virages · arr. %@", comment: ""),
                minutes, distance, virages, arrivee)
        }
        return String(
            format: NSLocalizedString("%.0f min · %@ · arr. %@", comment: ""),
            minutes, distance, arrivee)
    }
}
