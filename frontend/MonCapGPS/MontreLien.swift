import Foundation
import WatchConnectivity

/// Pousse l'état du guidage vers l'Apple Watch.
///
/// Même contrat que la Live Activity (GuidageAttributs.ContentState), même
/// throttle : la montre n'a pas besoin de plus de fraîcheur que l'écran
/// verrouillé. Le canal `updateApplicationContext` remplace l'état précédent
/// au lieu d'empiler des messages — parfait pour du « dernier état connu ».
@MainActor
final class MontreLien: NSObject, WCSessionDelegate, ObservableObject {
    static let partage = MontreLien()

    /// Vraie quand une montre est appairée SANS l'app MonCap : l'app
    /// propose alors de l'installer — TestFlight ne le fait pas toujours
    /// tout seul, et personne ne va fouiller l'app Watch sans y être invité.
    @Published private(set) var montreSansApp = false

    private var dernierEtat: GuidageAttributs.ContentState?
    private var derniereMaj = Date.distantPast

    private override init() {
        super.init()
        guard WCSession.isSupported() else { return }
        WCSession.default.delegate = self
        WCSession.default.activate()
    }

    /// Relit l'état d'appairage. Les propriétés de WCSession se lisent de
    /// n'importe quel fil ; seule la publication repasse par le main actor.
    private nonisolated func relitMontre(_ session: WCSession) {
        let sansApp = session.activationState == .activated && session.isPaired
            && !session.isWatchAppInstalled
        Task { @MainActor in
            if self.montreSansApp != sansApp { self.montreSansApp = sansApp }
        }
    }

    /// Envoie l'état courant si une montre appairée a l'app installée.
    func maj(_ etat: GuidageAttributs.ContentState) {
        guard WCSession.isSupported() else { return }
        let session = WCSession.default
        guard session.activationState == .activated, session.isPaired,
              session.isWatchAppInstalled else { return }

        // Même filtre que GuidageActivite.maj : on ne pousse que si la
        // consigne change, ou tous les 50 m / 1 min.
        if let avant = dernierEtat,
           avant.instruction == etat.instruction,
           avant.symbole == etat.symbole,
           abs(avant.distanceM - etat.distanceM) < 50,
           Date().timeIntervalSince(derniereMaj) < 60 {
            return
        }
        dernierEtat = etat
        derniereMaj = Date()
        try? session.updateApplicationContext([
            "actif": true,
            "instruction": etat.instruction,
            "distanceM": etat.distanceM,
            "etaMinutes": etat.etaMinutes,
            "restantKm": etat.restantKm,
            "symbole": etat.symbole,
        ])
    }

    /// Fin de guidage : la montre repasse à l'écran d'attente.
    func termine() {
        dernierEtat = nil
        guard WCSession.isSupported() else { return }
        let session = WCSession.default
        guard session.activationState == .activated, session.isPaired,
              session.isWatchAppInstalled else { return }
        try? session.updateApplicationContext(["actif": false])
    }

    // MARK: - Les commandes venues du poignet

    /// Exécute une commande envoyée par la montre.
    ///
    /// Le canal est `sendMessage` et non `updateApplicationContext` : un
    /// ordre n'est pas un état. Il doit partir maintenant ou échouer
    /// franchement — « arrête-toi » livré trois minutes plus tard, après
    /// l'arrivée, serait pire que pas livré du tout.
    ///
    /// L'instance de guidage est atteinte par le même sujet que CarPlay :
    /// elle vit dans la scène du téléphone, et ce sujet est le pont.
    ///
    /// Rend faux quand la commande n'a pas pu s'appliquer — commande
    /// inconnue, ou aucun guidage en cours. La montre en fait un retour
    /// haptique d'échec plutôt que de laisser croire que l'ordre est passé.
    @MainActor
    static func execute(_ commande: String) -> Bool {
        guard let nav = NavigationManager.instanceCourante.value, nav.active else {
            return false
        }
        switch commande {
        case "arrete":
            nav.stop()
            return true
        case "redit":
            nav.redit()
            return true
        default:
            return false
        }
    }

    // WCSessionDelegate — rien à faire de spécial côté iPhone, sinon
    // réactiver la session quand elle bascule d'une montre à l'autre, et
    // suivre l'appairage pour la proposition d'installation.

    /// La montre attend une réponse : elle s'en sert pour vibrer juste. Le
    /// `replyHandler` doit donc être appelé dans tous les cas, y compris
    /// quand la commande échoue — sans quoi la montre attend puis rend une
    /// erreur de délai, qui ne dit pas la même chose qu'un refus.
    nonisolated func session(
        _ session: WCSession,
        didReceiveMessage message: [String: Any],
        replyHandler: @escaping ([String: Any]) -> Void
    ) {
        let commande = message["cmd"] as? String ?? ""
        Task { @MainActor in
            replyHandler(["fait": MontreLien.execute(commande)])
        }
    }

    /// Même chose pour un envoi sans réponse attendue.
    nonisolated func session(_ session: WCSession, didReceiveMessage message: [String: Any]) {
        let commande = message["cmd"] as? String ?? ""
        Task { @MainActor in _ = MontreLien.execute(commande) }
    }
    nonisolated func session(_ session: WCSession,
                             activationDidCompleteWith activationState: WCSessionActivationState,
                             error: Error?) {
        relitMontre(session)
    }

    nonisolated func sessionWatchStateDidChange(_ session: WCSession) {
        relitMontre(session)
    }

    nonisolated func sessionDidBecomeInactive(_ session: WCSession) {}

    nonisolated func sessionDidDeactivate(_ session: WCSession) {
        session.activate()
    }
}
