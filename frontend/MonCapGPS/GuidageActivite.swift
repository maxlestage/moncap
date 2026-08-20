import ActivityKit
import Foundation

/// La Live Activity du guidage : la consigne, sa distance et l'heure
/// d'arrivée sur l'écran verrouillé et dans la Dynamic Island, sans
/// déverrouiller le téléphone.
///
/// Cette structure existe en double : ici, et dans l'extension `Guidage`
/// qui dessine l'activité. ActivityKit apparie les deux processus par le
/// **nom** du type et ses clés JSON — les deux copies doivent rester
/// identiques mot pour mot, c'est le contrat.
struct GuidageAttributs: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        var instruction: String
        var distanceM: Double
        var etaMinutes: Double
        var restantKm: Double
        /// Nom du symbole SF de la manœuvre (la même flèche que le bandeau).
        var symbole: String
    }
}

/// Le cycle de vie de l'activité, aligné sur celui du guidage : demandée au
/// départ, mise à jour au fil des consignes, terminée à l'arrêt.
@MainActor
enum GuidageActivite {
    private static var activite: Activity<GuidageAttributs>?
    private static var dernierEnvoi: GuidageAttributs.ContentState?

    /// Demande l'activité. Refus silencieux si l'utilisateur les a coupées
    /// dans Réglages — le guidage n'en dépend en rien.
    static func demarre(_ etat: GuidageAttributs.ContentState) {
        termine()
        guard ActivityAuthorizationInfo().areActivitiesEnabled else { return }
        dernierEnvoi = etat
        activite = try? Activity.request(
            attributes: GuidageAttributs(),
            content: .init(state: etat, staleDate: nil))
    }

    /// Pousse un nouvel état — mais seulement ce qui se voit : une consigne
    /// qui change, cinquante mètres avalés, une minute d'écart. ActivityKit
    /// n'aime pas la mitraille, et la position tombe chaque seconde.
    static func maj(_ etat: GuidageAttributs.ContentState) {
        guard let activite else { return }
        if let d = dernierEnvoi,
            d.instruction == etat.instruction,
            d.symbole == etat.symbole,
            abs(d.distanceM - etat.distanceM) < 50,
            abs(d.etaMinutes - etat.etaMinutes) < 1
        {
            return
        }
        dernierEnvoi = etat
        Task { await activite.update(ActivityContent(state: etat, staleDate: nil)) }
    }

    static func termine() {
        guard let ancienne = activite else { return }
        activite = nil
        dernierEnvoi = nil
        Task { await ancienne.end(nil, dismissalPolicy: .immediate) }
    }
}
