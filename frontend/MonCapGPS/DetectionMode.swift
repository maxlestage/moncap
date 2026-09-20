import CoreMotion
import Foundation

/// Reconnaît tout seul comment on se déplace : à pied, à vélo, en voiture.
///
/// Le sélecteur de mode partait de la voiture et y restait. Quelqu'un qui
/// sort du parking à pied cherchait un itinéraire routier pour traverser une
/// place piétonne, et s'en apercevait au premier « prenez à droite » dans une
/// rue qu'il ne pouvait pas emprunter. Le téléphone sait pourtant : le
/// coprocesseur de mouvement classe l'activité en continu, sans réveiller le
/// GPS ni peser sur la batterie — c'est le même signal qui fait compter les
/// pas.
///
/// **Trois modes seulement.** Core Motion range le bus, le tram et la voiture
/// dans un même `automotive` : rien dans l'accéléromètre ne distingue un bus
/// d'une berline. Les transports restent donc un choix explicite, et la
/// détection ne les propose jamais — mieux vaut ne rien dire que de deviner
/// faux sur le seul mode où l'on ne conduit pas.
///
/// **Le choix de l'utilisateur gagne toujours.** Dès qu'on touche le
/// sélecteur, la détection se tait pour le reste du trajet : rien n'est plus
/// pénible qu'un réglage qui se rétablit tout seul. Elle reprend la main à la
/// fin du guidage, quand la question se repose.
@MainActor
final class DetectionMode: ObservableObject {
    static let partage = DetectionMode()

    /// Le mode reconnu, une fois qu'il s'est confirmé. Nil tant que rien
    /// n'est sûr — au démarrage, ou pendant une marche d'approche vers la
    /// voiture, où l'activité change trop vite pour être suivie.
    @Published private(set) var detecte: TravelMode?

    /// Vraie quand l'appareil sait classer l'activité. Faux sur un appareil
    /// sans coprocesseur de mouvement, ou si l'autorisation est refusée —
    /// l'écran cesse alors de proposer un réglage qui ne ferait rien.
    @Published private(set) var disponible = CMMotionActivityManager.isActivityAvailable()

    /// Nombre de relevés concordants avant de changer d'avis.
    ///
    /// Core Motion publie un relevé par changement d'activité, et il en
    /// publie de faux : un feu rouge donne `stationary`, un dos d'âne donne
    /// parfois `walking` en voiture. Sans ce seuil, le mode aurait clignoté
    /// et recalculé l'itinéraire à chaque cahot.
    private static let concordances = 3

    private let manager = CMMotionActivityManager()
    private var candidat: TravelMode?
    private var compte = 0
    private var enMarche = false

    private init() {}

    /// Commence à écouter. Sans effet si l'appareil ne sait pas classer, ou
    /// si l'écoute tourne déjà.
    func commence() {
        guard CMMotionActivityManager.isActivityAvailable(), !enMarche else { return }
        enMarche = true
        manager.startActivityUpdates(to: .main) { [weak self] activite in
            guard let self, let activite else { return }
            self.encaisse(activite)
        }
    }

    func arrete() {
        guard enMarche else { return }
        enMarche = false
        manager.stopActivityUpdates()
        candidat = nil
        compte = 0
    }

    /// Oublie ce qui a été reconnu — à la fin d'un guidage, quand la question
    /// se repose depuis le début.
    func oublie() {
        detecte = nil
        candidat = nil
        compte = 0
    }

    private func encaisse(_ activite: CMMotionActivity) {
        guard let mode = Self.mode(de: activite) else { return }
        if mode == candidat {
            compte += 1
        } else {
            candidat = mode
            compte = 1
        }
        if compte >= Self.concordances, detecte != mode {
            detecte = mode
        }
    }

    // MARK: - La lecture d'une activité

    /// Le mode que cette activité désigne, ou nil quand elle ne désigne rien.
    ///
    /// `nonisolated static` : c'est une fonction pure sur un relevé, et les
    /// tests doivent pouvoir l'appeler sans acteur.
    nonisolated static func mode(de a: CMMotionActivity) -> TravelMode? {
        // Une activité douteuse ne vaut pas mieux qu'aucune : Core Motion
        // marque lui-même sa confiance, et changer de mode sur un « peut-être »
        // recalculerait l'itinéraire pour rien.
        guard a.confidence != .low else { return nil }
        // L'ordre compte. Core Motion peut lever deux drapeaux à la fois —
        // `automotive` et `stationary` à un feu rouge, `walking` et `cycling`
        // au démarrage d'un coup de pédale. Le véhicule l'emporte, puis le
        // vélo : c'est du plus contraignant au moins contraignant pour le
        // calcul d'itinéraire, et se tromper vers le plus contraignant fait
        // faire un détour, pas emprunter une voie interdite.
        if a.automotive { return .car }
        if a.cycling { return .bike }
        if a.walking || a.running { return .walk }
        // `stationary` et `unknown` ne disent rien du mode : à l'arrêt on est
        // toujours dans le mode d'avant, et l'oublier ferait repartir la
        // voiture en piéton à chaque feu.
        return nil
    }
}
