import MapKit
import MapLibre
import SwiftUI

/// Écran « Carte hors ligne » : consultation des tuiles enregistrées et
/// téléchargement de la zone visible.
struct OfflineMapScreen: View {
    @Environment(\.dismiss) private var dismiss
    /// Position d'ouverture de la carte.
    let center: CLLocationCoordinate2D?

    /// Style vectoriel : renseigné, la carte est rendue par MapLibre plutôt
    /// que par des tuiles images.
    @AppStorage(TileSource.styleKey) private var styleURLString = ""
    @StateObject private var packs = VectorPackDownloader()
    @State private var bounds: MLNCoordinateBounds?
    /// Le style est-il réellement servi ? Un serveur sans cartes rendrait une
    /// carte grise sans le moindre message.
    @State private var styleStatus = StyleStatus.checking

    /// Fond réellement affiché : celui de l'utilisateur s'il répond, sinon le
    /// nôtre. Vérifié à l'ouverture, pour ne jamais rester devant un écran vide.
    @State private var style = URL(string: TileSource.defaultStyle)!
    @State private var storedBytes = 0
    @State private var depth = Depth.ville
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
            VectorMapView(
                styleURL: style,
                route: route,
                initialCenter: startRegion.center,
                onRegionChange: { bounds = $0 }
            )
            .ignoresSafeArea()
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
                vectorPanel
            }
        }
        .task(id: styleURLString) {
            styleStatus = .checking
            style = await TileSource.resolvedStyle(custom: styleURLString)
            // Le fond retenu répond, sauf absence de réseau : c'est le seul
            // cas qui mérite encore un message.
            styleStatus = await StyleProbe.check(style)
        }
        .onAppear {
            route = RouteStore.load()?.coordinates ?? []
            storedBytes = packs.storedBytes()
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
                    if let bounds {
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

    /// « 24 Mo » — l'espace disque parle plus qu'un nombre de tuiles.
    private static func megabytes(_ bytes: Int) -> String {
        let mo = Double(bytes) / 1_000_000
        return mo < 10
            ? String(format: "%.1f Mo", mo) : String(format: "%.0f Mo", mo)
    }
}
