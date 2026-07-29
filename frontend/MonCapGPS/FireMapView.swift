import MapKit
import SwiftUI

/// Un contour de feu approximatif : polygone (enveloppe des points chauds d'un
/// même foyer) + son centre et l'heure de la dernière détection satellite.
struct FirePerimeter: Identifiable {
    let id: Int
    let polygon: [CLLocationCoordinate2D]
    let center: CLLocationCoordinate2D
    /// Détection satellite la plus récente du foyer (pour « Mise à jour il y a X h »).
    let updatedAt: Date?
    /// Première détection du foyer sur la fenêtre de 7 jours (« Créé il y a X j »).
    let createdAt: Date?
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
    /// Foyer dont l'infobulle est ouverte (tap sur le marqueur) — une seule à
    /// la fois, sinon les bulles se superposent en masse noire au dézoom.
    @State private var selectedID: Int?
    /// Noms des foyers (« Feu de Saumos ») par position arrondie, géocodés à la
    /// demande au tap ; "" = géocodage en cours (évite les requêtes doublons).
    @State private var names: [String: String] = [:]

    var body: some View {
        ZStack(alignment: .top) {
            Map(position: $camera) {
                ForEach(perimeters) { p in
                    // Zone touchée : brun brûlé translucide cerné de rouge vif,
                    // comme les contours de feuxdeforet.fr sur fond satellite.
                    MapPolygon(coordinates: p.polygon)
                        .foregroundStyle(Color(red: 0.48, green: 0.20, blue: 0.13).opacity(0.55))
                        .stroke(Color(red: 0.86, green: 0.13, blue: 0.11), lineWidth: 2)
                    Annotation("", coordinate: p.center) {
                        // Le marqueur reste centré sur le foyer ; l'infobulle
                        // sombre ne s'ouvre qu'au tap (une seule à la fois),
                        // sinon les bulles de tous les foyers se superposent.
                        FireMarker()
                            .overlay(alignment: .top) {
                                if selectedID == p.id {
                                    FireUpdateBubble(
                                        name: name(of: p),
                                        createdAt: p.createdAt,
                                        updatedAt: p.updatedAt ?? service.lastUpdate
                                    )
                                    .offset(y: 52)
                                }
                            }
                            .onTapGesture {
                                withAnimation(.spring(duration: 0.25)) {
                                    selectedID = selectedID == p.id ? nil : p.id
                                }
                                if selectedID == p.id { loadName(of: p) }
                            }
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
            // Les identifiants de foyers changent au recalcul : referme
            // l'infobulle plutôt que de la laisser sauter sur un autre foyer.
            selectedID = nil
        }
    }

    /// Clé de cache d'un foyer : position arrondie (~1 km), stable d'un
    /// recalcul à l'autre contrairement à l'identifiant de cluster.
    private func nameKey(_ c: CLLocationCoordinate2D) -> String {
        "\(Int(c.latitude * 100))|\(Int(c.longitude * 100))"
    }

    /// Nom géocodé du foyer (« Feu de Saumos »), s'il est déjà connu.
    private func name(of p: FirePerimeter) -> String? {
        names[nameKey(p.center)].flatMap { $0.isEmpty ? nil : $0 }
    }

    /// Géocodage inverse du centre du foyer, à la demande (un seul appel par
    /// foyer, au premier tap) : nomme le feu d'après sa commune, comme le site.
    private func loadName(of p: FirePerimeter) {
        let key = nameKey(p.center)
        guard names[key] == nil else { return }
        names[key] = ""  // requête en cours
        let center = p.center
        Task {
            let loc = CLLocation(latitude: center.latitude, longitude: center.longitude)
            let mark = try? await CLGeocoder().reverseGeocodeLocation(loc).first
            if let city = mark?.locality ?? mark?.subLocality ?? mark?.subAdministrativeArea {
                names[key] = "Feu de \(city)"
            }
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
                // Compteur en avant, comme le bandeau « 15 feux en cours » du site.
                Text(
                    perimeters.isEmpty
                        ? "Carte des feux en cours"
                        : "\(perimeters.count) feu\(perimeters.count > 1 ? "x" : "") en cours"
                )
                .font(.headline)
                Text(
                    perimeters.isEmpty
                        ? "Aucun foyer détecté"
                        : "contour satellite · touchez un foyer"
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

/// Infobulle sombre sous le marqueur, identique à celle de feuxdeforet.fr :
/// nom du feu (géocodé), « Contour satellite approximatif », puis « Créé il y
/// a X j · Mise à jour il y a X h » en italique.
struct FireUpdateBubble: View {
    let name: String?
    let createdAt: Date?
    let updatedAt: Date?

    var body: some View {
        VStack(spacing: 2) {
            if let name {
                Text(name)
                    .font(.footnote.weight(.bold))
                    .foregroundStyle(.white)
            }
            Text("Contour satellite approximatif")
                .font(name == nil ? .footnote.weight(.semibold) : .caption)
                .foregroundStyle(name == nil ? .white : .white.opacity(0.8))
            if let ago = agoText {
                Text(ago)
                    .font(.caption.italic())
                    .foregroundStyle(.white.opacity(0.65))
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(Color.black.opacity(0.72), in: RoundedRectangle(cornerRadius: 12))
        .fixedSize()
    }

    /// « Créé il y a 6 j · Mise à jour il y a 3 h » (chaque moitié omise si
    /// inconnue ; la création n'apparaît qu'à partir d'un jour).
    private var agoText: String? {
        var parts: [String] = []
        if let createdAt {
            let days = Int(Date().timeIntervalSince(createdAt) / 86_400)
            if days >= 1 { parts.append("Créé il y a \(days) j") }
        }
        if let updatedAt {
            let hours = Int(Date().timeIntervalSince(updatedAt) / 3600)
            parts.append(
                hours < 1 ? "Mise à jour il y a moins d'1 h" : "Mise à jour il y a \(hours) h")
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

/// Géométrie des contours de feu : regroupe les points chauds proches et calcule
/// l'enveloppe convexe de chaque groupe (le « contour approximatif »).
enum FireGeometry {
    /// Un foyer sans détection depuis ce délai est considéré éteint (l'API
    /// renvoie 7 jours d'historique pour dater les débuts de feux, mais seuls
    /// les foyers encore actifs sont affichés — « feux en cours »).
    private static let activeWindow: TimeInterval = 36 * 3600

    /// Construit un contour par foyer. On privilégie un **détourage concave**
    /// (épouse la forme réelle des pixels détectés) ; à défaut on retombe sur
    /// l'enveloppe convexe, et les points isolés donnent un petit cercle.
    static func perimeters(from fires: [Fire]) -> [FirePerimeter] {
        guard !fires.isEmpty else { return [] }
        let clusters = cluster(fires, thresholdMeters: 6000)
        var out: [FirePerimeter] = []
        for fireGroup in clusters {
            // Foyer « en cours » = au moins une détection récente. Sans aucun
            // horodatage (vieux backend), on affiche par prudence.
            let updatedAt = fireGroup.compactMap { $0.detectedAt }.max()
            if let updatedAt, Date().timeIntervalSince(updatedAt) > activeWindow { continue }
            let group = fireGroup.map { $0.coordinate }
            let center = centroid(group)
            let polygon: [CLLocationCoordinate2D]
            if group.count >= 4, let concave = concaveHull(group) {
                // Contour détouré : suit les concavités du foyer, sans englober
                // les zones intactes. Léger arrondi + petit débord (~1 pixel
                // VIIRS de 375 m) pour couvrir le pourtour réel du feu.
                polygon = smooth(bufferedRing(concave, factor: 1.06), iterations: 1)
            } else if group.count >= 3 {
                polygon = smooth(bufferedRing(convexHull(group), factor: 1.25), iterations: 2)
            } else {
                polygon = circle(center, radiusMeters: 3500)
            }
            out.append(
                FirePerimeter(
                    id: out.count, polygon: polygon, center: center,
                    updatedAt: updatedAt,
                    createdAt: fireGroup.compactMap { $0.firstDetectedAt }.min()))
        }
        return out
    }

    /// Regroupe les feux : un point rejoint un groupe s'il est à moins de
    /// `thresholdMeters` de l'un de ses membres (agrégation simple). Distance
    /// équirectangulaire (planaire) : aux échelles de quelques km l'écart avec
    /// la distance géodésique est négligeable, et c'est ~100× plus rapide que
    /// `CLLocation.distance` — nécessaire avec 7 jours de détections.
    private static func cluster(_ fires: [Fire], thresholdMeters: Double) -> [[Fire]] {
        let metersPerDegree = 111_320.0
        let thresholdSq = thresholdMeters * thresholdMeters
        var groups: [[Fire]] = []
        for f in fires {
            let p = f.coordinate
            let cosLat = cos(p.latitude * .pi / 180)
            if let idx = groups.firstIndex(where: { group in
                group.contains { g in
                    let q = g.coordinate
                    let dx = (q.longitude - p.longitude) * metersPerDegree * cosLat
                    let dy = (q.latitude - p.latitude) * metersPerDegree
                    return dx * dx + dy * dy < thresholdSq
                }
            }) {
                groups[idx].append(f)
            } else {
                groups.append([f])
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
    /// « plein » (comme une zone touchée). `factor` proche de 1 = quasi collé
    /// aux points (détourage précis).
    private static func bufferedRing(_ ring: [CLLocationCoordinate2D], factor: Double)
        -> [CLLocationCoordinate2D]
    {
        let c = centroid(ring)
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

    // MARK: - Détourage concave (k plus proches voisins)

    /// Enveloppe **concave** (« détourage ») par k plus proches voisins
    /// (Moreira & Santos, 2007) : contrairement à l'enveloppe convexe, elle
    /// épouse les concavités du nuage de points. Essaie des k croissants ;
    /// renvoie `nil` si aucun ne donne un contour valide (l'appelant retombe
    /// alors proprement sur l'enveloppe convexe).
    private static func concaveHull(_ coords: [CLLocationCoordinate2D]) -> [CLLocationCoordinate2D]? {
        let uniq = Array(Set(coords.map { Pt($0.longitude, $0.latitude) }))
        // Sous-échantillonne les très gros foyers (grille) pour garder un calcul
        // rapide sur le thread de fond, tout en préservant la forme.
        let pts = downsample(uniq, target: 256)
        guard pts.count >= 4 else { return nil }
        var k = 3
        while k <= min(pts.count - 1, 25) {
            if let hull = concaveHullK(pts, k: k), hull.count >= 3 {
                return hull.map { CLLocationCoordinate2D(latitude: $0.y, longitude: $0.x) }
            }
            k += 1
        }
        return nil
    }

    /// Réduit un nuage à au plus ~`target` points en n'en gardant qu'un par
    /// cellule d'une grille (couverture spatiale préservée, doublons éliminés).
    private static func downsample(_ pts: [Pt], target: Int) -> [Pt] {
        guard pts.count > target else { return pts }
        let xs = pts.map { $0.x }
        let ys = pts.map { $0.y }
        let minX = xs.min()!, maxX = xs.max()!, minY = ys.min()!, maxY = ys.max()!
        let grid = max(1, Int(Double(target).squareRoot()))
        let w = max(maxX - minX, 1e-9)
        let h = max(maxY - minY, 1e-9)
        var seen = Set<Int>()
        var out: [Pt] = []
        for p in pts {
            let cx = min(grid - 1, Int((p.x - minX) / w * Double(grid)))
            let cy = min(grid - 1, Int((p.y - minY) / h * Double(grid)))
            if seen.insert(cy * grid + cx).inserted { out.append(p) }
        }
        return out
    }

    /// Une passe de l'algorithme pour un k donné. `nil` si le contour se
    /// recroise ou n'englobe pas tous les points (→ réessai avec k+1).
    private static func concaveHullK(_ points: [Pt], k: Int) -> [Pt]? {
        let n = points.count
        let kk = min(max(k, 3), n - 1)
        var dataset = points
        let first = dataset.min(by: { $0.y < $1.y || ($0.y == $1.y && $0.x < $1.x) })!
        var hull: [Pt] = [first]
        var current = first
        dataset.removeAll { $0 == first }
        var prevAngle = 0.0
        var step = 2
        var iterations = 0
        let maxIter = n * 10

        while (current != first || step == 2) && !dataset.isEmpty {
            iterations += 1
            if iterations > maxIter { return nil }
            if step == 5 { dataset.append(first) }
            let candidates = kNearest(dataset, to: current, k: kk).sorted {
                clockwiseTurn(prevAngle, current, $0) > clockwiseTurn(prevAngle, current, $1)
            }
            var chosen: Pt? = nil
            for cand in candidates {
                // La nouvelle arête (current → cand) ne doit croiser aucune
                // arête déjà tracée (les arêtes adjacentes partagent un sommet
                // et sont ignorées par le test de croisement).
                var intersects = false
                var i = 1
                while i <= hull.count - 2 {
                    if segmentsIntersect(current, cand, hull[i - 1], hull[i]) {
                        intersects = true
                        break
                    }
                    i += 1
                }
                if !intersects {
                    chosen = cand
                    break
                }
            }
            guard let next = chosen else { return nil }
            current = next
            hull.append(next)
            prevAngle = atan2(
                hull[hull.count - 1].y - hull[hull.count - 2].y,
                hull[hull.count - 1].x - hull[hull.count - 2].x)
            dataset.removeAll { $0 == next }
            step += 1
        }

        // Tous les points doivent être à l'intérieur du contour, sinon k trop
        // petit → on rejette pour réessayer plus large.
        for p in points where !hull.contains(p) {
            if !pointInside(p, hull) { return nil }
        }
        return hull
    }

    /// Les `k` points les plus proches de `p` (distance planaire approchée).
    private static func kNearest(_ pts: [Pt], to p: Pt, k: Int) -> [Pt] {
        pts.sorted {
            let d0 = ($0.x - p.x) * ($0.x - p.x) + ($0.y - p.y) * ($0.y - p.y)
            let d1 = ($1.x - p.x) * ($1.x - p.x) + ($1.y - p.y) * ($1.y - p.y)
            return d0 < d1
        }
        .prefix(k).map { $0 }
    }

    /// Angle de rotation horaire entre la direction précédente et `from → to`
    /// (0…2π) : on choisit le virage le plus « à droite » pour longer le bord.
    private static func clockwiseTurn(_ prevAngle: Double, _ from: Pt, _ to: Pt) -> Double {
        let a = atan2(to.y - from.y, to.x - from.x)
        let twoPi = 2 * Double.pi
        var d = (prevAngle - a).truncatingRemainder(dividingBy: twoPi)
        if d < 0 { d += twoPi }
        return d
    }

    /// Deux segments se croisent-ils franchement (hors extrémités partagées) ?
    private static func segmentsIntersect(_ p1: Pt, _ p2: Pt, _ p3: Pt, _ p4: Pt) -> Bool {
        func cross(_ a: Pt, _ b: Pt, _ c: Pt) -> Double {
            (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
        }
        let d1 = cross(p3, p4, p1)
        let d2 = cross(p3, p4, p2)
        let d3 = cross(p1, p2, p3)
        let d4 = cross(p1, p2, p4)
        return ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0))
            && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0))
    }

    /// Le point est-il à l'intérieur du polygone (lancer de rayon) ?
    private static func pointInside(_ p: Pt, _ poly: [Pt]) -> Bool {
        var inside = false
        var j = poly.count - 1
        for i in 0..<poly.count {
            let a = poly[i]
            let b = poly[j]
            if (a.y > p.y) != (b.y > p.y) {
                let xCross = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x)
                if p.x < xCross { inside.toggle() }
            }
            j = i
        }
        return inside
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
