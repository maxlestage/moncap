import CoreLocation
import Foundation

/// Un point d'incendie détecté par satellite (NASA FIRMS).
struct Fire: Identifiable {
    /// Identité stable (position) pour un rendu de carte fluide.
    var id: String { "\(Int(coordinate.latitude * 1e4))|\(Int(coordinate.longitude * 1e4))" }
    let coordinate: CLLocationCoordinate2D
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

    /// Base de l'API (identique à APIClient).
    private let baseURL = URL(string: "https://moncap-c41a5aaf07e8.herokuapp.com")!

    private var lastFetch = Date.distantPast

    /// Rafraîchit la liste des feux (au plus toutes les 15 min). Les données
    /// FIRMS NRT ne sont mises à jour que quelques fois par jour, inutile
    /// d'appeler plus souvent — et le backend met déjà en cache.
    func refresh() {
        guard Date().timeIntervalSince(lastFetch) > 900 else { return }
        lastFetch = Date()
        Task {
            if let f = await Self.fetch(base: baseURL) { fires = f }
        }
    }

    /// Réponse JSON du backend : `[{ "lat": .., "lon": .. }, ...]`.
    private struct Point: Decodable {
        let lat: Double
        let lon: Double
    }

    /// Interroge `GET /fires`. Renvoie nil en cas d'échec (affichage gardé).
    /// `nonisolated` : le décodage JSON + la transformation (potentiellement des
    /// centaines de points) s'exécutent hors du thread principal.
    nonisolated private static func fetch(base: URL) async -> [Fire]? {
        let url = base.appendingPathComponent("fires")
        guard
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let points = try? JSONDecoder().decode([Point].self, from: data)
        else { return nil }
        return points.map {
            Fire(coordinate: CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon))
        }
    }
}
