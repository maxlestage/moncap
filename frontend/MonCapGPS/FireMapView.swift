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

/// Géométrie des contours de feu.
///
/// Le contour d'un foyer est l'**union des empreintes de ses détections**
/// satellite : chaque point chaud VIIRS (pixel de 375 m) est élargi en un
/// disque, et l'on trace la frontière de leur réunion. On obtient une forme
/// organique continue qui épouse la zone brûlée — comme les contours de
/// feuxdeforet.fr — au lieu des enveloppes anguleuses (triangles, cercles
/// isolés) que donnait le calcul d'enveloppe convexe/concave sur des points
/// épars.
///
/// Mise en œuvre : champ de distance sur une grille locale, extraction de
/// l'iso-ligne par *marching squares* orienté (l'intérieur reste à gauche, donc
/// les segments se chaînent sans ambiguïté en anneaux fermés), puis lissage de
/// Chaikin.
enum FireGeometry {
    /// Rayon de l'empreinte attribuée à une détection. Un pixel VIIRS fait
    /// 375 m de côté ; on élargit un peu pour couvrir le pourtour réel du feu
    /// et souder les détections voisines d'un même foyer.
    private static let footprintMeters = 800.0
    /// Résolution de la grille de calcul.
    private static let cellMeters = 150.0
    /// Distance de regroupement en foyers distincts.
    private static let clusterMeters = 12_000.0
    /// Sous ce seuil, un contour est du bruit et n'est pas affiché.
    private static let minAreaM2 = 300_000.0
    /// Plafond de cellules par foyer : au-delà, la grille est élargie plutôt
    /// que de faire exploser le calcul sur un foyer très étendu.
    private static let maxCells = 250_000
    /// Un foyer sans détection depuis ce délai est considéré éteint (l'API
    /// renvoie 7 jours d'historique pour dater les débuts de feux, mais seuls
    /// les foyers encore actifs sont affichés — « feux en cours »).
    private static let activeWindow: TimeInterval = 36 * 3600

    private static let metersPerDegree = 111_320.0

    /// Construit les contours des foyers **encore actifs**. La forme est bâtie
    /// sur *tout* l'historique du foyer (la zone brûlée s'accumule), tandis que
    /// l'activité récente décide seulement de l'affichage.
    static func perimeters(from fires: [Fire]) -> [FirePerimeter] {
        guard !fires.isEmpty else { return [] }
        var out: [FirePerimeter] = []
        for group in cluster(fires, thresholdMeters: clusterMeters) {
            let latest = group.compactMap { $0.detectedAt }.max()
            if let latest, Date().timeIntervalSince(latest) > activeWindow { continue }
            let shapes = rings(of: group)
            guard !shapes.isEmpty else { continue }
            let centers = shapes.map { centroid($0) }
            // Un foyer peut se scinder en plusieurs poches : chaque détection
            // est rattachée à la poche la plus proche, pour dater chaque
            // contour avec ses propres détections.
            var buckets = [[Fire]](repeating: [], count: shapes.count)
            for f in group {
                var best = 0
                var bestDistance = Double.greatestFiniteMagnitude
                for (i, c) in centers.enumerated() {
                    let d = squaredMeters(f.coordinate, c)
                    if d < bestDistance {
                        bestDistance = d
                        best = i
                    }
                }
                buckets[best].append(f)
            }
            for (i, ring) in shapes.enumerated() {
                let own = buckets[i].isEmpty ? group : buckets[i]
                out.append(
                    FirePerimeter(
                        id: out.count, polygon: ring, center: centers[i],
                        updatedAt: own.compactMap { $0.detectedAt }.max(),
                        createdAt: own.compactMap { $0.firstDetectedAt }.min()))
            }
        }
        return out
    }

    private static func squaredMeters(
        _ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D
    ) -> Double {
        let cosLat = cos(a.latitude * .pi / 180)
        let dx = (b.longitude - a.longitude) * metersPerDegree * cosLat
        let dy = (b.latitude - a.latitude) * metersPerDegree
        return dx * dx + dy * dy
    }

    /// Regroupe les feux : un point rejoint un groupe s'il est à moins de
    /// `thresholdMeters` de l'un de ses membres. Distance équirectangulaire
    /// (planaire) : à ces échelles l'écart avec la distance géodésique est
    /// négligeable, et c'est bien plus rapide que `CLLocation.distance`.
    private static func cluster(_ fires: [Fire], thresholdMeters: Double) -> [[Fire]] {
        let thresholdSq = thresholdMeters * thresholdMeters
        var groups: [[Fire]] = []
        for f in fires {
            if let idx = groups.firstIndex(where: { group in
                group.contains { squaredMeters(f.coordinate, $0.coordinate) < thresholdSq }
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
        return CLLocationCoordinate2D(
            latitude: pts.reduce(0) { $0 + $1.latitude } / n,
            longitude: pts.reduce(0) { $0 + $1.longitude } / n)
    }

    // MARK: - Contour = frontière de l'union des empreintes

    /// Anneaux fermés délimitant l'union des empreintes d'un foyer.
    private static func rings(of group: [Fire]) -> [[CLLocationCoordinate2D]] {
        guard let first = group.first?.coordinate else { return [] }
        let cosLat = cos(first.latitude * .pi / 180)
        // Projection locale en mètres, ancrée sur la première détection.
        let pts: [(x: Double, y: Double)] = group.map {
            (
                x: ($0.coordinate.longitude - first.longitude) * metersPerDegree * cosLat,
                y: ($0.coordinate.latitude - first.latitude) * metersPerDegree
            )
        }
        // Marge >= empreinte : le champ est négatif au bord, donc les anneaux
        // sont toujours fermés (sinon le contour sortirait de la grille).
        var cell = cellMeters
        let margin = footprintMeters + 3 * cell
        let xs = pts.map { $0.x }
        let ys = pts.map { $0.y }
        let minX = xs.min()! - margin
        let maxX = xs.max()! + margin
        let minY = ys.min()! - margin
        let maxY = ys.max()! + margin
        // Foyer très étendu : on dégrade la résolution plutôt que d'abandonner.
        while Int((maxX - minX) / cell + 2) * Int((maxY - minY) / cell + 2) > maxCells {
            cell *= 2
        }
        let nx = Int((maxX - minX) / cell) + 2
        let ny = Int((maxY - minY) / cell) + 2
        guard nx > 2, ny > 2 else { return [] }

        // Champ = empreinte − distance à la détection la plus proche.
        // L'iso-ligne 0 est donc exactement la frontière de l'union des disques.
        var field = [Double](repeating: -Double.greatestFiniteMagnitude, count: nx * ny)
        let reach = footprintMeters + cell
        for p in pts {
            let i0 = max(0, Int((p.x - reach - minX) / cell))
            let i1 = min(nx - 1, Int((p.x + reach - minX) / cell) + 1)
            let j0 = max(0, Int((p.y - reach - minY) / cell))
            let j1 = min(ny - 1, Int((p.y + reach - minY) / cell) + 1)
            guard i0 <= i1, j0 <= j1 else { continue }
            for j in j0...j1 {
                let dy = (minY + Double(j) * cell) - p.y
                for i in i0...i1 {
                    let dx = (minX + Double(i) * cell) - p.x
                    let v = footprintMeters - (dx * dx + dy * dy).squareRoot()
                    let k = j * nx + i
                    if v > field[k] { field[k] = v }
                }
            }
        }

        return isoRings(field, nx: nx, ny: ny)
            .filter { area($0) * cell * cell >= minAreaM2 }
            .map { ring in
                smooth(ring, iterations: 2).map { g in
                    CLLocationCoordinate2D(
                        latitude: first.latitude + (minY + g.y * cell) / metersPerDegree,
                        longitude: first.longitude
                            + (minX + g.x * cell) / (metersPerDegree * cosLat))
                }
            }
    }

    /// Aire (en cellules²) d'un anneau, par la formule du lacet.
    private static func area(_ ring: [(x: Double, y: Double)]) -> Double {
        var a = 0.0
        for i in 0..<ring.count {
            let p = ring[i]
            let q = ring[(i + 1) % ring.count]
            a += p.x * q.y - q.x * p.y
        }
        return abs(a) / 2
    }

    /// Clé d'appariement des extrémités de segments (grille au 1/100 000).
    private struct Key: Hashable {
        let x: Int
        let y: Int
        init(_ p: (x: Double, y: Double)) {
            x = Int((p.x * 100_000).rounded())
            y = Int((p.y * 100_000).rounded())
        }
    }

    /// *Marching squares* orienté : extrait l'iso-ligne `field == 0` sous forme
    /// d'anneaux fermés, en coordonnées de grille. Chaque segment est dirigé de
    /// façon à garder l'intérieur à sa gauche : chaque extrémité a donc une
    /// seule arête sortante, et le chaînage est déterministe.
    private static func isoRings(_ field: [Double], nx: Int, ny: Int)
        -> [[(x: Double, y: Double)]]
    {
        // Arêtes de la cellule : 0 = bas, 1 = droite, 2 = haut, 3 = gauche.
        // Table (depuis, vers) par configuration de coins
        // (bit 1 = bas-gauche, 2 = bas-droite, 4 = haut-droite, 8 = haut-gauche).
        let table: [[(Int, Int)]] = [
            [], [(0, 3)], [(1, 0)], [(1, 3)],
            [(2, 1)], [(0, 3), (2, 1)], [(2, 0)], [(2, 3)],
            [(3, 2)], [(0, 2)], [(1, 0), (3, 2)], [(1, 2)],
            [(3, 1)], [(0, 1)], [(3, 0)], [],
        ]
        var next: [Key: Key] = [:]
        var point: [Key: (x: Double, y: Double)] = [:]

        func cut(
            _ a: (x: Double, y: Double), _ va: Double,
            _ b: (x: Double, y: Double), _ vb: Double
        ) -> (x: Double, y: Double) {
            let d = vb - va
            let t = abs(d) < 1e-12 ? 0.5 : min(1, max(0, (0 - va) / d))
            return (x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t)
        }

        for j in 0..<(ny - 1) {
            for i in 0..<(nx - 1) {
                let vBL = field[j * nx + i]
                let vBR = field[j * nx + i + 1]
                let vTR = field[(j + 1) * nx + i + 1]
                let vTL = field[(j + 1) * nx + i]
                var code = 0
                if vBL > 0 { code |= 1 }
                if vBR > 0 { code |= 2 }
                if vTR > 0 { code |= 4 }
                if vTL > 0 { code |= 8 }
                let segs = table[code]
                if segs.isEmpty { continue }
                let bl = (x: Double(i), y: Double(j))
                let br = (x: Double(i + 1), y: Double(j))
                let tr = (x: Double(i + 1), y: Double(j + 1))
                let tl = (x: Double(i), y: Double(j + 1))
                let edge: [(x: Double, y: Double)] = [
                    cut(bl, vBL, br, vBR),  // bas
                    cut(br, vBR, tr, vTR),  // droite
                    cut(tl, vTL, tr, vTR),  // haut
                    cut(bl, vBL, tl, vTL),  // gauche
                ]
                for (from, to) in segs {
                    let a = edge[from]
                    let b = edge[to]
                    let ka = Key(a)
                    let kb = Key(b)
                    if ka == kb { continue }
                    point[ka] = a
                    point[kb] = b
                    next[ka] = kb
                }
            }
        }

        var rings: [[(x: Double, y: Double)]] = []
        while let start = next.keys.first {
            var ring: [(x: Double, y: Double)] = []
            var cur = start
            while let step = next.removeValue(forKey: cur) {
                if let p = point[cur] { ring.append(p) }
                cur = step
                if cur == start { break }
            }
            if ring.count >= 4 { rings.append(ring) }
        }
        return rings
    }

    /// Lissage par coupe-coins de Chaikin : arrondit un polygone fermé (les
    /// marches d'escalier de la grille deviennent une courbe organique).
    private static func smooth(_ ring: [(x: Double, y: Double)], iterations: Int)
        -> [(x: Double, y: Double)]
    {
        guard ring.count >= 3 else { return ring }
        var pts = ring
        for _ in 0..<iterations {
            var out: [(x: Double, y: Double)] = []
            out.reserveCapacity(pts.count * 2)
            for i in 0..<pts.count {
                let a = pts[i]
                let b = pts[(i + 1) % pts.count]
                out.append((x: a.x * 0.75 + b.x * 0.25, y: a.y * 0.75 + b.y * 0.25))
                out.append((x: a.x * 0.25 + b.x * 0.75, y: a.y * 0.25 + b.y * 0.75))
            }
            pts = out
        }
        return pts
    }
}
