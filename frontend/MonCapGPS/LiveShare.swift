import CoreLocation
import Foundation

/// Partage de position en direct : crée un lien de suivi côté serveur, envoie
/// périodiquement la position, et permet d'arrêter. Le proche ouvre le lien
/// dans un navigateur (aucune app requise) et suit la position en temps réel.
@MainActor
final class LiveShareManager: ObservableObject {
    @Published private(set) var active = false
    @Published private(set) var shareURL: URL?

    /// Base de l'API (identique à APIClient).
    private let baseURL = URL(string: "https://moncap-c41a5aaf07e8.herokuapp.com")!
    private var token: String?
    private var lastSend = Date.distantPast

    /// Démarre un partage ; renvoie le lien à envoyer (ou nil en cas d'échec).
    func start(name: String) async -> URL? {
        var req = URLRequest(url: baseURL.appendingPathComponent("live"))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try? JSONSerialization.data(withJSONObject: ["name": name])
        guard let (data, resp) = try? await URLSession.shared.data(for: req),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let tok = json["token"] as? String
        else { return nil }
        token = tok
        active = true
        let url = baseURL.appendingPathComponent("live").appendingPathComponent(tok)
        shareURL = url
        return url
    }

    /// Envoie la position courante (au plus toutes les 5 s).
    func update(_ c: CLLocationCoordinate2D, heading: Double, speed: Double) {
        guard active, let token, Date().timeIntervalSince(lastSend) > 5 else { return }
        lastSend = Date()
        let url = baseURL.appendingPathComponent("live").appendingPathComponent(token)
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try? JSONSerialization.data(withJSONObject: [
            "lat": c.latitude, "lon": c.longitude, "heading": heading, "speed": speed,
        ])
        Task { _ = try? await URLSession.shared.data(for: req) }
    }

    /// Arrête le partage (supprime le lien côté serveur).
    func stop() {
        if let token {
            let url = baseURL.appendingPathComponent("live").appendingPathComponent(token)
            var req = URLRequest(url: url)
            req.httpMethod = "DELETE"
            Task { _ = try? await URLSession.shared.data(for: req) }
        }
        token = nil
        active = false
        shareURL = nil
    }
}
