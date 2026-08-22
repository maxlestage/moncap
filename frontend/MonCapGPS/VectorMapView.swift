import CoreLocation
import MapLibre
import SwiftUI

/// Carte **vectorielle** hors ligne, rendue par MapLibre.
///
/// Les archives PMTiles produites par `planetiler` à partir d'OpenStreetMap
/// sont vectorielles : bien plus légères qu'un jeu de tuiles images, mais elles
/// demandent un moteur de rendu, que MapKit n'a pas. MapLibre apporte en prime
/// sa propre gestion du hors ligne (packs de régions), utilisée ici plutôt que
/// de retélécharger des tuiles à la main.
struct VectorMapView: UIViewRepresentable {
    /// Style de carte (JSON), servi par notre backend.
    let styleURL: URL
    /// Tracé de l'itinéraire mémorisé, s'il y en a un.
    let route: [CLLocationCoordinate2D]
    /// Zones déjà enregistrées, dessinées en bleu sur la carte : on
    /// reconnaît d'un regard ce qui est déjà téléchargé.
    var zones: [VectorPackDownloader.ZoneEnregistree] = []
    /// Couloirs d'itinéraire enregistrés, dessinés en trait large.
    var couloirs: [[CLLocationCoordinate2D]] = []
    /// Cadrage demandé : à l'ouverture, puis à chaque lieu choisi dans la
    /// recherche. Chaque valeur porte une identité, donc redemander le même
    /// endroit y ramène bien.
    let cam: MapCam
    /// Remonte la zone visible, pour savoir quoi télécharger.
    let onRegionChange: (MLNCoordinateBounds) -> Void

    func makeUIView(context: Context) -> MLNMapView {
        let view = MLNMapView(frame: .zero, styleURL: styleURL)
        view.delegate = context.coordinator
        view.showsUserLocation = true
        view.setCenter(cam.center, zoomLevel: cam.zoom, animated: false)
        context.coordinator.appliedCam = cam
        view.logoView.isHidden = false  // attribution requise par la licence
        return view
    }

    func updateUIView(_ view: MLNMapView, context: Context) {
        // Le centre n'est pas réimposé après l'ouverture : l'utilisateur
        // resterait sinon collé au point de départ à chaque rafraîchissement.
        //
        // Le style, lui, doit suivre : il est résolu de façon asynchrone et
        // arrive après la construction de la vue. Sans cela un serveur
        // personnel n'était jamais affiché, et les zones téléchargées ne
        // correspondaient pas à la carte visible.
        // La comparaison porte sur ce qu'on a demandé, pas sur ce que la vue
        // renvoie : pendant le chargement celle-ci ne vaut pas encore la valeur
        // posée, et le style repartirait de zéro sans jamais aboutir.
        if context.coordinator.appliedStyle != styleURL {
            context.coordinator.appliedStyle = styleURL
            view.styleURL = styleURL
        }
        // Le tracé mémorisé est lu après coup (`onAppear`) : figé à la
        // création, il valait toujours la liste vide.
        if cam != context.coordinator.appliedCam {
            context.coordinator.appliedCam = cam
            view.setCenter(cam.center, zoomLevel: cam.zoom, animated: true)
        }
        context.coordinator.update(route: route, on: view)
        context.coordinator.update(zones: zones, couloirs: couloirs, on: view)
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(route: route, onRegionChange: onRegionChange)
    }

    final class Coordinator: NSObject, MLNMapViewDelegate {
        private var route: [CLLocationCoordinate2D]
        private var zones: [VectorPackDownloader.ZoneEnregistree] = []
        private var couloirs: [[CLLocationCoordinate2D]] = []
        private let onRegionChange: (MLNCoordinateBounds) -> Void
        /// Style demandé à la vue ; fait foi pendant le chargement.
        var appliedStyle: URL?
        var appliedCam: MapCam?

        init(
            route: [CLLocationCoordinate2D],
            onRegionChange: @escaping (MLNCoordinateBounds) -> Void
        ) {
            self.route = route
            self.onRegionChange = onRegionChange
        }

        /// Mémorise le tracé et le dessine si le style est déjà là.
        func update(route: [CLLocationCoordinate2D], on view: MLNMapView) {
            guard !same(route, self.route) else { return }
            self.route = route
            if let style = view.style { draw(on: style) }
        }

        /// Deux tracés identiques ? Le nombre de points seul ne suffit pas :
        /// un itinéraire recalculé en compte souvent autant.
        private func same(_ a: [CLLocationCoordinate2D], _ b: [CLLocationCoordinate2D]) -> Bool {
            guard a.count == b.count else { return false }
            guard let x = a.first, let y = b.first, let u = a.last, let v = b.last else {
                return true
            }
            return abs(x.latitude - y.latitude) < 1e-7 && abs(x.longitude - y.longitude) < 1e-7
                && abs(u.latitude - v.latitude) < 1e-7 && abs(u.longitude - v.longitude) < 1e-7
        }

        /// L'itinéraire ne peut être ajouté qu'une fois le style chargé : les
        /// couches se greffent dessus.
        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            // Rechargé, le style a perdu ses couches : tout est reposé.
            draw(on: style)
            drawZones(on: style)
        }

        /// Mémorise les zones enregistrées et les redessine si besoin.
        func update(
            zones: [VectorPackDownloader.ZoneEnregistree],
            couloirs: [[CLLocationCoordinate2D]], on view: MLNMapView
        ) {
            guard zones != self.zones || couloirs.count != self.couloirs.count
            else { return }
            self.zones = zones
            self.couloirs = couloirs
            if let style = view.style { drawZones(on: style) }
        }

        /// Les zones déjà enregistrées, en bleu translucide bordé : le seul
        /// moyen de reconnaître d'un regard ce qui est déjà téléchargé — le
        /// bouton, lui, ne le disait que si l'on cadrait pile dessus.
        private func drawZones(on style: MLNStyle) {
            for id in ["zones-fond", "zones-bord", "couloirs-trace"] {
                if let layer = style.layer(withIdentifier: id) {
                    style.removeLayer(layer)
                }
            }
            for id in ["zones-enregistrees", "couloirs-enregistres"] {
                if let source = style.source(withIdentifier: id) {
                    style.removeSource(source)
                }
            }

            if !zones.isEmpty {
                let polygones = zones.map { z -> MLNPolygonFeature in
                    var coins = [
                        CLLocationCoordinate2D(latitude: z.swLat, longitude: z.swLon),
                        CLLocationCoordinate2D(latitude: z.swLat, longitude: z.neLon),
                        CLLocationCoordinate2D(latitude: z.neLat, longitude: z.neLon),
                        CLLocationCoordinate2D(latitude: z.neLat, longitude: z.swLon),
                    ]
                    return MLNPolygonFeature(coordinates: &coins, count: 4)
                }
                let source = MLNShapeSource(
                    identifier: "zones-enregistrees",
                    shape: MLNShapeCollectionFeature(shapes: polygones), options: nil)
                style.addSource(source)
                let fond = MLNFillStyleLayer(identifier: "zones-fond", source: source)
                fond.fillColor = NSExpression(forConstantValue: UIColor.systemBlue)
                fond.fillOpacity = NSExpression(forConstantValue: 0.10)
                style.addLayer(fond)
                let bord = MLNLineStyleLayer(identifier: "zones-bord", source: source)
                bord.lineColor = NSExpression(forConstantValue: UIColor.systemBlue)
                bord.lineWidth = NSExpression(forConstantValue: 1.5)
                bord.lineDashPattern = NSExpression(forConstantValue: [2, 2])
                style.addLayer(bord)
            }

            if !couloirs.isEmpty {
                let lignes = couloirs.map { trace -> MLNPolylineFeature in
                    var coords = trace
                    return MLNPolylineFeature(
                        coordinates: &coords, count: UInt(coords.count))
                }
                let source = MLNShapeSource(
                    identifier: "couloirs-enregistres",
                    shape: MLNShapeCollectionFeature(shapes: lignes), options: nil)
                style.addSource(source)
                let couche = MLNLineStyleLayer(
                    identifier: "couloirs-trace", source: source)
                couche.lineColor = NSExpression(forConstantValue: UIColor.systemBlue)
                couche.lineWidth = NSExpression(forConstantValue: 9)
                couche.lineOpacity = NSExpression(forConstantValue: 0.18)
                couche.lineCap = NSExpression(forConstantValue: "round")
                style.addLayer(couche)
            }
        }

        private func draw(on style: MLNStyle) {
            if let layer = style.layer(withIdentifier: "itineraire-trace") {
                style.removeLayer(layer)
            }
            if let source = style.source(withIdentifier: "itineraire") {
                style.removeSource(source)
            }
            guard route.count >= 2 else { return }
            var coords = route
            let line = MLNPolylineFeature(coordinates: &coords, count: UInt(coords.count))
            let source = MLNShapeSource(identifier: "itineraire", shape: line, options: nil)
            style.addSource(source)
            let layer = MLNLineStyleLayer(identifier: "itineraire-trace", source: source)
            layer.lineColor = NSExpression(forConstantValue: UIColor.systemBlue)
            layer.lineWidth = NSExpression(forConstantValue: 5)
            layer.lineCap = NSExpression(forConstantValue: "round")
            style.addLayer(layer)
        }

        func mapView(_ mapView: MLNMapView, regionDidChangeAnimated animated: Bool) {
            onRegionChange(mapView.visibleCoordinateBounds)
        }
    }
}

/// Téléchargement hors ligne d'une zone vectorielle, via les packs MapLibre.
///
/// MapLibre sait déjà découper une zone en tuiles, les récupérer et les garder
/// : inutile de refaire ce travail. On se contente de suivre l'avancement.
@MainActor
final class VectorPackDownloader: NSObject, ObservableObject {
    @Published private(set) var running = false
    @Published private(set) var progress: Double = 0
    @Published private(set) var message: String?
    /// Espace occupé par les zones enregistrées, en octets.
    @Published private(set) var storedBytes = 0
    /// Nombre de zones enregistrées.
    @Published private(set) var packCount = 0
    /// Les rectangles des zones enregistrées, prêts à dessiner : c'est le
    /// seul moyen de RECONNAÎTRE d'un regard ce qui est déjà téléchargé —
    /// le bouton ne le disait que si l'on cadrait pile dessus (constaté).
    @Published private(set) var zones: [ZoneEnregistree] = []
    /// Les couloirs d'itinéraire enregistrés (packs en forme de tracé).
    @Published private(set) var couloirs: [[CLLocationCoordinate2D]] = []

    /// Un rectangle enregistré, en quatre nombres : `CLLocationCoordinate2D`
    /// n'est pas Equatable, les doubles le sont.
    struct ZoneEnregistree: Equatable {
        let swLat: Double
        let swLon: Double
        let neLat: Double
        let neLon: Double
    }

    override init() {
        super.init()
        let center = NotificationCenter.default
        center.addObserver(
            self, selector: #selector(progressChanged(_:)),
            name: NSNotification.Name.MLNOfflinePackProgressChanged, object: nil)
        center.addObserver(
            self, selector: #selector(failed(_:)),
            name: NSNotification.Name.MLNOfflinePackError, object: nil)
    }

    /// Lance le téléchargement de la zone visible.
    func start(styleURL: URL, bounds: MLNCoordinateBounds, maxZoom: Double) {
        guard !running else { return }
        let region = MLNTilePyramidOfflineRegion(
            styleURL: styleURL, bounds: bounds, fromZoomLevel: 8, toZoomLevel: maxZoom)
        launch(region)
    }

    /// Enregistre une région, quelle que soit sa forme.
    private func launch(_ region: MLNOfflineRegion) {
        // Le contexte est libre : on y met de quoi reconnaître nos packs.
        let context = Data("moncap".utf8)
        running = true
        progress = 0
        message = nil
        MLNOfflineStorage.shared.addPack(for: region, withContext: context) {
            [weak self] pack, error in
            guard let self else { return }
            if let error {
                // MapLibre rappelle hors du fil principal : publier l'état
                // directement mettrait à jour l'interface depuis un autre fil.
                Task { @MainActor in
                    self.running = false
                    self.message = error.localizedDescription
                }
                return
            }
            pack?.resume()
        }
    }

    /// Enregistre un couloir suivant l'itinéraire.
    ///
    /// C'est ce qui rend un long trajet praticable hors réseau : seules les
    /// tuiles traversées sont gardées, là où le rectangle englobant en
    /// réclamerait dix à trente fois plus sur plusieurs centaines de kilomètres.
    func startRoute(styleURL: URL, route: [CLLocationCoordinate2D], maxZoom: Double) {
        guard !running, route.count >= 2 else { return }
        var coords = route
        let line = MLNPolyline(coordinates: &coords, count: UInt(coords.count))
        let region = MLNShapeOfflineRegion(
            styleURL: styleURL, shape: line, fromZoomLevel: 6, toZoomLevel: maxZoom)
        launch(region)
    }

    /// Supprime toutes les zones enregistrées.
    func clearAll() {
        guard let packs = MLNOfflineStorage.shared.packs else { return }
        for pack in packs {
            MLNOfflineStorage.shared.removePack(pack, withCompletionHandler: nil)
        }
        // Un pack retiré ne signale plus rien : sans cela le panneau restait
        // sur « Téléchargement en cours… » indéfiniment.
        running = false
        progress = 0
        message = nil
        storedBytes = 0
        packCount = 0
        zones = []
        couloirs = []
    }

    /// Redemande leur état aux zones enregistrées.
    ///
    /// Une zone au repos ignore sa propre taille : MapLibre ne remplit
    /// `progress` qu'à la demande. Sans cela l'écran annonçait « Aucune zone
    /// enregistrée » alors que les cartes étaient bien présentes.
    ///
    /// La liste elle-même se charge en tâche de fond : au lancement elle est
    /// encore absente, sans que rien ne le signale — d'où les tentatives.
    func refresh(retries: Int = 6) {
        guard let packs = MLNOfflineStorage.shared.packs else {
            guard retries > 0 else { return }
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: 400_000_000)
                self.refresh(retries: retries - 1)
            }
            return
        }
        packCount = packs.count
        for pack in packs { pack.requestProgress() }
        recount(packs)
    }

    private func recount(_ packs: [MLNOfflinePack]) {
        packCount = packs.count
        storedBytes = packs.reduce(0) { $0 + Int($1.progress.countOfBytesCompleted) }
        var rectangles: [ZoneEnregistree] = []
        var traces: [[CLLocationCoordinate2D]] = []
        for pack in packs {
            if let r = pack.region as? MLNTilePyramidOfflineRegion {
                rectangles.append(
                    ZoneEnregistree(
                        swLat: r.bounds.sw.latitude, swLon: r.bounds.sw.longitude,
                        neLat: r.bounds.ne.latitude, neLon: r.bounds.ne.longitude))
            } else if let r = pack.region as? MLNShapeOfflineRegion,
                let ligne = r.shape as? MLNPolyline
            {
                let n = Int(ligne.pointCount)
                guard n >= 2 else { continue }
                var coords = [CLLocationCoordinate2D](
                    repeating: CLLocationCoordinate2D(), count: n)
                ligne.getCoordinates(&coords, range: NSRange(location: 0, length: n))
                traces.append(coords)
            }
        }
        if zones != rectangles { zones = rectangles }
        if couloirs.count != traces.count { couloirs = traces }
    }

    /// Une zone déjà enregistrée couvre-t-elle celle-ci, au même détail ?
    ///
    /// Sans ce test, réappuyer sur « Télécharger » empilait des copies de la
    /// même zone, chacune occupant à nouveau la place.
    func covers(bounds: MLNCoordinateBounds, maxZoom: Double) -> Bool {
        for pack in MLNOfflineStorage.shared.packs ?? [] {
            guard let r = pack.region as? MLNTilePyramidOfflineRegion,
                r.maximumZoomLevel >= maxZoom
            else { continue }
            let marge = 1e-6
            if r.bounds.sw.latitude <= bounds.sw.latitude + marge,
                r.bounds.sw.longitude <= bounds.sw.longitude + marge,
                r.bounds.ne.latitude >= bounds.ne.latitude - marge,
                r.bounds.ne.longitude >= bounds.ne.longitude - marge
            {
                return true
            }
        }
        return false
    }

    @objc private func progressChanged(_ note: Notification) {
        guard let pack = note.object as? MLNOfflinePack else { return }
        let p = pack.progress
        let expected = max(1, p.countOfResourcesExpected)
        Task { @MainActor in
            self.progress = Double(p.countOfResourcesCompleted) / Double(expected)
            if p.countOfResourcesCompleted >= expected {
                self.running = false
            }
            // Chaque signal renseigne aussi la place occupée : c'est par ce
            // chemin que les zones au repos finissent par se déclarer.
            self.recount(MLNOfflineStorage.shared.packs ?? [])
        }
    }

    @objc private func failed(_ note: Notification) {
        Task { @MainActor in
            self.running = false
            self.message = "Téléchargement interrompu."
        }
    }
}
