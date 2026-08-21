import Foundation
import MetricKit

/// Les rapports de plantage, sans service tiers : MetricKit remet au
/// lancement suivant les diagnostics que le système a relevés — pile
/// d'appels comprise — et on les poste au serveur, qui les journalise.
///
/// Rien d'autre ne part : pas d'identifiant, pas de position, pas de
/// métriques d'usage. Un plantage muet est le seul bogue qu'on ne corrige
/// jamais ; celui-ci laisse une trace dans `heroku logs`.
final class Diagnostics: NSObject, MXMetricManagerSubscriber {
    static let shared = Diagnostics()

    func demarre() {
        MXMetricManager.shared.add(self)
    }

    func didReceive(_ payloads: [MXDiagnosticPayload]) {
        for p in payloads {
            envoie(p.jsonRepresentation())
        }
    }

    private func envoie(_ donnees: Data) {
        // Deux cents kilooctets suffisent largement à une pile : au-delà,
        // c'est du lest qu'on ne fait pas voyager sur le forfait de qui
        // vient déjà de subir un plantage.
        guard donnees.count < 200_000,
            let url = URL(
                string: "https://moncap-c41a5aaf07e8.herokuapp.com/diagnostics")
        else { return }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = donnees
        // Best effort : si l'envoi échoue, MetricKit relivrera plus tard.
        URLSession.shared.dataTask(with: req).resume()
    }
}
