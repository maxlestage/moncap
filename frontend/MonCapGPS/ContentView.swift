import SwiftUI
import MapKit

private let alertTypes: [(category: String, emoji: String, label: String)] = [
    ("police", "🚓", "Police"),
    ("accident", "💥", "Accident"),
    ("bouchon", "🚧", "Bouchon"),
    ("danger", "⚠️", "Danger"),
]

private func emoji(for category: String) -> String {
    alertTypes.first { $0.category == category }?.emoji ?? "⚠️"
}

struct ContentView: View {
    @StateObject private var location = LocationManager()
    @StateObject private var realtime = RealtimeClient(url: APIClient().wsURL)
    private let api = APIClient()

    @State private var positions: [Position] = []
    @State private var stats: Stats?
    @State private var routeInfo: String?
    @State private var camera: MapCameraPosition = .automatic
    @State private var showList = false
    @State private var gpxFile: IdentifiableURL?
    @State private var sharing = false
    @State private var driverName = "Moi"

    private var liveCars: [LiveUser] { Array(realtime.liveUsers.values) }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                map
                controls
            }
            .navigationTitle("MonCap GPS")
            .toolbar {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    connectionDot
                    Button("Exporter GPX", systemImage: "square.and.arrow.up") {
                        Task { await exportGPX() }
                    }
                    .disabled(positions.isEmpty)
                    Button("Liste", systemImage: "list.bullet") { showList = true }
                }
            }
            .sheet(isPresented: $showList) { positionList }
            .sheet(item: $gpxFile) { file in ShareSheet(items: [file.url]) }
            .task {
                location.start()
                realtime.onPositionsChanged = { Task { await refresh() } }
                realtime.connect()
                await refresh()
            }
            .onReceive(location.$coordinate) { coord in
                if sharing, let c = coord {
                    realtime.sendLive(lat: c.latitude, lon: c.longitude, label: driverName)
                }
            }
        }
    }

    // MARK: - Carte

    private var map: some View {
        Map(position: $camera) {
            UserAnnotation()
            ForEach(positions) { p in
                Marker(p.label, coordinate: .init(latitude: p.lat, longitude: p.lon))
            }
            if positions.count >= 2 {
                MapPolyline(coordinates: positions.map { .init(latitude: $0.lat, longitude: $0.lon) })
                    .stroke(.blue, lineWidth: 3)
            }
            // Voitures live des autres utilisateurs.
            ForEach(liveCars) { u in
                Annotation(u.label, coordinate: .init(latitude: u.lat, longitude: u.lon)) {
                    Text("🚗").font(.title2)
                }
            }
            // Signalements façon Waze.
            ForEach(realtime.alerts) { a in
                Annotation(a.label.isEmpty ? a.category : a.label,
                           coordinate: .init(latitude: a.lat, longitude: a.lon)) {
                    Text(emoji(for: a.category)).font(.title2)
                }
            }
        }
        .frame(maxHeight: .infinity)
    }

    private var connectionDot: some View {
        Image(systemName: "circle.fill")
            .font(.system(size: 10))
            .foregroundStyle(realtime.connected ? .green : .gray)
    }

    // MARK: - Contrôles

    private var controls: some View {
        VStack(spacing: 12) {
            if let info = routeInfo {
                Text(info).font(.headline)
            }

            HStack {
                Button(sharing ? "🟢 Partage…" : "📍 Partager") { toggleSharing() }
                    .buttonStyle(.borderedProminent)
                    .tint(sharing ? .green : .blue)
                    .disabled(location.coordinate == nil)
                if !liveCars.isEmpty {
                    Text("\(liveCars.count) en direct")
                        .font(.caption).padding(6)
                        .background(.green.opacity(0.15)).clipShape(Capsule())
                }
            }

            // Boutons de signalement.
            HStack {
                ForEach(alertTypes, id: \.category) { t in
                    Button("\(t.emoji)") { report(category: t.category, label: t.label) }
                        .buttonStyle(.bordered)
                        .disabled(location.coordinate == nil)
                }
            }

            HStack {
                Button("Enregistrer ma position") { Task { await saveCurrent() } }
                    .buttonStyle(.bordered)
                    .disabled(location.coordinate == nil)
                Button("Itinéraire") { Task { await fullRoute() } }
                    .buttonStyle(.bordered)
                    .disabled(positions.count < 2)
            }
        }
        .padding()
    }

    private var positionList: some View {
        NavigationStack {
            List {
                if let s = stats {
                    Section("Statistiques") {
                        Text("\(s.count) positions · itinéraire \(String(format: "%.1f", s.total_km)) km")
                            .font(.subheadline)
                    }
                }
                ForEach(positions) { p in
                    VStack(alignment: .leading) {
                        Text(p.label).font(.headline)
                        Text(String(format: "%.4f, %.4f", p.lat, p.lon))
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                .onDelete(perform: deletePositions)
            }
            .navigationTitle("Positions")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("OK") { showList = false }
                }
            }
        }
    }

    // MARK: - Actions

    private func refresh() async {
        positions = (try? await api.positions()) ?? []
        stats = try? await api.stats()
    }

    private func toggleSharing() {
        sharing.toggle()
        if sharing, let c = location.coordinate {
            realtime.sendLive(lat: c.latitude, lon: c.longitude, label: driverName)
        }
    }

    private func report(category: String, label: String) {
        guard let c = location.coordinate else { return }
        realtime.sendAlert(category: category, lat: c.latitude, lon: c.longitude, label: label)
    }

    private func saveCurrent() async {
        guard let c = location.coordinate else { return }
        let pos = NewPosition(lat: c.latitude, lon: c.longitude, label: "Point \(positions.count + 1)")
        _ = try? await api.add(pos)
        // La liste se rafraîchira aussi via l'événement temps réel.
    }

    private func deletePositions(at offsets: IndexSet) {
        let ids = offsets.map { positions[$0].id }
        Task { for id in ids { try? await api.delete(id: id) } }
    }

    private func fullRoute() async {
        let points = positions.map { Coord(lat: $0.lat, lon: $0.lon) }
        if let r = try? await api.multiRoute(points) {
            routeInfo = String(format: "Itinéraire : %.1f km · %.0f min", r.total_km, r.duration_min)
        }
    }

    private func exportGPX() async {
        if let url = try? await api.exportGPX() {
            gpxFile = IdentifiableURL(url: url)
        }
    }
}
