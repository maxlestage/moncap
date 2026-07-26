import CoreLocation
import Foundation

/// Un point d'incendie détecté par satellite (NASA FIRMS).
struct Fire: Identifiable {
    /// Identité stable (position) pour un rendu de carte fluide.
    var id: String { "\(Int(coordinate.latitude * 1e4))|\(Int(coordinate.longitude * 1e4))" }
    let coordinate: CLLocationCoordinate2D
}

/// Incendies actifs sur la France métropolitaine via l'API NASA FIRMS
/// (Fire Information for Resource Management System) — détections satellite
/// VIIRS (375 m) des dernières 24 h.
///
/// FIRMS exige une clé gratuite (MAP_KEY, https://firms.modaps.eosdis.nasa.gov/api/map_key/).
/// La clé est saisie par l'utilisateur et stockée sur l'appareil ; sans clé,
/// aucun feu n'est chargé (silencieux).
@MainActor
final class FireService: ObservableObject {
    @Published private(set) var fires: [Fire] = []

    private var lastFetch = Date.distantPast
    private var lastKey = ""

    /// Rafraîchit la liste des feux (au plus toutes les 15 min ; immédiat si la
    /// clé change). Les données FIRVS NRT ne sont mises à jour que quelques fois
    /// par jour, inutile d'appeler plus souvent.
    func refresh(key: String) {
        let k = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !k.isEmpty else {
            fires = []
            lastKey = ""
            return
        }
        let keyChanged = k != lastKey
        guard keyChanged || Date().timeIntervalSince(lastFetch) > 900 else { return }
        lastFetch = Date()
        lastKey = k
        Task {
            if let f = await Self.fetch(key: k) { fires = f }
        }
    }

    /// Interroge FIRMS : France métropolitaine (Corse incluse), VIIRS S-NPP,
    /// dernières 24 h. Renvoie nil en cas d'échec (l'affichage courant est gardé).
    private static func fetch(key: String) async -> [Fire]? {
        // Zone = ouest,sud,est,nord.
        let area = "-5.5,41,10,51.5"
        guard
            let url = URL(
                string:
                    "https://firms.modaps.eosdis.nasa.gov/api/area/csv/\(key)/VIIRS_SNPP_NRT/\(area)/1"
            ),
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let text = String(data: data, encoding: .utf8)
        else { return nil }

        // CSV : latitude,longitude,... (première ligne = en-tête).
        var out: [Fire] = []
        for line in text.split(whereSeparator: \.isNewline).dropFirst() {
            let cols = line.split(separator: ",", omittingEmptySubsequences: false)
            guard cols.count >= 2,
                let lat = Double(cols[0]), let lon = Double(cols[1])
            else { continue }
            out.append(Fire(coordinate: CLLocationCoordinate2D(latitude: lat, longitude: lon)))
            if out.count >= 500 { break }  // garde-fou anti-surcharge
        }
        return out
    }
}
