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

    // WCSessionDelegate — rien à faire de spécial côté iPhone, sinon
    // réactiver la session quand elle bascule d'une montre à l'autre, et
    // suivre l'appairage pour la proposition d'installation.
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
