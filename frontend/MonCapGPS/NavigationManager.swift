import AVFoundation
import MapKit

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

    private var steps: [MKRoute.Step] = []
    private var stepIndex = 0
    private var spokenApproach = false
    private var spokenImminent = false
    private var offRouteHits = 0
    private var lastReroute = Date.distantPast

    private let synth = AVSpeechSynthesizer()

    /// Démarre la navigation vers une destination.
    func start(route: MKRoute, destination: CLLocationCoordinate2D) {
        self.destination = destination
        configureAudio()
        load(route, announceStart: true)
        active = true
    }

    /// Applique un itinéraire recalculé (sans annonce « Départ »).
    func applyReroute(route: MKRoute) {
        load(route, announceStart: false)
        rerouting = false
        speak("Nouvel itinéraire.")
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

        let maneuver = maneuverCoordinate(steps[stepIndex])
        let d = distance(coord, maneuver)
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

    private func load(_ route: MKRoute, announceStart: Bool) {
        steps = route.steps
        routeCoords = route.polyline.coordinates
        remainingKm = route.distance / 1000
        etaMinutes = route.expectedTravelTime / 60
        stepIndex = steps.firstIndex { !$0.instructions.isEmpty } ?? 0
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

    private func stepText(_ step: MKRoute.Step) -> String {
        step.instructions.isEmpty ? "Continuez tout droit" : step.instructions
    }

    private func maneuverCoordinate(_ step: MKRoute.Step) -> CLLocationCoordinate2D {
        step.polyline.coordinates.last ?? step.polyline.coordinate
    }

    private func distance(_ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D) -> Double {
        CLLocation(latitude: a.latitude, longitude: a.longitude)
            .distance(from: CLLocation(latitude: b.latitude, longitude: b.longitude))
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
