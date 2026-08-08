import AVFoundation
import MapKit

/// Une étape de navigation : texte d'instruction + point de la manœuvre.
/// Permet d'assembler des itinéraires composés de plusieurs tronçons.
struct NavStep {
    let text: String
    let coord: CLLocationCoordinate2D
}

/// Manœuvre à effectuer, déduite de l'instruction fournie par le moteur
/// d'itinéraire. Elle porte la flèche affichée : celle-ci découle donc de la
/// même analyse que la consigne et ne peut pas la contredire (l'icône du
/// bandeau était auparavant figée sur « à droite », quelle que soit la
/// manœuvre).
enum Maneuver {
    case left, slightLeft, sharpLeft
    case right, slightRight, sharpRight
    case roundaboutLeft, roundaboutRight
    case uTurn, straight, arrive

    /// Flèche du bandeau (symboles SF garantis présents sur iOS 17+).
    var icon: String {
        switch self {
        case .left: return "arrow.turn.up.left"
        case .right: return "arrow.turn.up.right"
        case .slightLeft: return "arrow.up.left"
        case .slightRight: return "arrow.up.right"
        case .sharpLeft: return "arrow.uturn.left"
        case .sharpRight: return "arrow.uturn.right"
        case .roundaboutLeft: return "arrow.turn.up.left"
        case .roundaboutRight: return "arrow.turn.up.right"
        case .uTurn: return "arrow.uturn.down"
        case .straight: return "arrow.up"
        case .arrive: return "mappin.and.ellipse"
        }
    }
}

/// Consigne de navigation prête à afficher et à annoncer.
///
/// L'affichage tient en un mot (« Droite »), lisible d'un coup d'œil au
/// volant, et le **détail utile** — numéro de sortie d'un rond-point, virage
/// serré — vient sur une seconde ligne plutôt que d'alourdir la consigne.
/// Aucun nom de rue : seul le côté à prendre compte.
struct NavInstruction {
    let maneuver: Maneuver
    /// Consigne principale, en un mot.
    let display: String
    /// Précision secondaire (« 2e sortie », « serrez »), nil si inutile.
    let detail: String?
    /// Formulation orale complète, naturelle à l'écoute.
    let spoken: String

    static let waiting = NavInstruction(
        maneuver: .straight, display: "Calcul de l'itinéraire…", detail: nil,
        spoken: "calcul de l'itinéraire")
}

/// Analyse d'une instruction de moteur d'itinéraire.
///
/// Déclarée en extension à dessein : un initialiseur écrit dans le corps
/// d'une struct supprime l'initialiseur memberwise, dont dépendent ici
/// toutes les branches (`self.init(maneuver:display:detail:spoken:)`).
extension NavInstruction {
    /// Analyse l'instruction du moteur (Apple Plans en français ou en anglais
    /// selon la langue de l'appareil, BRouter pour le vélo).
    init(from instruction: String) {
        var s =
            instruction
            .folding(options: .diacriticInsensitive, locale: Locale(identifier: "fr"))
            .lowercased()
        // Le numéro de sortie se lit AVANT de couper le nom de la voie.
        let exit = Self.roundaboutExit(in: s)
        // Coupe le nom de la voie (« … sur Rue de la Rive Gauche ») avant
        // d'analyser le côté : sinon une rue dont le nom contient « gauche »
        // ou « droite » inverserait la manœuvre.
        for separator in [" sur ", " onto ", " vers ", " toward"] {
            if let r = s.range(of: separator) {
                s = String(s[s.startIndex..<r.lowerBound])
                break
            }
        }
        let left = s.contains("gauche") || s.contains("left")
        let right = s.contains("droite") || s.contains("right")
        let slight = s.contains("legerement") || s.contains("slight") || s.contains("serrez")
            || s.contains("keep")
        let sharp = s.contains("fortement") || s.contains("sharp")

        if s.contains("arriv") || s.contains("destination") {
            self.init(maneuver: .arrive, display: "Arrivée", detail: nil,
                      spoken: "vous êtes arrivé")
        } else if s.contains("demi-tour") || s.contains("demi tour") || s.contains("u-turn") {
            self.init(maneuver: .uTurn, display: "Demi-tour", detail: nil,
                      spoken: "faites demi-tour")
        } else if s.contains("rond-point") || s.contains("giratoire") || s.contains("roundabout") {
            // Au rond-point, le numéro de sortie prime sur le côté.
            let m: Maneuver = left ? .roundaboutLeft : .roundaboutRight
            let side = left ? "à gauche" : "à droite"
            if let exit {
                let nth = exit == 1 ? "1re" : "\(exit)e"
                self.init(
                    maneuver: m, display: "\(nth) sortie", detail: "rond-point",
                    spoken: "au rond-point, prenez la \(nth) sortie")
            } else {
                self.init(
                    maneuver: m, display: left ? "Gauche" : "Droite", detail: "rond-point",
                    spoken: "au rond-point, \(side)")
            }
        } else if left || right {
            let m: Maneuver
            if slight {
                m = left ? .slightLeft : .slightRight
            } else if sharp {
                m = left ? .sharpLeft : .sharpRight
            } else {
                m = left ? .left : .right
            }
            let side = left ? "à gauche" : "à droite"
            self.init(
                maneuver: m,
                display: left ? "Gauche" : "Droite",
                detail: slight ? "serrez" : (sharp ? "virage serré" : nil),
                spoken: slight ? "serrez \(side)" : (sharp ? "franchement \(side)" : side))
        } else if s.contains("sortie") || s.contains("exit") {
            // Sortie d'axe rapide sans côté précisé : par la droite en France.
            // Le numéro d'échangeur (« sortie 12 ») suit le mot, contrairement
            // au rang de sortie d'un rond-point (« 2e sortie ») qui le précède.
            if let number = Self.exitNumber(in: s) {
                self.init(
                    maneuver: .slightRight, display: "Sortie \(number)", detail: "sortie",
                    spoken: "prenez la sortie \(number)")
            } else if let exit {
                let nth = exit == 1 ? "1re" : "\(exit)e"
                self.init(
                    maneuver: .slightRight, display: "\(nth) sortie", detail: "sortie",
                    spoken: "prenez la \(nth) sortie")
            } else {
                self.init(
                    maneuver: .slightRight, display: "Sortie", detail: nil,
                    spoken: "prenez la sortie")
            }
        } else {
            self.init(maneuver: .straight, display: "Tout droit", detail: nil,
                      spoken: "tout droit")
        }
    }

    /// Numéro d'échangeur qui **suit** le mot sortie (« sortie 12 », « exit
    /// 23a »). nil si absent.
    private static func exitNumber(in s: String) -> String? {
        for keyword in ["sortie ", "exit "] {
            guard let r = s.range(of: keyword) else { continue }
            let after = s[r.upperBound...].prefix { $0.isNumber || $0.isLetter }
            let label = String(after)
            // Doit commencer par un chiffre : sinon c'est un mot (« sortie
            // suivante »), pas un numéro.
            if let f = label.first, f.isNumber, label.count <= 4 { return label }
        }
        return nil
    }

    /// Rang de sortie d'un rond-point (« la 2e sortie », « the 2nd exit »,
    /// « la deuxième sortie ») — il **précède** le mot. nil si absent.
    private static func roundaboutExit(in s: String) -> Int? {
        let words = [
            "premiere": 1, "deuxieme": 2, "troisieme": 3, "quatrieme": 4,
            "cinquieme": 5, "sixieme": 6, "first": 1, "second": 2, "third": 3,
        ]
        for (word, n) in words where s.contains(word) {
            return n
        }
        // Forme chiffrée : on prend le nombre qui précède « sortie » / « exit ».
        for keyword in ["sortie", "exit"] {
            guard let r = s.range(of: keyword) else { continue }
            let before = s[s.startIndex..<r.lowerBound]
            let digits = before.reversed().drop { !$0.isNumber }.prefix { $0.isNumber }
            let value = Int(String(digits.reversed()))
            if let value, (1...9).contains(value) { return value }
        }
        return nil
    }
}

/// Navigation turn-by-turn avec annonces vocales et recalcul automatique
/// quand on sort de l'itinéraire.
@MainActor
final class NavigationManager: ObservableObject {
    @Published var active = false
    @Published var rerouting = false
    @Published var instruction = NavInstruction.waiting.display
    /// Précision secondaire affichée sous la consigne (« 2e sortie »).
    @Published var detail: String?
    /// Manœuvre courante : pilote la flèche du bandeau, toujours cohérente
    /// avec `instruction` puisque les deux découlent de la même analyse.
    @Published var maneuver: Maneuver = .straight
    /// Formulation orale de la consigne courante, plus naturelle à l'écoute
    /// que le mot affiché.
    private var spokenInstruction = NavInstruction.waiting.spoken
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
    /// Annonce de préparation déjà faite pour l'étape courante.
    private var spokenPrepare = false
    /// Dernier point d'étape vocal (heure d'arrivée).
    private var lastEtaSpoken = Date()
    private var offRouteHits = 0
    private var lastReroute = Date.distantPast
    /// Recalculs consécutifs échoués (typiquement hors réseau) : espace les
    /// tentatives suivantes au lieu de réessayer toutes les 5 s.
    private var rerouteFailures = 0
    /// L'absence de réseau n'est annoncée qu'une fois par épisode.
    private var toldOffline = false

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

    /// Applique un itinéraire plus rapide trouvé en cours de route
    /// (« X minutes de gagnées »).
    func applyFasterRoute(route: MKRoute, savedMinutes: Int) {
        load(steps: Self.navSteps(from: route), coords: route.polyline.coordinates,
             km: route.distance / 1000, min: route.expectedTravelTime / 60,
             announceStart: false)
        speak("Itinéraire plus rapide trouvé : environ \(savedMinutes) minutes de gagnées.")
    }

    /// Applique un itinéraire de contournement quand on est bloqué dans un
    /// bouchon (« toujours avancer »).
    func applyDetour(route: MKRoute, savedMinutes: Int) {
        load(steps: Self.navSteps(from: route), coords: route.polyline.coordinates,
             km: route.distance / 1000, min: route.expectedTravelTime / 60,
             announceStart: false)
        if savedMinutes >= 1 {
            speak(
                "Ça bouchonne devant. Itinéraire de contournement, environ \(savedMinutes) minutes gagnées."
            )
        } else {
            speak("Ça bouchonne devant. Itinéraire de contournement.")
        }
    }

    /// Applique un itinéraire qui évite un bouchon signalé par la communauté,
    /// repéré en amont sur le trajet (avant même d'y être bloqué).
    func applyReportedJamDetour(route: MKRoute) {
        load(steps: Self.navSteps(from: route), coords: route.polyline.coordinates,
             km: route.distance / 1000, min: route.expectedTravelTime / 60,
             announceStart: false)
        speak("Bouchon signalé devant. Itinéraire pour l'éviter.")
    }

    /// Applique un itinéraire recalculé (sans annonce « Départ »).
    func applyReroute(route: MKRoute) {
        load(steps: Self.navSteps(from: route), coords: route.polyline.coordinates,
             km: route.distance / 1000, min: route.expectedTravelTime / 60,
             announceStart: false)
        rerouting = false
        rerouteFailures = 0
        toldOffline = false
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
        rerouteFailures = 0
        toldOffline = false
        RouteStore.clear()
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
                let retryDelay = 5.0 * Double(min(6, 1 + rerouteFailures))
                if offRouteHits >= 3, Date().timeIntervalSince(lastReroute) > retryDelay {
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
        apply(steps[stepIndex])

        // Deux annonces par manœuvre, pas plus : une pour préparer, une au
        // moment de tourner. (Il y en avait jusqu'à six, qui répétaient la
        // même consigne.)
        if d < 300, !spokenPrepare {
            speak("Dans \(roundedMeters(d)) mètres, \(spokenInstruction)")
            spokenPrepare = true
        }
        if d < 25 {
            // Dernière consigne au moment de tourner (sauf à l'arrivée, qui a
            // sa propre annonce).
            if stepIndex < steps.count - 1 {
                speak("Maintenant, \(spokenInstruction)")
            }
            advance()
        }

        // Point d'étape régulier : heure d'arrivée annoncée toutes les 10 min.
        if Date().timeIntervalSince(lastEtaSpoken) > 600 {
            lastEtaSpoken = Date()
            speak("Arrivée prévue vers \(Self.arrivalString(inMinutes: etaMinutes)).")
        }
    }

    /// Heure d'arrivée estimée (« 18:32 »).
    private static func arrivalString(inMinutes minutes: Double) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.dateFormat = "HH:mm"
        return f.string(from: Date().addingTimeInterval(minutes * 60))
    }

    // MARK: - Privé

    private func triggerReroute() {
        offRouteHits = 0
        lastReroute = Date()
        rerouting = true
        maneuver = .straight
        instruction = "Recalcul de l'itinéraire…"
        detail = nil
        speak("Recalcul de l'itinéraire.")
        onReroute?()
    }

    /// Le recalcul a échoué (réseau indisponible le plus souvent).
    ///
    /// Sans cela `rerouting` restait vrai indéfiniment : le bandeau se figeait
    /// sur « Recalcul de l'itinéraire… » et le guidage ne reprenait jamais. On
    /// conserve donc l'itinéraire précédent, qui reste exploitable hors ligne.
    func rerouteFailed() {
        guard rerouting else { return }
        rerouting = false
        rerouteFailures += 1
        if stepIndex < steps.count { apply(steps[stepIndex]) }
        if !toldOffline {
            toldOffline = true
            speak("Recalcul impossible. Je garde l'itinéraire.")
        }
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
        spokenPrepare = false
        lastEtaSpoken = Date()
        if stepIndex < steps.count {
            apply(steps[stepIndex])
            if announceStart { speak("Départ, " + spokenInstruction) }
        }
        // Mémorise l'itinéraire : il ne pourrait pas être recalculé hors ligne.
        if let destination {
            RouteStore.save(
                destination: destination, coords: coords, steps: steps, km: km, minutes: min)
        }
    }

    private func advance() {
        stepIndex += 1
        spokenPrepare = false
        if stepIndex >= steps.count {
            speak("Vous êtes arrivé à destination.")
            active = false
            RouteStore.clear()
        }
    }

    /// Applique une étape : consigne courte (sans nom de rue) + flèche
    /// correspondante, dérivées de la même manœuvre.
    private func apply(_ step: NavStep) {
        let n = NavInstruction(from: step.text)
        maneuver = n.maneuver
        instruction = n.display
        detail = n.detail
        spokenInstruction = n.spoken
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
