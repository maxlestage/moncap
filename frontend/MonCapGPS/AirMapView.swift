import MapKit
import SwiftUI
import UIKit

/// Overlay image d'une carte de chaleur de qualité de l'air (champ continu).
final class AQIImageOverlay: NSObject, MKOverlay {
    let image: UIImage
    let rect: MKMapRect
    init(image: UIImage, rect: MKMapRect) {
        self.image = image
        self.rect = rect
    }
    var boundingMapRect: MKMapRect { rect }
    var coordinate: CLLocationCoordinate2D {
        MKMapPoint(x: rect.midX, y: rect.midY).coordinate
    }
}

/// Dessine l'image de chaleur dans son rectangle géographique.
final class AQIImageRenderer: MKOverlayRenderer {
    private let image: UIImage
    init(_ overlay: AQIImageOverlay) {
        self.image = overlay.image
        super.init(overlay: overlay)
    }
    override func draw(_ mapRect: MKMapRect, zoomScale: MKZoomScale, in context: CGContext) {
        let drawRect = rect(for: overlay.boundingMapRect)
        UIGraphicsPushContext(context)
        image.draw(in: drawRect)
        UIGraphicsPopContext()
    }
}

/// Carte de chaleur qualité de l'air **maison** : on échantillonne une grille de
/// valeurs EAQI (données CAMS via open-meteo, sans clé), on l'interpole en une
/// image lisse (dégradé continu) et on la pose en overlay sur un MKMapView →
/// rendu fondu comme les cartes dédiées, et rapide (rendu natif). Écran séparé
/// pour ne pas alourdir la carte GPS.
struct AirMapRepresentable: UIViewRepresentable {
    let center: CLLocationCoordinate2D?

    func makeUIView(context: Context) -> MKMapView {
        let map = MKMapView()
        map.delegate = context.coordinator
        map.showsUserLocation = true
        map.pointOfInterestFilter = .excludingAll
        if let c = center {
            map.setRegion(
                MKCoordinateRegion(
                    center: c, latitudinalMeters: 600_000, longitudinalMeters: 600_000),
                animated: false)
        }
        context.coordinator.refresh(map)
        return map
    }

    func updateUIView(_ map: MKMapView, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator: NSObject, MKMapViewDelegate {
        private var overlay: AQIImageOverlay?
        private var lastKey = ""
        private var lastFetch = Date.distantPast
        private var busy = false

        func mapView(_ mapView: MKMapView, regionDidChangeAnimated animated: Bool) {
            refresh(mapView)
        }

        func mapView(_ mapView: MKMapView, rendererFor overlay: MKOverlay) -> MKOverlayRenderer {
            if let o = overlay as? AQIImageOverlay { return AQIImageRenderer(o) }
            return MKOverlayRenderer(overlay: overlay)
        }

        /// Recalcule la carte de chaleur pour la zone visible (si elle a changé).
        func refresh(_ mapView: MKMapView) {
            // Affiche instantanément la dernière image connue (préchargée au
            // lancement ou d'une ouverture précédente), puis on actualise.
            if overlay == nil, let cached = AirHeat.lastOverlay {
                let ov = AQIImageOverlay(image: cached.image, rect: cached.rect)
                overlay = ov
                mapView.addOverlay(ov, level: .aboveRoads)
            }
            let region = mapView.region
            let key = String(
                format: "%.1f,%.1f,%.1f,%.1f",
                region.center.latitude, region.center.longitude,
                region.span.latitudeDelta, region.span.longitudeDelta)
            guard !busy, key != lastKey || Date().timeIntervalSince(lastFetch) > 300 else { return }
            lastKey = key
            lastFetch = Date()
            busy = true

            // BEAUCOUP plus large que l'écran (×2,6) : ça constitue une réserve
            // tout autour, donc les déplacements restent dans du déjà-affiché →
            // mise à jour perçue instantanée (l'ancienne image reste visible
            // pendant le rechargement, pas de flash). Borné quand on est dézoomé.
            let latSpan = min(region.span.latitudeDelta * 2.6, 30.0)
            let lonSpan = min(region.span.longitudeDelta * 2.6, 34.0)
            let north = region.center.latitude + latSpan / 2
            let south = region.center.latitude - latSpan / 2
            let west = region.center.longitude - lonSpan / 2
            let east = region.center.longitude + lonSpan / 2
            let n = 16

            var coords: [CLLocationCoordinate2D] = []
            coords.reserveCapacity(n * n)
            for r in 0..<n {
                let lat = north - (north - south) * Double(r) / Double(n - 1)
                for c in 0..<n {
                    let lon = west + (east - west) * Double(c) / Double(n - 1)
                    coords.append(CLLocationCoordinate2D(latitude: lat, longitude: lon))
                }
            }

            let tl = MKMapPoint(CLLocationCoordinate2D(latitude: north, longitude: west))
            let br = MKMapPoint(CLLocationCoordinate2D(latitude: south, longitude: east))
            let rect = MKMapRect(x: tl.x, y: tl.y, width: br.x - tl.x, height: br.y - tl.y)

            Task { [weak self, weak mapView] in
                let grid = await AirHeat.fetchGrid(coords: coords, n: n)
                guard let self, let mapView, let grid,
                    let image = AirHeat.image(grid: grid, n: n)
                else { await MainActor.run { self?.busy = false }; return }
                await MainActor.run {
                    if let old = self.overlay { mapView.removeOverlay(old) }
                    let ov = AQIImageOverlay(image: image, rect: rect)
                    self.overlay = ov
                    mapView.addOverlay(ov, level: .aboveRoads)
                    AirHeat.lastOverlay = (image, rect)  // cache pour un affichage instantané
                    self.busy = false
                }
            }
        }
    }
}

/// Génération de la carte de chaleur : requête open-meteo groupée + interpolation
/// bilinéaire vers une image lissée.
enum AirHeat {
    /// Dernière image générée (+ son rectangle), pour un affichage instantané à
    /// l'ouverture. Touché uniquement sur le thread principal.
    static var lastOverlay: (image: UIImage, rect: MKMapRect)?

    /// Précharge la carte de chaleur autour d'un point (au lancement), pour que
    /// l'écran air s'affiche instantanément quand on l'ouvre.
    nonisolated static func prefetch(center: CLLocationCoordinate2D) async {
        let latSpan = 8.0
        let lonSpan = 10.0
        let north = center.latitude + latSpan / 2
        let south = center.latitude - latSpan / 2
        let west = center.longitude - lonSpan / 2
        let east = center.longitude + lonSpan / 2
        let n = 16
        var coords: [CLLocationCoordinate2D] = []
        coords.reserveCapacity(n * n)
        for r in 0..<n {
            let lat = north - (north - south) * Double(r) / Double(n - 1)
            for c in 0..<n {
                let lon = west + (east - west) * Double(c) / Double(n - 1)
                coords.append(CLLocationCoordinate2D(latitude: lat, longitude: lon))
            }
        }
        guard let grid = await fetchGrid(coords: coords, n: n),
            let img = image(grid: grid, n: n)
        else { return }
        let tl = MKMapPoint(CLLocationCoordinate2D(latitude: north, longitude: west))
        let br = MKMapPoint(CLLocationCoordinate2D(latitude: south, longitude: east))
        let rect = MKMapRect(x: tl.x, y: tl.y, width: br.x - tl.x, height: br.y - tl.y)
        await MainActor.run { lastOverlay = (img, rect) }
    }

    /// Échantillonne la grille EAQI (une seule requête open-meteo). `nil` si échec.
    nonisolated static func fetchGrid(coords: [CLLocationCoordinate2D], n: Int) async -> [Double]? {
        let lats = coords.map { String(format: "%.3f", $0.latitude) }.joined(separator: ",")
        let lons = coords.map { String(format: "%.3f", $0.longitude) }.joined(separator: ",")
        var comps = URLComponents(string: "https://air-quality-api.open-meteo.com/v1/air-quality")!
        comps.queryItems = [
            URLQueryItem(name: "latitude", value: lats),
            URLQueryItem(name: "longitude", value: lons),
            URLQueryItem(name: "current", value: "european_aqi"),
        ]
        guard let url = comps.url,
            let (data, _) = try? await URLSession.shared.data(from: url)
        else { return nil }

        let objs: [[String: Any]]
        if let arr = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] {
            objs = arr
        } else if let one = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            objs = [one]
        } else {
            return nil
        }
        guard objs.count == n * n else { return nil }

        var grid = [Double](repeating: 0, count: n * n)
        for (i, obj) in objs.enumerated() {
            let cur = obj["current"] as? [String: Any]
            let v =
                (cur?["european_aqi"] as? Double) ?? (cur?["european_aqi"] as? Int).map(Double.init)
                ?? 0
            grid[i] = v
        }
        return grid
    }

    /// Construit une image RGBA lissée (256×256) par interpolation bilinéaire de
    /// la grille, coloriée selon une rampe EAQI continue. `shouldInterpolate`
    /// laisse ensuite MapKit lisser encore à l'affichage.
    nonisolated static func image(grid: [Double], n: Int) -> UIImage? {
        let w = 256
        let h = 256
        let bytesPerRow = w * 4
        var data = [UInt8](repeating: 0, count: h * bytesPerRow)
        let alpha: UInt8 = 150
        for py in 0..<h {
            let gy = Double(py) / Double(h - 1) * Double(n - 1)
            let r0 = min(Int(gy), n - 2)
            let fy = gy - Double(r0)
            for px in 0..<w {
                let gx = Double(px) / Double(w - 1) * Double(n - 1)
                let c0 = min(Int(gx), n - 2)
                let fx = gx - Double(c0)
                let v00 = grid[r0 * n + c0]
                let v01 = grid[r0 * n + c0 + 1]
                let v10 = grid[(r0 + 1) * n + c0]
                let v11 = grid[(r0 + 1) * n + c0 + 1]
                let top = v00 * (1 - fx) + v01 * fx
                let bot = v10 * (1 - fx) + v11 * fx
                let v = top * (1 - fy) + bot * fy
                let (r, g, b) = color(v)
                let i = py * bytesPerRow + px * 4
                data[i] = r
                data[i + 1] = g
                data[i + 2] = b
                data[i + 3] = alpha
            }
        }
        guard let provider = CGDataProvider(data: Data(data) as CFData) else { return nil }
        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue)
        guard
            let cg = CGImage(
                width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: bytesPerRow, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: bitmapInfo, provider: provider, decode: nil,
                shouldInterpolate: true, intent: .defaultIntent)
        else { return nil }
        return UIImage(cgImage: cg)
    }

    /// Rampe de couleur EAQI continue (0 = bon → 100+ = extrême).
    private static func color(_ value: Double) -> (UInt8, UInt8, UInt8) {
        let stops: [(Double, Double, Double, Double)] = [
            (0, 0.30, 0.68, 0.30),  // Bon — vert
            (20, 0.60, 0.80, 0.25),  // Correct
            (40, 0.96, 0.86, 0.20),  // Moyen — jaune
            (60, 0.96, 0.55, 0.15),  // Mauvais — orange
            (80, 0.90, 0.20, 0.20),  // Très mauvais — rouge
            (100, 0.55, 0.15, 0.55),  // Extrême — violet
        ]
        let x = max(0, min(100, value))
        for i in 0..<(stops.count - 1) {
            let a = stops[i]
            let b = stops[i + 1]
            if x <= b.0 {
                let t = (b.0 - a.0) == 0 ? 0 : (x - a.0) / (b.0 - a.0)
                return (
                    UInt8((a.1 + (b.1 - a.1) * t) * 255),
                    UInt8((a.2 + (b.2 - a.2) * t) * 255),
                    UInt8((a.3 + (b.3 - a.3) * t) * 255)
                )
            }
        }
        let last = stops[stops.count - 1]
        return (UInt8(last.1 * 255), UInt8(last.2 * 255), UInt8(last.3 * 255))
    }
}

/// Écran plein de la carte qualité de l'air (carte de chaleur maison) + légende.
struct AirQualityMapScreen: View {
    @Environment(\.dismiss) private var dismiss
    let center: CLLocationCoordinate2D?

    private static let legend: [(String, Color)] = [
        ("Bon", Color(red: 0.30, green: 0.68, blue: 0.30)),
        ("Correct", Color(red: 0.60, green: 0.80, blue: 0.25)),
        ("Moyen", Color(red: 0.96, green: 0.86, blue: 0.20)),
        ("Mauvais", Color(red: 0.96, green: 0.55, blue: 0.15)),
        ("Très mauvais", Color(red: 0.90, green: 0.20, blue: 0.20)),
        ("Extrême", Color(red: 0.55, green: 0.15, blue: 0.55)),
    ]

    var body: some View {
        ZStack(alignment: .top) {
            AirMapRepresentable(center: center)
                .ignoresSafeArea()
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
                legendCard
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
}
