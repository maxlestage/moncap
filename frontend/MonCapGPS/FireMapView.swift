import MapKit
import SwiftUI

/// Un contour de feu approximatif : polygone (enveloppe des points chauds d'un
/// même foyer) + son centre.
struct FirePerimeter: Identifiable {
    let id: Int
    let polygon: [CLLocationCoordinate2D]
    let center: CLLocationCoordinate2D
}

/// Écran dédié « Carte des feux » à la manière de feuxdeforet.fr : fond
/// satellite, contours de feu en rouge translucide et marqueurs flamme. Séparé
/// de la carte GPS pour ne pas l'alourdir. Les contours sont approximés à partir
/// des points chauds satellite (NASA FIRMS) regroupés par foyer.
struct FireMapScreen: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var service: FireService

    @State private var perimeters: [FirePerimeter] = []
    @State private var camera: MapCameraPosition = .automatic

    var body: some View {
        ZStack(alignment: .top) {
            Map(position: $camera) {
                ForEach(perimeters) { p in
                    MapPolygon(coordinates: p.polygon)
                        .foregroundStyle(.red.opacity(0.35))
                        .stroke(.red.opacity(0.9), lineWidth: 2)
                    Annotation("Contour satellite approximatif", coordinate: p.center) {
                        FireMarker()
                    }
                }
            }
            .mapStyle(.hybrid(elevation: .flat))
            .ignoresSafeArea()

            header
        }
        // Recalcule les contours dès que la liste des feux change (arrivée des
        // données), hors du thread principal.
        .task(id: service.fires.count) {
            let fires = service.fires
            perimeters = await Task.detached { FireGeometry.perimeters(from: fires) }.value
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.headline)
                    .foregroundStyle(.primary)
                    .frame(width: 40, height: 40)
                    .background(.regularMaterial, in: Circle())
            }
            VStack(alignment: .leading, spacing: 1) {
                Text("Feux de forêt").font(.headline)
                Text(
                    perimeters.isEmpty
                        ? "Aucun foyer détecté"
                        : "\(perimeters.count) foyer(s) · contour satellite approximatif"
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
}

/// Géométrie des contours de feu : regroupe les points chauds proches et calcule
/// l'enveloppe convexe de chaque groupe (le « contour approximatif »).
enum FireGeometry {
    /// Construit un contour par foyer. Points isolés → petit cercle.
    static func perimeters(from fires: [Fire]) -> [FirePerimeter] {
        let points = fires.map { $0.coordinate }
        guard !points.isEmpty else { return [] }
        let clusters = cluster(points, thresholdMeters: 6000)
        var out: [FirePerimeter] = []
        for (i, group) in clusters.enumerated() {
            let center = centroid(group)
            let hull = convexHull(group)
            let polygon =
                hull.count >= 3
                ? smooth(bufferedRing(hull), iterations: 2)
                : circle(center, radiusMeters: 3500)
            out.append(FirePerimeter(id: i, polygon: polygon, center: center))
        }
        return out
    }

    /// Regroupe les points : un point rejoint un groupe s'il est à moins de
    /// `thresholdMeters` de l'un de ses membres (agrégation simple).
    private static func cluster(_ points: [CLLocationCoordinate2D], thresholdMeters: Double)
        -> [[CLLocationCoordinate2D]]
    {
        var groups: [[CLLocationCoordinate2D]] = []
        for p in points {
            let loc = CLLocation(latitude: p.latitude, longitude: p.longitude)
            if let idx = groups.firstIndex(where: { group in
                group.contains { q in
                    loc.distance(from: CLLocation(latitude: q.latitude, longitude: q.longitude))
                        < thresholdMeters
                }
            }) {
                groups[idx].append(p)
            } else {
                groups.append([p])
            }
        }
        return groups
    }

    private static func centroid(_ pts: [CLLocationCoordinate2D]) -> CLLocationCoordinate2D {
        let n = Double(pts.count)
        let lat = pts.reduce(0) { $0 + $1.latitude } / n
        let lon = pts.reduce(0) { $0 + $1.longitude } / n
        return CLLocationCoordinate2D(latitude: lat, longitude: lon)
    }

    /// Enveloppe convexe (chaîne monotone d'Andrew), lon = x, lat = y.
    private static func convexHull(_ pts: [CLLocationCoordinate2D]) -> [CLLocationCoordinate2D] {
        let p = Array(Set(pts.map { Pt($0.longitude, $0.latitude) })).sorted()
        guard p.count >= 3 else { return p.map { CLLocationCoordinate2D(latitude: $0.y, longitude: $0.x) } }
        func cross(_ o: Pt, _ a: Pt, _ b: Pt) -> Double {
            (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
        }
        var lower: [Pt] = []
        for pt in p {
            while lower.count >= 2, cross(lower[lower.count - 2], lower[lower.count - 1], pt) <= 0 {
                lower.removeLast()
            }
            lower.append(pt)
        }
        var upper: [Pt] = []
        for pt in p.reversed() {
            while upper.count >= 2, cross(upper[upper.count - 2], upper[upper.count - 1], pt) <= 0 {
                upper.removeLast()
            }
            upper.append(pt)
        }
        let hull = lower.dropLast() + upper.dropLast()
        return hull.map { CLLocationCoordinate2D(latitude: $0.y, longitude: $0.x) }
    }

    /// Élargit légèrement l'enveloppe autour de son centre pour un contour plus
    /// « plein » (comme une zone touchée).
    private static func bufferedRing(_ ring: [CLLocationCoordinate2D]) -> [CLLocationCoordinate2D] {
        let c = centroid(ring)
        let factor = 1.25
        return ring.map {
            CLLocationCoordinate2D(
                latitude: c.latitude + ($0.latitude - c.latitude) * factor,
                longitude: c.longitude + ($0.longitude - c.longitude) * factor)
        }
    }

    /// Lissage par coupe-coins de Chaikin : arrondit un polygone fermé
    /// (contour anguleux → organique). Chaque itération double le nombre de
    /// points en rapprochant chaque segment de son milieu.
    private static func smooth(_ ring: [CLLocationCoordinate2D], iterations: Int)
        -> [CLLocationCoordinate2D]
    {
        guard ring.count >= 3 else { return ring }
        var pts = ring
        for _ in 0..<iterations {
            var next: [CLLocationCoordinate2D] = []
            next.reserveCapacity(pts.count * 2)
            for i in 0..<pts.count {
                let a = pts[i]
                let b = pts[(i + 1) % pts.count]
                next.append(
                    CLLocationCoordinate2D(
                        latitude: a.latitude * 0.75 + b.latitude * 0.25,
                        longitude: a.longitude * 0.75 + b.longitude * 0.25))
                next.append(
                    CLLocationCoordinate2D(
                        latitude: a.latitude * 0.25 + b.latitude * 0.75,
                        longitude: a.longitude * 0.25 + b.longitude * 0.75))
            }
            pts = next
        }
        return pts
    }

    /// Polygone circulaire (~24 points) autour d'un centre, rayon en mètres.
    private static func circle(_ c: CLLocationCoordinate2D, radiusMeters: Double)
        -> [CLLocationCoordinate2D]
    {
        let dLat = radiusMeters / 111_320.0
        let dLon = radiusMeters / (111_320.0 * cos(c.latitude * .pi / 180))
        return (0..<24).map { i in
            let a = Double(i) / 24.0 * 2 * .pi
            return CLLocationCoordinate2D(
                latitude: c.latitude + dLat * sin(a),
                longitude: c.longitude + dLon * cos(a))
        }
    }

    /// Point 2D avec ordre lexicographique (pour l'enveloppe convexe).
    private struct Pt: Hashable, Comparable {
        let x: Double
        let y: Double
        init(_ x: Double, _ y: Double) {
            self.x = x
            self.y = y
        }
        static func < (a: Pt, b: Pt) -> Bool { a.x < b.x || (a.x == b.x && a.y < b.y) }
    }
}
