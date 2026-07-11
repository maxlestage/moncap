import AVFoundation
import MapKit

/// Une étape de navigation : texte d'instruction + point de la manœuvre.
/// Permet d'assembler des itinéraires composés de plusieurs tronçons.
struct NavStep {
    let text: String
    let coord: CLLocationCoordinate2D
}

/// Navigation turn-by-turn avec annonces vocales et recalcul automatique
/// quand on sort de l'itinéraire.
@MainActor
final class NavigationManager: ObservableObject {
    @Published var active = false
    @Published var rerouting = false
    @Published var instruction = "Calcul de l'itinéraire…"
    @Published var distanceToNext: Double = 0
    @Published var remainingKm: Double = 0
    @Published var etaMinutes: Double = 0
    @Published var routeCoords: [CLLocationCoordinate2D] = []

    /// Destination courante (pour le recalcul).
    private(set) var destination: CLLocationCoordinate2D?
    /// Déclenché quand un recalcul est nécessaire (l'UI relance MKDirections).
    var onReroute: (() -> Void)?

    private var steps: [NavStep] = []
    private var stepIndex = 0
    /// Distance et durée totales de l'itinéraire (pour estimer le restant).
    private var totalKm = 0.0
    private var totalMin = 0.0
    private var spokenApproach = false
    private var spokenImminent = false
    private var offRouteHits = 0
    private var lastReroute = Date.distantPast

    private let synth = AVSpeechSynthesizer()

    /// Démarre la navigation vers une destination (à partir d'un MKRoute).
    func start(route: MKRoute, destination: CLLocationCoordinate2D) {
        start(steps: Self.navSteps(from: route),
              coordinates: route.polyline.coordinates,
              distanceKm: route.distance / 1000,
              etaMinutes: route.expectedTravelTime / 60,
              destination: destination)
    }

    /// Démarre la navigation à partir d'étapes et d'un tracé déjà assemblés
    /// (utile pour un itinéraire composé de plusieurs tronçons).
    func start(steps: [NavStep], coordinates: [CLLocationCoordinate2D],
               distanceKm: Double, etaMinutes: Double,
               destination: CLLocationCoordinate2D) {
        self.destination = destination
        configureAudio()
        load(steps: steps, coords: coordinates, km: distanceKm, min: etaMinutes,
             announceStart: true)
        active = true
    }

    /// Applique un itinéraire recalculé (sans annonce « Départ »).
    func applyReroute(route: MKRoute) {
        load(steps: Self.navSteps(from: route), coords: route.polyline.coordinates,
             km: route.distance / 1000, min: route.expectedTravelTime / 60,
             announceStart: false)
        rerouting = false
        speak("Nouvel itinéraire.")
    }

    /// Convertit les étapes d'un MKRoute en NavStep.
    private static func navSteps(from route: MKRoute) -> [NavStep] {
        route.steps.map {
            NavStep(text: $0.instructions,
                    coord: $0.polyline.coordinates.last ?? $0.polyline.coordinate)
        }
    }

    /// Annonce vocale ponctuelle (ex. signalement à proximité).
    func announce(_ text: String) {
        speak(text)
    }

    func stop() {
        active = false
        rerouting = false
        synth.stopSpeaking(at: .immediate)
        steps = []
        routeCoords = []
        instruction = ""
        destination = nil
    }

    /// Met à jour la progression à partir de la position courante.
    func update(_ coord: CLLocationCoordinate2D) {
        guard active else { return }

        // Met à jour en temps réel la distance et la durée restantes.
        updateRemaining(coord)

        // Détection de sortie d'itinéraire (anti-rebond : 3 relevés de suite).
        if !rerouting {
            let deviation = distanceToRoute(coord)
            if deviation > 55 {
                offRouteHits += 1
                if offRouteHits >= 3, Date().timeIntervalSince(lastReroute) > 5 {
                    triggerReroute()
                    return
                }
            } else {
                offRouteHits = 0
            }
        }

        guard !rerouting, stepIndex < steps.count else { return }

        let d = distance(coord, steps[stepIndex].coord)
        distanceToNext = d
        instruction = stepText(steps[stepIndex])

        if d < 250, !spokenApproach {
            speak("Dans \(roundedMeters(d)) mètres, \(instruction)")
            spokenApproach = true
        }
        if d < 60, !spokenImminent {
            speak(instruction)
            spokenImminent = true
        }
        if d < 25 {
            advance()
        }
    }

    // MARK: - Privé

    private func triggerReroute() {
        offRouteHits = 0
        lastReroute = Date()
        rerouting = true
        instruction = "Recalcul de l'itinéraire…"
        speak("Recalcul de l'itinéraire.")
        onReroute?()
    }

    private func load(steps: [NavStep], coords: [CLLocationCoordinate2D],
                      km: Double, min: Double, announceStart: Bool) {
        self.steps = steps
        routeCoords = coords
        totalKm = km
        totalMin = min
        remainingKm = km
        etaMinutes = min
        stepIndex = steps.firstIndex { !$0.text.isEmpty } ?? 0
        spokenApproach = false
        spokenImminent = false
        if stepIndex < steps.count {
            instruction = stepText(steps[stepIndex])
            if announceStart { speak("Départ. " + instruction) }
        }
    }

    private func advance() {
        stepIndex += 1
        spokenApproach = false
        spokenImminent = false
        if stepIndex >= steps.count {
            speak("Vous êtes arrivé à destination.")
            active = false
        }
    }

    private func stepText(_ step: NavStep) -> String {
        step.text.isEmpty ? "Continuez tout droit" : step.text
    }

    private func distance(_ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D) -> Double {
        CLLocation(latitude: a.latitude, longitude: a.longitude)
            .distance(from: CLLocation(latitude: b.latitude, longitude: b.longitude))
    }

    /// Recalcule la distance et la durée restantes depuis la position courante.
    private func updateRemaining(_ coord: CLLocationCoordinate2D) {
        guard routeCoords.count >= 2 else { return }
        remainingKm = remainingDistance(from: coord) / 1000
        // Vitesse moyenne de l'itinéraire pour une ETA cohérente.
        let avgKmh = totalMin > 0 ? totalKm / (totalMin / 60) : 40
        etaMinutes = avgKmh > 0 ? remainingKm / avgKmh * 60 : 0
    }

    /// Distance (m) restante le long de l'itinéraire depuis un point (projeté
    /// sur le segment le plus proche, puis somme jusqu'à l'arrivée).
    private func remainingDistance(from p: CLLocationCoordinate2D) -> Double {
        let coords = routeCoords
        guard coords.count >= 2 else { return 0 }
        let mLat = 111_320.0
        let mLon = 111_320.0 * cos(p.latitude * .pi / 180)
        func xy(_ c: CLLocationCoordinate2D) -> (Double, Double) {
            ((c.longitude - p.longitude) * mLon, (c.latitude - p.latitude) * mLat)
        }
        var bestI = 0, bestT = 0.0, best = Double.greatestFiniteMagnitude
        for i in 0..<(coords.count - 1) {
            let (ax, ay) = xy(coords[i])
            let (bx, by) = xy(coords[i + 1])
            let dx = bx - ax, dy = by - ay
            let len2 = dx * dx + dy * dy
            let t = len2 == 0 ? 0 : max(0, min(1, -(ax * dx + ay * dy) / len2))
            let cx = ax + t * dx, cy = ay + t * dy
            let d = (cx * cx + cy * cy).squareRoot()
            if d < best { best = d; bestI = i; bestT = t }
        }
        // De la projection jusqu'à la fin du segment courant, puis les suivants.
        let a = coords[bestI], b = coords[bestI + 1]
        let proj = CLLocationCoordinate2D(
            latitude: a.latitude + (b.latitude - a.latitude) * bestT,
            longitude: a.longitude + (b.longitude - a.longitude) * bestT)
        var rem = distance(proj, b)
        var i = bestI + 1
        while i < coords.count - 1 {
            rem += distance(coords[i], coords[i + 1])
            i += 1
        }
        return rem
    }

    /// Distance minimale (m) entre un point et la polyligne de l'itinéraire.
    private func distanceToRoute(_ p: CLLocationCoordinate2D) -> Double {
        guard routeCoords.count >= 2 else { return 0 }
        let mPerDegLat = 111_320.0
        let mPerDegLon = 111_320.0 * cos(p.latitude * .pi / 180)
        func xy(_ c: CLLocationCoordinate2D) -> (Double, Double) {
            ((c.longitude - p.longitude) * mPerDegLon, (c.latitude - p.latitude) * mPerDegLat)
        }
        var best = Double.greatestFiniteMagnitude
        for i in 0..<(routeCoords.count - 1) {
            let (ax, ay) = xy(routeCoords[i])
            let (bx, by) = xy(routeCoords[i + 1])
            let dx = bx - ax, dy = by - ay
            let len2 = dx * dx + dy * dy
            let t = len2 == 0 ? 0 : max(0, min(1, -(ax * dx + ay * dy) / len2))
            let cx = ax + t * dx, cy = ay + t * dy
            best = min(best, (cx * cx + cy * cy).squareRoot())
        }
        return best
    }

    private func roundedMeters(_ d: Double) -> Int {
        d > 100 ? Int((d / 50).rounded()) * 50 : Int((d / 10).rounded()) * 10
    }

    private func configureAudio() {
        try? AVAudioSession.sharedInstance()
            .setCategory(.playback, mode: .voicePrompt, options: [.duckOthers, .mixWithOthers])
        try? AVAudioSession.sharedInstance().setActive(true)
    }

    private func speak(_ text: String) {
        let u = AVSpeechUtterance(string: text)
        u.voice = AVSpeechSynthesisVoice(language: "fr-FR")
        synth.speak(u)
    }
}
