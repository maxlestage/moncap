import CoreLocation
import Foundation

/// Un point d'incendie détecté par satellite (NASA FIRMS).
struct Fire: Identifiable {
    /// Identité stable (position) pour un rendu de carte fluide.
    var id: String { "\(Int(coordinate.latitude * 1e4))|\(Int(coordinate.longitude * 1e4))" }
    let coordinate: CLLocationCoordinate2D
    /// Dernière détection satellite (UTC), si connue — sert à afficher
    /// « Mise à jour il y a X h » sur la carte, comme feuxdeforet.fr.
    let detectedAt: Date?
    /// Première détection à cet endroit sur la fenêtre d'historique (7 j) —
    /// sert à afficher « Créé il y a X j ».
    let firstDetectedAt: Date?

    init(coordinate: CLLocationCoordinate2D, detectedAt: Date? = nil, firstDetectedAt: Date? = nil)
    {
        self.coordinate = coordinate
        self.detectedAt = detectedAt
        self.firstDetectedAt = firstDetectedAt
    }
}

/// Incendies actifs sur la France métropolitaine, servis par **notre backend**
/// (`GET /fires`) — détections satellite VIIRS (375 m) des dernières 24 h via
/// l'API NASA FIRMS.
///
/// La clé FIRMS est détenue côté serveur (Heroku) : l'app n'a aucune clé à
/// configurer, tous les utilisateurs profitent des feux automatiquement. En
/// cas d'échec, l'affichage courant est conservé (best effort, silencieux).
@MainActor
final class FireService: ObservableObject {
    @Published private(set) var fires: [Fire] = []
    /// Dernière récupération réussie — repli pour « Mise à jour il y a X h »
    /// quand les points n'ont pas d'horodatage satellite.
    @Published private(set) var lastUpdate: Date?

    /// Base de l'API (identique à APIClient).
    private let baseURL = URL(string: "https://moncap-c41a5aaf07e8.herokuapp.com")!

    private var lastFetch = Date.distantPast
    private var lastKey = ""

    /// Rafraîchit la liste des feux (au plus toutes les 15 min ; immédiat si la
    /// clé change). Si `key` (clé FIRMS saisie dans les réglages) est non vide,
    /// l'app interroge la NASA **directement** ; sinon elle passe par notre
    /// backend. Les données FIRMS NRT ne changent que quelques fois par jour.
    func refresh(key: String) {
        let k = key.trimmingCharacters(in: .whitespacesAndNewlines)
        let changed = k != lastKey
        guard changed || Date().timeIntervalSince(lastFetch) > 900 else { return }
        lastFetch = Date()
        lastKey = k
        let base = baseURL
        Task {
            let f = k.isEmpty ? await Self.fetchBackend(base: base) : await Self.fetchNASA(key: k)
            if let f {
                fires = f
                lastUpdate = Date()
            }
        }
    }

    /// Réponse JSON du backend : `[{ "lat": .., "lon": .., "t": .., "t0": .. },
    /// ...]` (`t` = dernière détection, `t0` = première, epoch UTC, optionnels).
    private struct Point: Decodable {
        let lat: Double
        let lon: Double
        let t: Double?
        let t0: Double?
    }

    /// Interroge notre backend `GET /fires`. `nonisolated` : décodage hors du
    /// thread principal. Renvoie nil en cas d'échec (affichage gardé).
    nonisolated private static func fetchBackend(base: URL) async -> [Fire]? {
        let url = base.appendingPathComponent("fires")
        guard
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let points = try? JSONDecoder().decode([Point].self, from: data)
        else { return nil }
        return points.map {
            Fire(
                coordinate: CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon),
                detectedAt: $0.t.map { Date(timeIntervalSince1970: $0) },
                firstDetectedAt: $0.t0.map { Date(timeIntervalSince1970: $0) })
        }
    }

    /// Interroge directement l'API NASA FIRMS (CSV) avec la clé de l'appareil :
    /// France métropolitaine (Corse incluse), VIIRS S-NPP, **7 derniers jours**
    /// (pour dater le début des foyers), avec le même filtrage anti-faux
    /// positifs que le backend.
    nonisolated private static func fetchNASA(key: String) async -> [Fire]? {
        let area = "-5.5,41,10,51.5"  // ouest,sud,est,nord
        guard
            let url = URL(
                string:
                    "https://firms.modaps.eosdis.nasa.gov/api/area/csv/\(key)/VIIRS_SNPP_NRT/\(area)/7"
            ),
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let text = String(data: data, encoding: .utf8)
        else { return nil }
        return parseFIRMS(text)
    }

    /// Taille d'une cellule de dédoublonnage (~550 m ≈ 1 pixel VIIRS).
    private static let gridDeg = 0.005
    /// Garde-fou sur le nombre de points gardés (les plus récents d'abord).
    private static let maxPoints = 4000

    /// Analyse le CSV FIRMS comme le backend : écarte les détections de
    /// confiance basse et de puissance (FRP) < 2 MW (torchères, brûlages,
    /// reflets), puis dédoublonne par cellule de grille en agrégeant première
    /// et dernière détection.
    ///
    /// Colonnes VIIRS : latitude,longitude,bright_ti4,scan,track,acq_date,
    /// acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight.
    nonisolated private static func parseFIRMS(_ text: String) -> [Fire] {
        struct Cell {
            var lat: Double
            var lon: Double
            var first: Date?
            var last: Date?
        }
        var cells: [Int64: Cell] = [:]
        for line in text.split(whereSeparator: \.isNewline).dropFirst() {
            let cols = line.split(separator: ",", omittingEmptySubsequences: false)
            guard cols.count >= 2, let lat = Double(cols[0]), let lon = Double(cols[1]) else {
                continue
            }
            if cols.count >= 10 {
                let conf = cols[9].trimmingCharacters(in: .whitespaces).lowercased()
                if conf == "l" || conf == "low" { continue }
                if let pct = Double(conf), pct < 30 { continue }
            }
            if cols.count >= 13, let frp = Double(cols[12]), frp < 2 { continue }
            let t = cols.count >= 7 ? acqDate(String(cols[5]), String(cols[6])) : nil
            let key =
                Int64((lat / gridDeg).rounded()) &* 1_000_000
                &+ Int64((lon / gridDeg).rounded())
            if var cell = cells[key] {
                if let t, cell.last == nil || t > cell.last! {
                    cell.lat = lat
                    cell.lon = lon
                    cell.last = t
                }
                if let t, cell.first == nil || t < cell.first! { cell.first = t }
                cells[key] = cell
            } else {
                cells[key] = Cell(lat: lat, lon: lon, first: t, last: t)
            }
        }
        // Cellules les plus récentes d'abord : le garde-fou coupe les vieilles
        // détections, pas les foyers actifs.
        return
            cells.values
            .sorted { ($0.last ?? .distantPast) > ($1.last ?? .distantPast) }
            .prefix(maxPoints)
            .map {
                Fire(
                    coordinate: CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon),
                    detectedAt: $0.last, firstDetectedAt: $0.first)
            }
    }

    /// Convertit une acquisition FIRMS (`acq_date` = « AAAA-MM-JJ », `acq_time`
    /// = « HHMM » UTC, parfois sans zéros de tête) en `Date`.
    nonisolated private static func acqDate(_ date: String, _ time: String) -> Date? {
        let parts = date.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3, let hm = Int(time.trimmingCharacters(in: .whitespaces)) else {
            return nil
        }
        var comps = DateComponents()
        comps.year = parts[0]
        comps.month = parts[1]
        comps.day = parts[2]
        comps.hour = hm / 100
        comps.minute = hm % 100
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC")!
        return cal.date(from: comps)
    }
}
