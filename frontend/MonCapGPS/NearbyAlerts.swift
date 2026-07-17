import CoreLocation
import UserNotifications

/// Notifie l'utilisateur (notification locale : bannière + son) lorsqu'une
/// alerte communautaire se trouve à proximité — y compris hors navigation et
/// écran verrouillé (le suivi de position continue en arrière-plan).
///
/// Best effort : silencieux si l'autorisation de notification est refusée
/// (iOS ignore alors simplement l'envoi). Chaque alerte n'est notifiée qu'une
/// fois ; une alerte disparue puis réapparue pourra de nouveau l'être.
final class NearbyAlertNotifier: NSObject, ObservableObject, UNUserNotificationCenterDelegate {
    /// Rayon de déclenchement (mètres).
    private let radius: Double = 600
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
    func check(
        from c: CLLocationCoordinate2D,
        alerts: [Alert],
        emoji: (String) -> String
    ) {
        // Oublie les alertes notifiées qui ne sont plus actives (expirées /
        // supprimées) pour qu'une réapparition puisse de nouveau notifier.
        notified.formIntersection(Set(alerts.map(\.id)))

        let here = CLLocation(latitude: c.latitude, longitude: c.longitude)
        for a in alerts where !notified.contains(a.id) {
            let d = here.distance(from: CLLocation(latitude: a.lat, longitude: a.lon))
            guard d <= radius else { continue }
            notified.insert(a.id)
            post(alert: a, distance: d, emoji: emoji)
        }
    }

    /// Réinitialise l'anti-répétition (ex. à la déconnexion).
    func reset() { notified.removeAll() }

    private func post(alert a: Alert, distance: Double, emoji: (String) -> String) {
        let what = a.label.isEmpty ? a.category : a.label
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
