import Foundation

/// Connexion WebSocket temps réel : positions partagées, voitures live,
/// signalements façon Waze.
@MainActor
final class RealtimeClient: ObservableObject {
    /// Voitures live des autres utilisateurs, par identifiant.
    @Published private(set) var liveUsers: [Int: LiveUser] = [:]
    /// Signalements en cours.
    @Published private(set) var alerts: [Alert] = []
    /// État de la connexion.
    @Published private(set) var connected = false

    /// Appelé quand les positions enregistrées changent (le client recharge).
    var onPositionsChanged: (() -> Void)?

    private let url: URL
    private var task: URLSessionWebSocketTask?
    private var pruneTimer: Timer?
    private var shouldReconnect = false

    init(url: URL) {
        self.url = url
    }

    func connect() {
        shouldReconnect = true
        startPruning()
        openSocket()
    }

    func disconnect() {
        shouldReconnect = false
        pruneTimer?.invalidate()
        task?.cancel(with: .goingAway, reason: nil)
        connected = false
    }

    private func openSocket() {
        let task = URLSession.shared.webSocketTask(with: url)
        self.task = task
        task.resume()
        connected = true
        receive()
    }

    /// Reconnexion automatique après une coupure (3 s).
    private func handleDisconnect() {
        connected = false
        guard shouldReconnect else { return }
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 3_000_000_000)
            if shouldReconnect { openSocket() }
        }
    }

    /// Envoie ma position GPS en direct.
    func sendLive(lat: Double, lon: Double, label: String) {
        send(["kind": "live", "lat": lat, "lon": lon, "label": label])
    }

    /// Envoie un signalement.
    func sendAlert(category: String, lat: Double, lon: Double, label: String) {
        send(["kind": "alert", "category": category, "lat": lat, "lon": lon, "label": label])
    }

    // MARK: - Privé

    private func send(_ dict: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: dict),
            let text = String(data: data, encoding: .utf8)
        else { return }
        task?.send(.string(text)) { _ in }
    }

    private func receive() {
        task?.receive { [weak self] result in
            guard let self else { return }
            switch result {
            case .failure:
                Task { @MainActor in self.handleDisconnect() }
            case .success(let message):
                if case .string(let text) = message {
                    Task { @MainActor in self.handle(text) }
                }
                self.receive()  // boucle de réception
            }
        }
    }

    private func handle(_ text: String) {
        guard let data = text.data(using: .utf8),
            let event = try? JSONDecoder().decode(ServerEvent.self, from: data)
        else { return }

        switch event {
        case .positionsChanged:
            onPositionsChanged?()
        case .live(let user):
            liveUsers[user.id] = user
        case .liveGone(let id):
            liveUsers[id] = nil
        case .alert(let alert):
            alerts.removeAll { $0.id == alert.id }
            alerts.append(alert)
        case .alerts(let list):
            alerts = list
        }
    }

    /// Purge les voitures live (>15 s) et signalements (>30 min) expirés.
    private func startPruning() {
        pruneTimer = Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                let now = Date()
                self.liveUsers = self.liveUsers.filter { now.timeIntervalSince($0.value.lastSeen) < 15 }
                let cutoff = now.timeIntervalSince1970 * 1000 - 30 * 60 * 1000
                self.alerts = self.alerts.filter { $0.ts >= cutoff }
            }
        }
    }
}
