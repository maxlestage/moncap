import Foundation
import WatchConnectivity
import WatchKit

/// L'état du guidage tel que l'iPhone le pousse. Les clés du dictionnaire
/// (actif, instruction, distanceM, etaMinutes, restantKm, symbole) sont le
/// contrat avec MontreLien.swift côté iPhone — même vocabulaire que la
/// Live Activity.
struct EtatGuidage: Equatable {
    var actif = false
    var instruction = ""
    var distanceM: Double = 0
    var etaMinutes: Double = 0
    var restantKm: Double = 0
    var symbole = "location.fill"
}

/// Reçoit l'état de guidage envoyé par l'iPhone.
///
/// Le canal est `updateApplicationContext` : le dernier état poussé survit à
/// la fermeture de l'app montre et se lit dès l'activation — pas de course
/// entre le lancement du guidage côté téléphone et l'ouverture au poignet.
final class LienTelephone: NSObject, WCSessionDelegate, ObservableObject {
    static let partage = LienTelephone()

    @Published var etat = EtatGuidage()

    private override init() {
        super.init()
        guard WCSession.isSupported() else { return }
        WCSession.default.delegate = self
        WCSession.default.activate()
    }

    func session(_ session: WCSession,
                 activationDidCompleteWith activationState: WCSessionActivationState,
                 error: Error?) {
        let contexte = session.receivedApplicationContext
        if !contexte.isEmpty { applique(contexte) }
    }

    func session(_ session: WCSession,
                 didReceiveApplicationContext applicationContext: [String: Any]) {
        applique(applicationContext)
    }

    /// Envoie un ordre au téléphone, et le dit au poignet.
    ///
    /// `sendMessage` exige que l'iPhone soit joignable — c'est le cas pendant
    /// un guidage, l'app y tourne au premier plan ou en arrière-plan actif.
    /// Injoignable, on ne fait pas semblant : une vibration d'échec, et
    /// l'écran reste tel qu'il est. Rien n'est plus trompeur qu'un bouton qui
    /// s'anime sans que rien ne se passe à l'autre bout.
    func commande(_ nom: String) {
        let session = WCSession.default
        guard session.activationState == .activated, session.isReachable else {
            WKInterfaceDevice.current().play(.failure)
            return
        }
        session.sendMessage(
            ["cmd": nom],
            replyHandler: { reponse in
                let fait = reponse["fait"] as? Bool ?? false
                DispatchQueue.main.async {
                    WKInterfaceDevice.current().play(fait ? .success : .failure)
                }
            },
            errorHandler: { _ in
                DispatchQueue.main.async {
                    WKInterfaceDevice.current().play(.failure)
                }
            })
    }

    private func applique(_ d: [String: Any]) {
        DispatchQueue.main.async {
            var nouveau = EtatGuidage()
            nouveau.actif = d["actif"] as? Bool ?? false
            nouveau.instruction = d["instruction"] as? String ?? ""
            nouveau.distanceM = d["distanceM"] as? Double ?? 0
            nouveau.etaMinutes = d["etaMinutes"] as? Double ?? 0
            nouveau.restantKm = d["restantKm"] as? Double ?? 0
            nouveau.symbole = d["symbole"] as? String ?? "location.fill"
            // Un tapotement au poignet à chaque nouvelle consigne : le
            // conducteur sent la manœuvre venir sans regarder l'écran.
            if nouveau.actif, !nouveau.instruction.isEmpty,
               nouveau.instruction != self.etat.instruction {
                WKInterfaceDevice.current().play(.directionUp)
            }
            self.etat = nouveau
        }
    }
}
