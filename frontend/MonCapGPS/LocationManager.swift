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
    }

    /// Demande l'autorisation et démarre le suivi.
    func start() {
        manager.requestWhenInUseAuthorization()
        manager.startUpdatingLocation()
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
