import CoreLocation
import Foundation

/// Un point d'incendie détecté par satellite (NASA FIRMS).
struct Fire: Identifiable {
    /// Identité stable (position) pour un rendu de carte fluide.
    var id: String { "\(Int(coordinate.latitude * 1e4))|\(Int(coordinate.longitude * 1e4))" }
    let coordinate: CLLocationCoordinate2D
    /// Instant de la détection satellite (UTC), si connu — sert à afficher
    /// « Mise à jour il y a X h » sur la carte, comme feuxdeforet.fr.
    let detectedAt: Date?

    init(coordinate: CLLocationCoordinate2D, detectedAt: Date? = nil) {
        self.coordinate = coordinate
        self.detectedAt = detectedAt
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

    /// Réponse JSON du backend : `[{ "lat": .., "lon": .., "t": .. }, ...]`
    /// (`t` = epoch secondes UTC de la détection satellite, optionnel).
    private struct Point: Decodable {
        let lat: Double
        let lon: Double
        let t: Double?
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
                detectedAt: $0.t.map { Date(timeIntervalSince1970: $0) })
        }
    }

    /// Interroge directement l'API NASA FIRMS (CSV) avec la clé de l'appareil :
    /// France métropolitaine (Corse incluse), VIIRS S-NPP, dernières 24 h.
    nonisolated private static func fetchNASA(key: String) async -> [Fire]? {
        let area = "-5.5,41,10,51.5"  // ouest,sud,est,nord
        guard
            let url = URL(
                string:
                    "https://firms.modaps.eosdis.nasa.gov/api/area/csv/\(key)/VIIRS_SNPP_NRT/\(area)/1"
            ),
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let text = String(data: data, encoding: .utf8)
        else { return nil }
        // CSV : latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,...
        // (première ligne = en-tête).
        var out: [Fire] = []
        for line in text.split(whereSeparator: \.isNewline).dropFirst() {
            let cols = line.split(separator: ",", omittingEmptySubsequences: false)
            guard cols.count >= 2, let lat = Double(cols[0]), let lon = Double(cols[1]) else {
                continue
            }
            let detected = cols.count >= 7 ? acqDate(String(cols[5]), String(cols[6])) : nil
            out.append(
                Fire(
                    coordinate: CLLocationCoordinate2D(latitude: lat, longitude: lon),
                    detectedAt: detected))
            if out.count >= 1000 { break }
        }
        return out
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
