import MapKit
import SwiftUI

/// Carte de qualité de l'air en **vraies tuiles raster** (WAQI / aqicn.org),
/// affichées nativement par MapKit → rendu lisse et fluide, comme les apps
/// dédiées. Vit dans son propre écran pour ne pas alourdir la carte GPS
/// principale (qui reste une SwiftUI Map). Nécessite un token WAQI gratuit.
struct AirMapRepresentable: UIViewRepresentable {
    let token: String
    let center: CLLocationCoordinate2D?

    func makeUIView(context: Context) -> MKMapView {
        let map = MKMapView()
        map.delegate = context.coordinator
        map.showsUserLocation = true
        map.pointOfInterestFilter = .excludingAll
        if let c = center {
            map.setRegion(
                MKCoordinateRegion(
                    center: c, latitudinalMeters: 500_000, longitudinalMeters: 500_000),
                animated: false)
        }
        context.coordinator.install(token: token, on: map)
        return map
    }

    func updateUIView(_ map: MKMapView, context: Context) {
        // Recharge la couche si le token change.
        if context.coordinator.token != token {
            context.coordinator.install(token: token, on: map)
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator: NSObject, MKMapViewDelegate {
        private(set) var token = ""
        private var overlay: MKTileOverlay?

        /// (Ré)installe la couche de tuiles WAQI par-dessus la carte Apple.
        func install(token: String, on map: MKMapView) {
            self.token = token
            if let old = overlay { map.removeOverlay(old) }
            let trimmed = token.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else {
                overlay = nil
                return
            }
            let template =
                "https://tiles.waqi.info/tiles/usepa-aqi/{z}/{x}/{y}.png?token=\(trimmed)"
            let tiles = MKTileOverlay(urlTemplate: template)
            tiles.canReplaceMapContent = false  // dessine PAR-DESSUS la carte Apple
            map.addOverlay(tiles, level: .aboveLabels)
            overlay = tiles
        }

        func mapView(_ mapView: MKMapView, rendererFor overlay: MKOverlay) -> MKOverlayRenderer {
            if let tile = overlay as? MKTileOverlay {
                let renderer = MKTileOverlayRenderer(tileOverlay: tile)
                renderer.alpha = 0.85  // bien visible tout en laissant deviner la carte
                return renderer
            }
            return MKOverlayRenderer(overlay: overlay)
        }
    }
}

/// Écran plein de la carte qualité de l'air (tuiles WAQI) + légende + fermeture.
struct AirQualityMapScreen: View {
    @Environment(\.dismiss) private var dismiss
    let token: String
    let center: CLLocationCoordinate2D?

    /// Bandes de l'indice US AQI (échelle des tuiles WAQI).
    private static let legend: [(String, Color)] = [
        ("Bon", .green),
        ("Modéré", .yellow),
        ("Sensibles", .orange),
        ("Mauvais", .red),
        ("Très mauvais", .purple),
        ("Dangereux", Color(red: 0.5, green: 0.0, blue: 0.13)),
    ]

    var body: some View {
        ZStack(alignment: .top) {
            if token.trimmingCharacters(in: .whitespaces).isEmpty {
                missingToken
            } else {
                AirMapRepresentable(token: token, center: center)
                    .ignoresSafeArea()
            }

            HStack(alignment: .top) {
                Button {
                    dismiss()
                } label: {
                    Image(systemName: "xmark")
                        .font(.headline)
                        .foregroundStyle(.primary)
                        .frame(width: 40, height: 40)
                        .background(.regularMaterial, in: Circle())
                }
                Spacer()
                if !token.trimmingCharacters(in: .whitespaces).isEmpty {
                    legendCard
                }
            }
            .padding()
        }
    }

    private var legendCard: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Qualité de l'air").font(.caption.weight(.bold))
            ForEach(Self.legend, id: \.0) { label, color in
                HStack(spacing: 6) {
                    RoundedRectangle(cornerRadius: 2).fill(color).frame(width: 14, height: 10)
                    Text(label).font(.caption2)
                }
            }
        }
        .padding(10)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    private var missingToken: some View {
        VStack(spacing: 14) {
            Image(systemName: "aqi.medium").font(.system(size: 44)).foregroundStyle(.blue)
            Text("Carte qualité de l'air").font(.title3.weight(.bold))
            Text(
                "Ajoute ton token WAQI gratuit dans Réglages → « Qualité de l'air » pour afficher la carte de chaleur en temps réel."
            )
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            Link(
                "Obtenir un token gratuit",
                destination: URL(string: "https://aqicn.org/data-platform/token/")!
            )
            .font(.subheadline.weight(.semibold))
        }
        .padding(32)
    }
}
