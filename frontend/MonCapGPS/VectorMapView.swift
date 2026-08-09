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
    let initialCenter: CLLocationCoordinate2D
    /// Remonte la zone visible, pour savoir quoi télécharger.
    let onRegionChange: (MLNCoordinateBounds) -> Void

    func makeUIView(context: Context) -> MLNMapView {
        let view = MLNMapView(frame: .zero, styleURL: styleURL)
        view.delegate = context.coordinator
        view.showsUserLocation = true
        view.setCenter(initialCenter, zoomLevel: 12, animated: false)
        view.logoView.isHidden = false  // attribution requise par la licence
        return view
    }

    func updateUIView(_ view: MLNMapView, context: Context) {
        // Le centre n'est pas réimposé après l'ouverture : l'utilisateur
        // resterait sinon collé au point de départ à chaque rafraîchissement.
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(route: route, onRegionChange: onRegionChange)
    }

    final class Coordinator: NSObject, MLNMapViewDelegate {
        private let route: [CLLocationCoordinate2D]
        private let onRegionChange: (MLNCoordinateBounds) -> Void

        init(
            route: [CLLocationCoordinate2D],
            onRegionChange: @escaping (MLNCoordinateBounds) -> Void
        ) {
            self.route = route
            self.onRegionChange = onRegionChange
        }

        /// L'itinéraire ne peut être ajouté qu'une fois le style chargé : les
        /// couches se greffent dessus.
        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
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

    /// Supprime toutes les zones enregistrées.
    func clearAll() {
        guard let packs = MLNOfflineStorage.shared.packs else { return }
        for pack in packs {
            MLNOfflineStorage.shared.removePack(pack, withCompletionHandler: nil)
        }
        progress = 0
        message = nil
    }

    /// Espace occupé par les zones enregistrées, en octets.
    func storedBytes() -> Int {
        guard let packs = MLNOfflineStorage.shared.packs else { return 0 }
        return packs.reduce(0) { $0 + Int($1.progress.countOfBytesCompleted) }
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
        }
    }

    @objc private func failed(_ note: Notification) {
        Task { @MainActor in
            self.running = false
            self.message = "Téléchargement interrompu."
        }
    }
}
