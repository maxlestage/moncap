import CoreLocation

/// Fournit la position GPS de l'appareil via CoreLocation.
final class LocationManager: NSObject, ObservableObject, CLLocationManagerDelegate {
    private let manager = CLLocationManager()

    /// Dernière coordonnée connue.
    @Published var coordinate: CLLocationCoordinate2D?
    /// Vitesse instantanée en km/h (0 si inconnue).
    @Published var speedKmh: Double = 0
    /// Cap de déplacement en degrés (0 = nord). Conserve la dernière valeur
    /// valide (CoreLocation renvoie -1 à l'arrêt).
    @Published var course: Double = 0

    override init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyBest
        // Filtre de distance : CoreLocation n'émet une mise à jour que tous les
        // ~10 m hors navigation (au lieu de plusieurs par seconde en continu).
        // Cela allège tout le pipeline déclenché à chaque point (réseau, trigo,
        // rendu carte) et économise nettement la batterie.
        manager.distanceFilter = 10
    }

    /// Demande l'autorisation et démarre le suivi.
    func start() {
        manager.requestWhenInUseAuthorization()
        manager.startUpdatingLocation()
    }

    /// Active/désactive le suivi en arrière-plan (pendant la navigation) :
    /// le GPS et le guidage continuent écran verrouillé, avec l'indicateur
    /// système visible.
    func setBackgroundTracking(_ on: Bool) {
        manager.allowsBackgroundLocationUpdates = on
        manager.pausesLocationUpdatesAutomatically = !on
        manager.showsBackgroundLocationIndicator = on
        // En navigation : filtre resserré (5 m) pour un guidage et une caméra 3D
        // fluides. Hors navigation : 10 m pour économiser la batterie.
        manager.distanceFilter = on ? 5 : 10
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard let loc = locations.last else { return }
        coordinate = loc.coordinate
        speedKmh = max(0, loc.speed) * 3.6
        if loc.course >= 0 { course = loc.course }
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        print("Erreur localisation: \(error.localizedDescription)")
    }
}
