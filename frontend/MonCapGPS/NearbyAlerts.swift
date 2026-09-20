import CoreLocation
import UserNotifications

/// Prévient d'une alerte communautaire proche : **à la voix** quand un
/// guidage tourne, par notification locale sinon — y compris écran verrouillé
/// (le suivi de position continue en arrière-plan).
///
/// La voix n'est pas un raffinement : au volant, le téléphone est sur son
/// support et les yeux sont sur la route. Une bannière y est invisible, et un
/// son seul ne dit pas ce qui arrive. C'est exactement l'usage où l'alerte
/// vaut quelque chose.
///
/// Best effort : silencieux si l'autorisation de notification est refusée
/// (iOS ignore alors simplement l'envoi). Chaque alerte n'est notifiée qu'une
/// fois ; une alerte disparue puis réapparue pourra de nouveau l'être.
final class NearbyAlertNotifier: NSObject, ObservableObject, UNUserNotificationCenterDelegate {
    /// Rayon de déclenchement hors guidage, ou à l'arrêt (mètres).
    private let radius: Double = 600

    /// Fenêtre d'ouverture devant soi, en degrés de part et d'autre du cap.
    ///
    /// Un rayon seul prévient de ce qui est **derrière**, et de l'autre côté
    /// du terre-plein : sur une deux-fois-deux-voies, la moitié des alertes à
    /// six cents mètres ne concernent pas celui qui roule. Soixante degrés
    /// laissent passer une alerte en courbe sans ramener celles qu'on vient
    /// de dépasser.
    private let ouvertureDeg: Double = 60

    /// Temps d'anticipation visé (secondes) : c'est lui qui fixe la distance
    /// d'annonce, pas un rayon fixe. Six cents mètres valent vingt-quatre
    /// secondes à 90 km/h et plus d'une minute en ville — trop tôt pour être
    /// utile, on a le temps d'oublier.
    private let anticipationS: Double = 20
    /// Bornes de cette distance : de quoi rester utile à l'arrêt comme à
    /// vitesse d'autoroute.
    private let minPrevenirM: Double = 200
    private let maxPrevenirM: Double = 900
    /// Alertes déjà notifiées (anti-répétition).
    private var notified: Set<Int> = []
    /// Demande d'autorisation faite une seule fois.
    private var askedAuthorization = false

    override init() {
        super.init()
        // Permet d'afficher la bannière même quand l'app est au premier plan.
        UNUserNotificationCenter.current().delegate = self
    }

    /// Demande l'autorisation de notifier (une seule fois), à l'activation de
    /// l'option par l'utilisateur.
    func requestAuthorizationIfNeeded() {
        guard !askedAuthorization else { return }
        askedAuthorization = true
        UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    /// À appeler quand la position ou la liste d'alertes change : notifie les
    /// alertes proches non encore signalées.
    /// - Parameters:
    ///   - cap: le cap suivi, en degrés. Ignoré à l'arrêt, où l'on ne va nulle
    ///     part et où tout ce qui est autour compte.
    ///   - vitesseKmh: sert à fixer la distance d'annonce par le temps qu'elle
    ///     laisse, et non par une longueur arbitraire.
    ///   - annonce: la voix du guidage, quand il tourne. Nil sinon, et la
    ///     notification reprend son rôle.
    func check(
        from c: CLLocationCoordinate2D,
        alerts: [Alert],
        cap: Double = -1,
        vitesseKmh: Double = 0,
        annonce: ((String) -> Void)? = nil,
        emoji: (String) -> String
    ) {
        // Oublie les alertes notifiées qui ne sont plus actives (expirées /
        // supprimées) pour qu'une réapparition puisse de nouveau notifier.
        notified.formIntersection(Set(alerts.map(\.id)))

        let roule = vitesseKmh >= 10 && cap >= 0
        let portee =
            roule
            ? min(maxPrevenirM, max(minPrevenirM, vitesseKmh / 3.6 * anticipationS))
            : radius

        let here = CLLocation(latitude: c.latitude, longitude: c.longitude)
        for a in alerts where !notified.contains(a.id) {
            let d = here.distance(from: CLLocation(latitude: a.lat, longitude: a.lon))
            guard d <= portee else { continue }
            // À l'arrêt on prévient de tout ce qui est autour : on ne va nulle
            // part, et l'alerte sert alors à décider où aller.
            if roule, Self.ecartDeCap(de: c, vers: a, cap: cap) > ouvertureDeg {
                continue
            }
            notified.insert(a.id)
            if let annonce {
                annonce(Self.phrase(alert: a, distance: d))
            } else {
                post(alert: a, distance: d, emoji: emoji)
            }
        }
    }

    /// L'écart, en degrés, entre le cap suivi et la direction de l'alerte.
    ///
    /// Rendu toujours entre 0 et 180 : sans ce repliement, une alerte plein
    /// nord vue depuis un cap de 350° donnerait un écart de 350 degrés au lieu
    /// de dix, et serait écartée alors qu'elle est droit devant.
    static func ecartDeCap(
        de position: CLLocationCoordinate2D, vers a: Alert, cap: Double
    ) -> Double {
        let rad = Double.pi / 180
        let dLon = (a.lon - position.longitude) * rad
        let lat1 = position.latitude * rad
        let lat2 = a.lat * rad
        let y = sin(dLon) * cos(lat2)
        let x = cos(lat1) * sin(lat2) - sin(lat1) * cos(lat2) * cos(dLon)
        let azimut = atan2(y, x) / rad
        let ecart = abs((azimut - cap).truncatingRemainder(dividingBy: 360))
        return ecart > 180 ? 360 - ecart : ecart
    }

    /// « Ralentisseur dans 300 mètres. » — la phrase que la voix prononce.
    ///
    /// Des mètres arrondis à la cinquantaine : la précision au mètre serait
    /// fausse (le signalement lui-même est posé à la louche) et illisible à
    /// l'oreille.
    static func phrase(alert a: Alert, distance: Double) -> String {
        let quoi = NSLocalizedString(a.label.isEmpty ? a.category : a.label, comment: "")
        let metres = max(50, Int((distance / 50).rounded()) * 50)
        return String(
            format: NSLocalizedString("%@ dans %lld mètres.", comment: ""), quoi, metres)
    }

    /// Réinitialise l'anti-répétition (ex. à la déconnexion).
    func reset() { notified.removeAll() }

    private func post(alert a: Alert, distance: Double, emoji: (String) -> String) {
        // Comme ailleurs : le fil porte le français, l'affichage traduit.
        let what = NSLocalizedString(a.label.isEmpty ? a.category : a.label, comment: "")
        let meters = max(50, Int((distance / 50).rounded()) * 50)
        let content = UNMutableNotificationContent()
        content.title = "\(emoji(a.category)) Alerte à proximité"
        content.body = "\(what) à \(meters) m."
        content.sound = .default
        // Déclenchement immédiat (trigger nil).
        let request = UNNotificationRequest(
            identifier: "moncap.alert.\(a.id)", content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }

    // MARK: - UNUserNotificationCenterDelegate

    /// Affiche la notification même lorsque l'app est au premier plan.
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }
}
