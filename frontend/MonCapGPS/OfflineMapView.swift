import MapKit
import MapLibre
import SwiftUI

/// Carte à tuiles téléchargées, en UIKit.
///
/// Le composant `Map` de SwiftUI ne sait pas afficher de tuiles personnalisées :
/// cet écran passe donc par `MKMapView`, seul moyen d'ajouter une couche
/// `MKTileOverlay`. La carte principale reste inchangée.
struct OfflineMapView: UIViewRepresentable {
    /// Tracé de l'itinéraire mémorisé, s'il y en a un.
    let route: [CLLocationCoordinate2D]
    /// Région d'ouverture (position de l'utilisateur en général).
    let initialRegion: MKCoordinateRegion
    /// Réseau disponible : hors ligne, on ne tente aucune requête de tuile.
    let online: Bool
    /// Remonte la région visible, pour savoir quoi télécharger.
    let onRegionChange: (MKCoordinateRegion) -> Void

    func makeUIView(context: Context) -> MKMapView {
        let view = MKMapView()
        view.delegate = context.coordinator
        view.showsUserLocation = true
        view.setRegion(initialRegion, animated: false)

        let overlay = CachedTileOverlay(urlTemplate: TileSource.template)
        // Remplace le fond Apple : hors ligne il resterait vide, et les deux
        // fonds superposés seraient illisibles.
        overlay.canReplaceMapContent = true
        overlay.allowsNetwork = online
        context.coordinator.overlay = overlay
        view.addOverlay(overlay, level: .aboveLabels)

        if route.count >= 2 {
            view.addOverlay(MKPolyline(coordinates: route, count: route.count))
        }
        return view
    }

    func updateUIView(_ view: MKMapView, context: Context) {
        context.coordinator.overlay?.allowsNetwork = online
        // La région n'est jamais réimposée après l'ouverture : l'utilisateur
        // resterait sinon collé au point de départ à chaque rafraîchissement.
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onRegionChange: onRegionChange)
    }

    final class Coordinator: NSObject, MKMapViewDelegate {
        var overlay: CachedTileOverlay?
        private let onRegionChange: (MKCoordinateRegion) -> Void

        init(onRegionChange: @escaping (MKCoordinateRegion) -> Void) {
            self.onRegionChange = onRegionChange
        }

        func mapView(_ mapView: MKMapView, rendererFor overlay: MKOverlay) -> MKOverlayRenderer {
            if let tiles = overlay as? MKTileOverlay {
                return MKTileOverlayRenderer(tileOverlay: tiles)
            }
            if let line = overlay as? MKPolyline {
                let renderer = MKPolylineRenderer(polyline: line)
                renderer.strokeColor = UIColor.systemBlue
                renderer.lineWidth = 6
                return renderer
            }
            return MKOverlayRenderer(overlay: overlay)
        }

        func mapView(_ mapView: MKMapView, regionDidChangeAnimated animated: Bool) {
            onRegionChange(mapView.region)
        }
    }
}

/// Écran « Carte hors ligne » : consultation des tuiles enregistrées et
/// téléchargement de la zone visible.
struct OfflineMapScreen: View {
    @Environment(\.dismiss) private var dismiss
    /// Position d'ouverture de la carte.
    let center: CLLocationCoordinate2D?

    @AppStorage(TileSource.defaultsKey) private var tileURL = ""
    /// Style vectoriel : renseigné, la carte est rendue par MapLibre plutôt
    /// que par des tuiles images.
    @AppStorage(TileSource.styleKey) private var styleURLString = ""
    @StateObject private var packs = VectorPackDownloader()
    @State private var bounds: MLNCoordinateBounds?
    /// Le style est-il réellement servi ? Un serveur sans cartes rendrait une
    /// carte grise sans le moindre message.
    @State private var styleStatus = StyleStatus.checking

    /// Style vectoriel exploitable, s'il y en a un.
    private var vectorStyle: URL? {
        let trimmed = styleURLString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix("http") else { return nil }
        return URL(string: trimmed)
    }

    /// Un serveur de tuiles exploitable est renseigné. Basé sur la valeur
    /// observée, pour que l'écran se mette à jour dès la saisie.
    private var configured: Bool {
        tileURL.contains("{z}") && tileURL.contains("{x}") && tileURL.contains("{y}")
    }
    @StateObject private var downloader = TileDownloader()
    @State private var region = MKCoordinateRegion()
    @State private var storedBytes = 0
    @State private var estimate: (count: Int, bytes: Int) = (0, 0)
    @State private var depth = Depth.ville
    @State private var confirmClear = false
    /// Tracé mémorisé, lu une seule fois (le corps de la vue est réévalué
    /// souvent : y lire le disque serait coûteux).
    @State private var route: [CLLocationCoordinate2D] = []

    /// Profondeur de zoom téléchargée : plus on descend, plus c'est détaillé
    /// et lourd.
    private enum Depth: String, CaseIterable, Identifiable {
        case region = "Région"
        case ville = "Ville"
        case rue = "Rue"
        var id: String { rawValue }
        var zooms: ClosedRange<Int> {
            switch self {
            case .region: return 8...12
            case .ville: return 8...14
            case .rue: return 8...16
            }
        }
    }

    private var startRegion: MKCoordinateRegion {
        MKCoordinateRegion(
            center: center ?? CLLocationCoordinate2D(latitude: 46.6, longitude: 2.4),
            latitudinalMeters: center == nil ? 900_000 : 12_000,
            longitudinalMeters: center == nil ? 900_000 : 12_000)
    }

    var body: some View {
        ZStack(alignment: .top) {
            if let style = vectorStyle {
                VectorMapView(
                    styleURL: style,
                    route: route,
                    initialCenter: startRegion.center,
                    onRegionChange: { bounds = $0 }
                )
                .ignoresSafeArea()
            } else if configured {
                OfflineMapView(
                    route: route,
                    initialRegion: startRegion,
                    online: true,
                    onRegionChange: { region = $0 }
                )
                .ignoresSafeArea()
            } else {
                setupHelp
            }
            VStack(spacing: 0) {
                header
                // La carte reste affichée malgré l'alerte : les zones déjà
                // enregistrées s'affichent sans réseau ni serveur.
                if let warning = styleStatus.message(hasStored: storedBytes > 0) {
                    Text(warning)
                        .font(.caption)
                        .multilineTextAlignment(.center)
                        .padding(10)
                        .frame(maxWidth: .infinity)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
                        .padding(.horizontal)
                }
            }
            VStack {
                Spacer()
                if vectorStyle != nil {
                    vectorPanel
                } else if configured {
                    panel
                }
            }
        }
        .task(id: styleURLString) {
            guard let style = vectorStyle else {
                styleStatus = .ok  // mode tuiles images : pas de style à vérifier
                return
            }
            styleStatus = .checking
            styleStatus = await StyleProbe.check(style)
        }
        .onAppear {
            region = startRegion
            route = RouteStore.load()?.coordinates ?? []
            storedBytes = vectorStyle != nil ? packs.storedBytes() : TileCache.sizeBytes()
            refreshEstimate()
        }
        .onChange(of: region.center.latitude) { _, _ in refreshEstimate() }
        .onChange(of: depth) { _, _ in refreshEstimate() }
        .onChange(of: downloader.running) { _, running in
            if !running { storedBytes = TileCache.sizeBytes() }
        }
        .onChange(of: packs.running) { _, running in
            if !running { storedBytes = packs.storedBytes() }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Button { dismiss() } label: {
                Image(systemName: "xmark")
                    .font(.headline)
                    .foregroundStyle(.primary)
                    .frame(width: 40, height: 40)
                    .background(.regularMaterial, in: Circle())
            }
            VStack(alignment: .leading, spacing: 1) {
                Text("Carte hors ligne").font(.headline)
                Text(
                    storedBytes > 0
                        ? "\(Self.megabytes(storedBytes)) enregistrés"
                        : "Aucune zone enregistrée"
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.regularMaterial, in: Capsule())
            Spacer()
        }
        .padding()
    }

    /// Panneau de téléchargement de la zone visible.
    private var panel: some View {
        VStack(spacing: 12) {
            Picker("Détail", selection: $depth) {
                ForEach(Depth.allCases) { d in Text(d.rawValue).tag(d) }
            }
            .pickerStyle(.segmented)

            if downloader.running {
                VStack(spacing: 6) {
                    ProgressView(value: downloader.progress)
                    HStack {
                        Text("\(downloader.done) / \(downloader.total) tuiles")
                            .font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        Button("Arrêter") { downloader.cancel() }
                            .font(.caption.weight(.semibold))
                    }
                }
            } else {
                Button {
                    downloader.start(region: region, zooms: depth.zooms)
                } label: {
                    Label(
                        estimate.count == 0
                            ? "Zone déjà enregistrée"
                            : "Télécharger cette zone · \(Self.megabytes(estimate.bytes))",
                        systemImage: "arrow.down.circle.fill"
                    )
                    .font(.subheadline.weight(.semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
                    .background(
                        (estimate.count == 0 ? Color.gray : Color.blue).opacity(0.18),
                        in: RoundedRectangle(cornerRadius: 14))
                }
                .disabled(estimate.count == 0)
                if estimate.count >= TileDownloader.maxTiles {
                    Text("Zone trop vaste : seules les premières tuiles seront enregistrées. Zoome davantage pour tout couvrir.")
                        .font(.caption2).foregroundStyle(.orange)
                }
                if downloader.failed > 0 {
                    Text("\(downloader.failed) tuile(s) non téléchargée(s).")
                        .font(.caption2).foregroundStyle(.secondary)
                }
            }

            HStack {
                Text("© OpenStreetMap contributors")
                    .font(.caption2).foregroundStyle(.secondary)
                Spacer()
                if storedBytes > 0 {
                    Button("Tout effacer", role: .destructive) { confirmClear = true }
                        .font(.caption.weight(.semibold))
                }
            }
        }
        .padding(14)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 20))
        .padding()
        .confirmationDialog(
            "Effacer toutes les cartes enregistrées ?", isPresented: $confirmClear,
            titleVisibility: .visible
        ) {
            Button("Tout effacer", role: .destructive) {
                TileCache.clear()
                storedBytes = 0
                refreshEstimate()
            }
            Button("Annuler", role: .cancel) {}
        }
    }

    /// Panneau de téléchargement en mode vectoriel : MapLibre gère lui-même le
    /// découpage en tuiles et leur conservation.
    private var vectorPanel: some View {
        VStack(spacing: 12) {
            Picker("Détail", selection: $depth) {
                ForEach(Depth.allCases) { d in Text(d.rawValue).tag(d) }
            }
            .pickerStyle(.segmented)

            if packs.running {
                VStack(spacing: 6) {
                    ProgressView(value: packs.progress)
                    Text("Téléchargement en cours…")
                        .font(.caption).foregroundStyle(.secondary)
                }
            } else {
                Button {
                    if let style = vectorStyle, let bounds {
                        packs.start(
                            styleURL: style, bounds: bounds,
                            maxZoom: Double(depth.zooms.upperBound))
                    }
                } label: {
                    Label("Télécharger cette zone", systemImage: "arrow.down.circle.fill")
                        .font(.subheadline.weight(.semibold))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                        .background(Color.blue.opacity(0.18), in: RoundedRectangle(cornerRadius: 14))
                }
                // Télécharger d'un serveur sans cartes ne produirait qu'un
                // pack vide, sans erreur visible.
                .disabled(bounds == nil || !styleStatus.canDownload)
                if let message = packs.message {
                    Text(message).font(.caption2).foregroundStyle(.orange)
                }
            }

            HStack {
                Text("© OpenStreetMap contributors")
                    .font(.caption2).foregroundStyle(.secondary)
                Spacer()
                Button("Tout effacer", role: .destructive) {
                    packs.clearAll()
                    storedBytes = 0
                }
                .font(.caption.weight(.semibold))
            }
        }
        .padding(14)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 20))
        .padding()
    }

    /// Écran d'explication tant qu'aucun serveur de tuiles n'est renseigné.
    private var setupHelp: some View {
        VStack(spacing: 16) {
            Image(systemName: "square.and.arrow.down.on.square")
                .font(.system(size: 44))
                .foregroundStyle(.blue)
            Text("Cartes hors ligne").font(.title3.weight(.bold))
            Text(
                """
                Apple Plans ne permet pas d'enregistrer ses cartes. Indique un \
                serveur de tuiles dans les réglages pour télécharger des zones \
                et les consulter sans réseau.
                """
            )
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            Text("Format attendu : https://exemple.fr/{z}/{x}/{y}.png")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
        }
        .padding(28)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemBackground))
    }

    private func refreshEstimate() {
        estimate = TileDownloader.estimate(region: region, zooms: depth.zooms)
    }

    /// « 24 Mo » — l'espace disque parle plus qu'un nombre de tuiles.
    private static func megabytes(_ bytes: Int) -> String {
        let mo = Double(bytes) / 1_000_000
        return mo < 10
            ? String(format: "%.1f Mo", mo) : String(format: "%.0f Mo", mo)
    }
}
