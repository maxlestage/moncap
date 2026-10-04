import CoreLocation
import MapKit
import SwiftUI
import UIKit

/// La carte de conduite rendue par **Apple Plans**.
///
/// MapLibre reste le moteur de tout le reste — cartes hors ligne, feux, qualité
/// de l'air, CarPlay — et pour une raison qui n'a pas changé : Apple Plans ne
/// permet pas d'enregistrer ses tuiles. C'est ce qui l'avait fait retirer.
///
/// Mais rouler et préparer une zone ne sont pas le même métier. Pour se
/// déplacer, Apple Plans apporte ce qu'OpenStreetMap rend mal : des noms de
/// commerces à jour, un rendu lisible d'un coup d'œil, et le trafic. Ce fichier
/// lui rend donc la carte de conduite, et elle seule.
///
/// **Le hors ligne garde la main.** Dès que le réseau manque — par choix ou
/// parce qu'une vallée l'a décidé — on repasse à MapLibre, qui a les tuiles en
/// réserve. Apple Plans afficherait un écran vide, ce qui est pire qu'un fond
/// moins joli. La règle est dans `FondDeCarte.effectif`, et elle est testée.
///
/// Le contrat est celui de `MainMapView`, au style près : mêmes épingles, mêmes
/// tracés, même caméra, mêmes rappels. L'écran de conduite ne sait pas laquelle
/// des deux il affiche, et c'est voulu — sans quoi chaque ajout aurait été à
/// faire deux fois.

// MARK: - Le choix du fond

/// Quel moteur rend la carte de conduite.
enum FondDeCarte: String, CaseIterable, Identifiable {
    /// Apple Plans. Demande le réseau.
    case apple
    /// OpenStreetMap par MapLibre. Marche sans réseau, et sait se télécharger.
    case osm

    var id: String { rawValue }

    /// Nom affiché.
    ///
    /// `LocalizedStringKey` et non `String` : Apple nomme son application
    /// différemment selon la langue — Plans, Mapas, Maps — et afficher « Apple
    /// Plans » à un hispanophone désignerait une app qu'il ne connaît pas sous
    /// ce nom. OpenStreetMap, lui, s'appelle pareil partout, et sa traduction
    /// est donc l'identité.
    var libelle: LocalizedStringKey {
        switch self {
        case .apple: return "Apple Plans"
        case .osm: return "OpenStreetMap"
        }
    }

    /// Clé de réglage. Nouvelle : les anciennes installations n'ont rien à
    /// migrer, et la valeur par défaut vaut pour tout le monde.
    static let cle = "moncap.fondDeCarte"

    /// Le fond réellement utilisable, qui n'est pas toujours celui demandé.
    ///
    /// Apple Plans ne garde aucune tuile : sans réseau, sa carte est vide. Un
    /// fond moins beau vaut mieux qu'un écran gris au moment précis où l'on a
    /// besoin de savoir où tourner — c'est tout l'intérêt des zones
    /// téléchargées, et les perdre en basculant sur Apple aurait été un
    /// mauvais échange.
    ///
    /// `horsLigne` est un choix de l'utilisateur, `reseauCoupe` un fait. Les
    /// deux mènent au même repli : dans un cas on a dit qu'on n'y comptait
    /// pas, dans l'autre on n'y peut rien.
    static func effectif(choix: FondDeCarte, horsLigne: Bool, reseauCoupe: Bool) -> FondDeCarte {
        guard choix == .apple else { return .osm }
        return (horsLigne || reseauCoupe) ? .osm : .apple
    }
}

// MARK: - Cadrage

/// La traduction entre les deux façons de cadrer une carte.
///
/// Tout l'écran de conduite raisonne en **niveaux de zoom**, la convention de
/// MapLibre reprise par `MapCam`. MapKit, lui, veut une distance en **mètres**.
/// Ce calcul est le seul endroit qui passe de l'un à l'autre — un facteur deux
/// ici, et la carte s'ouvre sur le mauvais quartier.
///
/// À part, et non dans le coordinateur qui s'en sert : c'est de l'arithmétique,
/// elle ne demande ni vue ni carte, et elle se teste donc sans en fabriquer.
enum CadrageApple {
    /// Mètres couverts par un point d'écran, au zoom et à la latitude donnés.
    ///
    /// Même convention que `MapCam` : au zoom 0 le tour du monde fait 512
    /// points. Mercator étant conforme, le rapport vaut dans les deux
    /// directions — un point représente le même nombre de mètres en largeur
    /// comme en hauteur, ce qui permet de n'en calculer qu'un.
    ///
    /// Le cosinus de la latitude n'est pas une finesse : l'oublier donnerait un
    /// cadrage juste à l'équateur et faux partout où l'app sert.
    static func metresParPoint(zoom: Double, lat: Double) -> Double {
        let tourDeTerre = 40_075_017.0
        return tourDeTerre * cos(lat * .pi / 180) / (512 * pow(2, max(0, zoom)))
    }
}

// MARK: - Vue

struct PlansAppleView: UIViewRepresentable {
    /// Caméra à appliquer. `nil` = laisser la carte où elle est.
    let cam: MapCam?
    let pins: [MapPin]
    let lines: [MapLine]
    /// Relief des bâtiments : réservé à la vue 3D.
    let buildings3D: Bool
    /// Trafic d'Apple. L'un des apports de ce fond : OpenStreetMap n'en a pas.
    var trafic: Bool = true
    let onPinTap: (String) -> Void
    /// Un lieu du fond de carte a été touché.
    let onPlaceTap: (CLLocationCoordinate2D, String) -> Void
    /// La carte a été touchée ailleurs.
    let onMapTap: (CLLocationCoordinate2D) -> Void
    var onMapLongPress: (CLLocationCoordinate2D) -> Void = { _ in }
    let onRegionChange: (MKCoordinateRegion) -> Void
    /// Geste manuel : sert à débrayer le suivi automatique.
    let onUserGesture: () -> Void

    func makeUIView(context: Context) -> PlansMapView {
        let view = PlansMapView(frame: .zero)
        let c = context.coordinator
        view.delegate = c

        // Pas de pastille bleue : la position est marquée par l'avatar, posé
        // comme épingle par la vue appelante — exactement comme sur MapLibre.
        view.showsUserLocation = false
        view.showsCompass = true
        // La boussole et l'échelle d'Apple remplacent celles de MapLibre.
        // L'attribution, elle, n'est pas notre affaire ici : MKMapView dessine
        // le logo Apple et son lien légal lui-même, et l'un comme l'autre sont
        // intouchables. C'est une contrainte en moins par rapport au fond
        // OpenStreetMap, dont il fallait placer le logo à la main.
        view.showsScale = false
        // Les lieux d'Apple deviennent sélectionnables : c'est l'équivalent
        // des couches `poi_*` d'OpenMapTiles, et ce qui fait marcher
        // `onPlaceTap` sans travail supplémentaire.
        view.selectableMapFeatures = [.pointsOfInterest]

        c.buildings3D = buildings3D
        c.trafic = trafic
        appliqueConfiguration(on: view)

        // Filet de sécurité identique à celui de MapLibre : la toute première
        // caméra arrive parfois avant que la carte n'ait sa place à l'écran, et
        // à l'arrêt rien ne la réémet.
        view.onLayout = { [weak view, weak c] in
            guard let view, let c else { return }
            c.applyPendingCam(to: view)
        }

        let tap = UITapGestureRecognizer(
            target: c, action: #selector(Coordinator.handleTap(_:)))
        tap.delegate = c
        // Comme sur MapLibre : sans cela le premier appui d'un double-tap de
        // zoom déclenchait déjà une action.
        for existing in view.gestureRecognizers ?? []
        where (existing as? UITapGestureRecognizer)?.numberOfTapsRequired == 2 {
            tap.require(toFail: existing)
        }
        view.addGestureRecognizer(tap)

        let pan = UIPanGestureRecognizer(
            target: c, action: #selector(Coordinator.handleGesture(_:)))
        pan.delegate = c
        view.addGestureRecognizer(pan)
        let pinch = UIPinchGestureRecognizer(
            target: c, action: #selector(Coordinator.handleGesture(_:)))
        pinch.delegate = c
        view.addGestureRecognizer(pinch)

        // Appui long : désigner un point comme destination. Le seul chemin
        // vers un but qui ne demande rien au réseau.
        let appuiLong = UILongPressGestureRecognizer(
            target: c, action: #selector(Coordinator.handleLongPress(_:)))
        appuiLong.minimumPressDuration = 0.45
        appuiLong.delegate = c
        view.addGestureRecognizer(appuiLong)

        if let cam {
            c.appliedCam = cam
            c.apply(cam, to: view, animated: false)
        }
        return view
    }

    func updateUIView(_ view: PlansMapView, context: Context) {
        let c = context.coordinator
        c.onPinTap = onPinTap
        c.onPlaceTap = onPlaceTap
        c.onMapTap = onMapTap
        c.onMapLongPress = onMapLongPress
        c.onRegionChange = onRegionChange
        c.onUserGesture = onUserGesture

        if c.buildings3D != buildings3D || c.trafic != trafic {
            c.buildings3D = buildings3D
            c.trafic = trafic
            appliqueConfiguration(on: view)
        }

        if let cam, cam != c.appliedCam {
            c.appliedCam = cam
            c.apply(cam, to: view, animated: true)
        }
        c.sync(pins: pins, on: view)
        c.sync(lines: lines, on: view)
    }

    private func appliqueConfiguration(on view: PlansMapView) {
        let config = MKStandardMapConfiguration(elevationStyle: buildings3D ? .realistic : .flat)
        config.showsTraffic = trafic
        config.pointOfInterestFilter = .includingAll
        view.preferredConfiguration = config
        // Les bâtiments ne se voient qu'inclinée ; à plat le relief ne coûte
        // rien à désactiver et la carte se dessine plus vite.
        view.showsBuildings = buildings3D
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    // MARK: Coordinateur

    final class Coordinator: NSObject, MKMapViewDelegate, UIGestureRecognizerDelegate {
        var onPinTap: (String) -> Void = { _ in }
        var onPlaceTap: (CLLocationCoordinate2D, String) -> Void = { _, _ in }
        var onMapTap: (CLLocationCoordinate2D) -> Void = { _ in }
        var onMapLongPress: (CLLocationCoordinate2D) -> Void = { _ in }
        var onRegionChange: (MKCoordinateRegion) -> Void = { _ in }
        var onUserGesture: () -> Void = {}

        var appliedCam: MapCam?
        var buildings3D = false
        var trafic = true

        private var laidOut = false
        /// Épingles en place, par identifiant.
        private var placed: [String: PlansAnnotation] = [:]
        /// Signatures des tracés dessinés, pour ne rien refaire pour rien.
        private var lineSignatures: [String] = []
        private var placedLines: [MKPolyline] = []
        /// Apparence de chaque tracé, retrouvée par le rendu.
        private var lineLooks: [ObjectIdentifier: MapLine] = [:]
        /// Un lieu d'Apple vient d'être sélectionné : le geste qui l'a touché
        /// ne doit pas aussi compter comme un appui « ailleurs ».
        private var derniereSelection: Date?

        // MARK: Caméra

        /// Rapport entre la distance caméra que veut MapKit et l'étendue au sol.
        ///
        /// MapKit ne publie pas son champ de vision : impossible de calculer de
        /// tête la distance caméra qui montre tant de mètres. Plutôt que de la
        /// deviner, on la lui demande — un `setRegion` sur une région connue,
        /// et sa caméra nous dit quelle distance il en a déduite. Mesuré une
        /// fois, puis réutilisé pour toutes les caméras tournées ou inclinées,
        /// que `setRegion` ne sait pas exprimer.
        private var rapportDistance: Double?

        /// Valeur d'attente, le temps que la mesure arrive : celle d'un champ
        /// de vision vertical de 30°, soit 1 / (2·tan 15°). Elle ne sert qu'aux
        /// toutes premières images si la carte s'ouvre déjà inclinée, et la
        /// mesure la remplace dès le premier cadrage à plat.
        private static let rapportSuppose = 1.866

        func apply(_ cam: MapCam, to view: MKMapView, animated: Bool) {
            let taille = view.bounds.size
            guard taille.height > 10 else { return }

            let mpp = CadrageApple.metresParPoint(zoom: cam.zoom, lat: cam.center.latitude)
            let hauteur = Double(taille.height) * mpp
            let largeur = Double(taille.width) * mpp

            let heading = cam.heading
            let pitch = max(0, min(60, cam.pitch))

            // À plat et sans rotation, `setRegion` fait mieux que nous : il
            // tient compte du format de l'écran et des marges. On profite du
            // passage pour mesurer le rapport dont les caméras inclinées ont
            // besoin.
            if heading == 0, pitch == 0 {
                view.setRegion(
                    MKCoordinateRegion(
                        center: cam.center,
                        latitudinalMeters: max(50, hauteur),
                        longitudinalMeters: max(50, largeur)),
                    animated: animated)
                // La mesure est bornée avant d'être retenue. Pendant une
                // animation, `camera` peut être lue à mi-course et donner un
                // rapport absurde ; un cadrage figé de travers serait pire que
                // la valeur d'attente. Hors de cette plage, on garde ce qu'on
                // avait — un prochain cadrage à plat remesurera.
                let mesure = view.camera.centerCoordinateDistance / max(1, hauteur)
                if (0.5...6).contains(mesure) { rapportDistance = mesure }
                return
            }

            let camera = MKMapCamera(
                lookingAtCenter: cam.center,
                fromDistance: hauteur * (rapportDistance ?? Self.rapportSuppose),
                pitch: CGFloat(pitch), heading: heading)
            view.setCamera(camera, animated: animated)
        }

        func applyPendingCam(to view: MKMapView) {
            guard !laidOut, view.bounds.height > 10 else { return }
            laidOut = true
            if let cam = appliedCam { apply(cam, to: view, animated: false) }
        }

        // MARK: Épingles

        func sync(pins: [MapPin], on view: MKMapView) {
            var seen = Set<String>()
            for pin in pins {
                seen.insert(pin.id)
                if let existing = placed[pin.id] {
                    if existing.look != pin.look {
                        // L'allure a changé : la vue est recréée, son image
                        // étant posée à la création.
                        view.removeAnnotation(existing)
                        placed[pin.id] = add(pin, to: view)
                    } else if !same(existing.coordinate, pin.coordinate) {
                        // Déplacer plutôt que recréer : évite un clignotement,
                        // et c'est le cas de tous les instants pour l'avatar.
                        existing.coordinate = pin.coordinate
                    }
                } else {
                    placed[pin.id] = add(pin, to: view)
                }
            }
            for (id, annotation) in placed where !seen.contains(id) {
                view.removeAnnotation(annotation)
                placed.removeValue(forKey: id)
            }
        }

        private func add(_ pin: MapPin, to view: MKMapView) -> PlansAnnotation {
            let a = PlansAnnotation()
            a.coordinate = pin.coordinate
            a.title = pin.title
            a.pinID = pin.id
            a.look = pin.look
            view.addAnnotation(a)
            return a
        }

        private func same(_ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D) -> Bool {
            abs(a.latitude - b.latitude) < 1e-7 && abs(a.longitude - b.longitude) < 1e-7
        }

        // MARK: Tracés

        func sync(lines: [MapLine], on view: MKMapView) {
            let signatures = lines.map(\.signature)
            guard signatures != lineSignatures else { return }
            lineSignatures = signatures

            view.removeOverlays(placedLines)
            placedLines = []
            lineLooks = [:]

            // L'ordre de la liste fait l'ordre d'empilement : le dernier ajouté
            // passe devant, et c'est l'itinéraire retenu. Même convention que
            // sur MapLibre, pour que l'appelant n'ait rien à savoir.
            for line in lines where line.coordinates.count >= 2 {
                let trace = MKPolyline(
                    coordinates: line.coordinates, count: line.coordinates.count)
                lineLooks[ObjectIdentifier(trace)] = line
                placedLines.append(trace)
                view.addOverlay(trace, level: .aboveRoads)
            }
        }

        // MARK: Délégué

        func mapView(_ mapView: MKMapView, viewFor annotation: MKAnnotation) -> MKAnnotationView? {
            // Les lieux d'Apple gardent leur propre allure : ce sont les
            // siens, et les redessiner les rendrait moins reconnaissables.
            if annotation is MKMapFeatureAnnotation { return nil }
            guard let pin = annotation as? PlansAnnotation, let look = pin.look else { return nil }

            // Réutilisation par allure, comme chez MapLibre : deux épingles de
            // même aspect partagent le même dessin, ce qui compte dès qu'une
            // catégorie de lieux en affiche des dizaines.
            let key = String(look.hashValue)
            let vue =
                mapView.dequeueReusableAnnotationView(withIdentifier: key)
                ?? MKAnnotationView(annotation: annotation, reuseIdentifier: key)
            vue.annotation = annotation
            vue.image = PinRenderer.image(for: look)
            // Nos propres panneaux remplacent les bulles d'Apple.
            vue.canShowCallout = false
            vue.centerOffset = .zero
            return vue
        }

        func mapView(_ mapView: MKMapView, rendererFor overlay: MKOverlay) -> MKOverlayRenderer {
            guard let trace = overlay as? MKPolyline,
                let look = lineLooks[ObjectIdentifier(trace)]
            else { return MKOverlayRenderer(overlay: overlay) }

            let rendu = MKPolylineRenderer(polyline: trace)
            rendu.strokeColor = UIColor(hex: look.color)
                .withAlphaComponent(CGFloat(look.opacity))
            rendu.lineWidth = CGFloat(look.width)
            rendu.lineCap = .round
            rendu.lineJoin = .round
            if look.dashed {
                // MapLibre compte les pointillés en multiples de l'épaisseur,
                // MapKit en points : on convertit, sans quoi un trait épais
                // donnait des tirets minuscules.
                rendu.lineDashPattern = [
                    NSNumber(value: look.width * 2), NSNumber(value: look.width * 1.5),
                ]
            }
            return rendu
        }

        func mapView(_ mapView: MKMapView, didSelect annotation: MKAnnotation) {
            mapView.deselectAnnotation(annotation, animated: false)
            derniereSelection = Date()

            // Un lieu du fond de carte : c'est l'équivalent exact des couches
            // `poi_*` interrogées sur le fond OpenStreetMap.
            if let lieu = annotation as? MKMapFeatureAnnotation {
                let nom = lieu.title ?? ""
                guard !nom.isEmpty else { return }
                onPlaceTap(lieu.coordinate, nom)
                return
            }
            guard let pin = annotation as? PlansAnnotation else { return }
            onPinTap(pin.pinID)
        }

        func mapView(_ mapView: MKMapView, regionDidChangeAnimated animated: Bool) {
            onRegionChange(mapView.region)
        }

        // MARK: Gestes

        @objc func handleLongPress(_ g: UILongPressGestureRecognizer) {
            // `.began` seulement : le geste continue d'émettre tant que le
            // doigt reste posé, et on ne veut qu'une destination par appui.
            guard g.state == .began, let view = g.view as? MKMapView else { return }
            onMapLongPress(view.convert(g.location(in: view), toCoordinateFrom: view))
        }

        @objc func handleTap(_ g: UITapGestureRecognizer) {
            guard let view = g.view as? MKMapView else { return }
            let point = g.location(in: view)

            // Une épingle sous le doigt est déjà traitée par `didSelect` : sans
            // ce test, l'appui déclenchait les deux chemins.
            //
            // On remonte la chaîne des vues parentes : `hitTest` rend la vue la
            // plus profonde, qui peut être une sous-vue de l'épingle et non
            // l'épingle elle-même. S'arrêter au premier niveau laissait donc
            // passer l'appui.
            var touchee = view.hitTest(point, with: nil)
            while let v = touchee, v !== view {
                if v is MKAnnotationView { return }
                touchee = v.superview
            }

            // Les lieux d'Apple, eux, sont dessinés dans la tuile et non en
            // sous-vue : aucun test de contact ne les trouve. Mais leur
            // sélection arrive dans le même touché que ce geste, donc une
            // sélection toute fraîche signifie que l'appui visait un lieu et a
            // déjà été servi.
            if let t = derniereSelection, Date().timeIntervalSince(t) < 0.4 { return }

            onMapTap(view.convert(point, toCoordinateFrom: view))
        }

        @objc func handleGesture(_ g: UIGestureRecognizer) {
            if g.state == .began || g.state == .changed { onUserGesture() }
        }

        func gestureRecognizer(
            _ g: UIGestureRecognizer,
            shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
        ) -> Bool { true }
    }
}

/// Carte Apple qui signale sa mise en page.
///
/// `UIViewRepresentable` n'expose aucun moment « la vue a sa taille », et la
/// conversion zoom → distance en a besoin : sans largeur ni hauteur, aucun
/// cadrage n'a de sens. Même raison que `SizedMapView` du côté MapLibre.
final class PlansMapView: MKMapView {
    var onLayout: (() -> Void)?

    override func layoutSubviews() {
        super.layoutSubviews()
        onLayout?()
    }
}

/// Épingle Apple enrichie de ce qu'il faut pour la dessiner et la reconnaître.
final class PlansAnnotation: MKPointAnnotation {
    var pinID = ""
    var look: PinLook?
}
