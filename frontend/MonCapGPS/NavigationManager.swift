import AVFoundation
import MapKit

/// Navigation turn-by-turn avec annonces vocales (français).
@MainActor
final class NavigationManager: ObservableObject {
    @Published var active = false
    @Published var instruction = "Calcul de l'itinéraire…"
    @Published var distanceToNext: Double = 0
    @Published var remainingKm: Double = 0
    @Published var etaMinutes: Double = 0
    @Published var routeCoords: [CLLocationCoordinate2D] = []

    private var steps: [MKRoute.Step] = []
    private var stepIndex = 0
    private var spokenApproach = false
    private var spokenImminent = false

    private let synth = AVSpeechSynthesizer()

    /// Démarre la navigation sur un itinéraire calculé.
    func start(route: MKRoute) {
        steps = route.steps
        routeCoords = route.polyline.coordinates
        remainingKm = route.distance / 1000
        etaMinutes = route.expectedTravelTime / 60
        stepIndex = firstMeaningfulStep()
        spokenApproach = false
        spokenImminent = false
        active = true
        configureAudio()
        if stepIndex < steps.count {
            instruction = stepText(steps[stepIndex])
            speak("Départ. " + instruction)
        }
    }

    func stop() {
        active = false
        synth.stopSpeaking(at: .immediate)
        steps = []
        routeCoords = []
        instruction = ""
    }

    /// Met à jour la progression à partir de la position courante.
    func update(_ coord: CLLocationCoordinate2D) {
        guard active, stepIndex < steps.count else { return }
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

    private func advance() {
        stepIndex += 1
        spokenApproach = false
        spokenImminent = false
        if stepIndex >= steps.count {
            speak("Vous êtes arrivé à destination.")
            active = false
        }
    }

    private func firstMeaningfulStep() -> Int {
        steps.firstIndex { !$0.instructions.isEmpty } ?? 0
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
