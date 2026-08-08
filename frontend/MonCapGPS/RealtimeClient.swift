import Foundation

/// Connexion WebSocket temps réel : positions partagées, voitures live,
/// signalements communautaires.
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

    /// Identifiant de notre connexion, donné par le serveur à l'ouverture :
    /// sert à ignorer notre propre position live dans la diffusion.
    private var myID: Int?

    private let url: URL
    private var task: URLSessionWebSocketTask?
    private var pruneTimer: Timer?
    private var shouldReconnect = false
    /// Délai avant la prochaine tentative de reconnexion (croissant hors réseau).
    private var retryDelay: Double = 3

    init(url: URL) {
        self.url = url
    }

    func connect() {
        shouldReconnect = true
        retryDelay = 3
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
        // Le serveur attribue un identifiant par connexion : le précédent
        // n'est plus le nôtre après une reconnexion.
        myID = nil
        let task = URLSession.shared.webSocketTask(with: url)
        self.task = task
        task.resume()
        connected = true
        receive()
    }

    /// Reconnexion automatique après une coupure, avec délai croissant.
    ///
    /// Hors réseau la tentative échoue immédiatement : à cadence fixe on
    /// réessayait toutes les 3 s indéfiniment, pour rien et au détriment de la
    /// batterie. Le délai double jusqu'à 30 s, et repart à 3 s dès qu'une
    /// connexion aboutit.
    private func handleDisconnect() {
        connected = false
        guard shouldReconnect else { return }
        let delay = retryDelay
        retryDelay = Swift.min(30, delay * 2)
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            if shouldReconnect { openSocket() }
        }
    }

    /// Envoie ma position GPS en direct.
    func sendLive(lat: Double, lon: Double, label: String, avatar: String) {
        send(["kind": "live", "lat": lat, "lon": lon, "label": label, "avatar": avatar])
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
        case .hello(let id):
            // Le serveur nous répond : la connexion est bien établie.
            retryDelay = 3
            myID = id
            // Une position à nous a pu arriver avant l'identification.
            liveUsers[id] = nil
        case .positionsChanged:
            onPositionsChanged?()
        case .live(let user):
            // Notre propre position nous revient par la diffusion : on
            // l'ignore, sinon on apparaît deux fois sur la carte (avatar
            // local + écho du serveur).
            guard user.id != myID else { return }
            liveUsers[user.id] = user
        case .liveGone(let id):
            liveUsers[id] = nil
        case .alert(let alert):
            alerts.removeAll { $0.id == alert.id }
            alerts.append(alert)
        case .alertGone(let id):
            alerts.removeAll { $0.id == id }
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
                // Ne réaffecter (donc ne notifier les vues) que si quelque chose
                // a réellement expiré : sinon ce timer de 3 s invaliderait tout
                // le body de la carte en continu, app immobile.
                let users = self.liveUsers.filter { now.timeIntervalSince($0.value.lastSeen) < 15 }
                if users.count != self.liveUsers.count { self.liveUsers = users }
                let cutoff = now.timeIntervalSince1970 * 1000 - 30 * 60 * 1000
                let live = self.alerts.filter { $0.ts >= cutoff }
                if live.count != self.alerts.count { self.alerts = live }
            }
        }
    }
}
