import SwiftUI
import MapKit

private let alertTypes: [(category: String, emoji: String, label: String, color: Color)] = [
    ("police", "🚓", "Police", .blue),
    ("accident", "💥", "Accident", .red),
    ("bouchon", "🚧", "Bouchon", .orange),
    ("danger", "⚠️", "Danger", .yellow),
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
    @State private var camera: MapCameraPosition = .userLocation(fallback: .automatic)
    @State private var showPlaces = false
    @State private var showReports = false
    @State private var gpxFile: IdentifiableURL?
    @State private var sharing = false
    @State private var driverName = "Moi"
    @State private var routeCoords: [CLLocationCoordinate2D] = []

    private var liveCars: [LiveUser] { Array(realtime.liveUsers.values) }

    var body: some View {
        ZStack(alignment: .bottom) {
            map.ignoresSafeArea()

            // Barre de recherche flottante (haut).
            VStack {
                searchBar
                Spacer()
            }
            .padding(.horizontal)
            .padding(.top, 8)

            // Carte d'itinéraire + barre d'actions (bas).
            VStack(spacing: 12) {
                if let info = routeInfo { routeCard(info) }
                bottomBar
            }
            .padding(.horizontal)
            .padding(.bottom, 6)
        }
        .sheet(isPresented: $showPlaces) { placesSheet }
        .sheet(isPresented: $showReports) { reportsSheet }
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

    // MARK: - Carte

    private var map: some View {
        Map(position: $camera) {
            UserAnnotation()
            ForEach(positions) { p in
                Marker(p.label, coordinate: .init(latitude: p.lat, longitude: p.lon))
            }
            // Itinéraire routier le plus simple, en vert.
            if !routeCoords.isEmpty {
                MapPolyline(coordinates: routeCoords)
                    .stroke(.green, style: StrokeStyle(lineWidth: 7, lineCap: .round, lineJoin: .round))
            } else if positions.count >= 2 {
                // Repli : tracé direct si le routage n'a rien renvoyé.
                MapPolyline(coordinates: positions.map { .init(latitude: $0.lat, longitude: $0.lon) })
                    .stroke(.green.opacity(0.5), style: StrokeStyle(lineWidth: 5, dash: [8, 6]))
            }
            ForEach(liveCars) { u in
                Annotation(u.label, coordinate: .init(latitude: u.lat, longitude: u.lon)) {
                    Text("🚗").font(.title)
                }
            }
            ForEach(realtime.alerts) { a in
                Annotation(a.label.isEmpty ? a.category : a.label,
                           coordinate: .init(latitude: a.lat, longitude: a.lon)) {
                    ZStack {
                        Circle().fill(.white).frame(width: 34, height: 34).shadow(radius: 2)
                        Text(emoji(for: a.category)).font(.body)
                    }
                }
            }
        }
        .mapControls { MapCompass() }
    }

    // MARK: - Superpositions

    private var searchBar: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
            Text("Où allez-vous ?").foregroundStyle(.secondary)
            Spacer()
            Circle()
                .fill(realtime.connected ? .green : .gray)
                .frame(width: 9, height: 9)
        }
        .padding(.vertical, 14)
        .padding(.horizontal, 16)
        .background(.regularMaterial, in: Capsule())
        .shadow(color: .black.opacity(0.15), radius: 6, y: 3)
        .onTapGesture { showPlaces = true }
    }

    private func routeCard(_ info: String) -> some View {
        HStack {
            Image(systemName: "car.fill").foregroundStyle(.blue)
            Text(info).font(.subheadline.weight(.semibold))
            Spacer()
            Button { routeInfo = nil } label: {
                Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
            }
        }
        .padding(14)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        .shadow(color: .black.opacity(0.15), radius: 6, y: 3)
    }

    private var bottomBar: some View {
        HStack(alignment: .bottom) {
            speedPill
            Spacer()
            VStack(spacing: 14) {
                circleButton(system: "location.fill", tint: .blue) { recenter() }
                reportButton
            }
        }
    }

    private var speedPill: some View {
        VStack(spacing: 0) {
            Text("\(Int(location.speedKmh))").font(.title2.weight(.bold))
            Text("km/h").font(.caption2).foregroundStyle(.secondary)
        }
        .frame(width: 70, height: 70)
        .background(.regularMaterial, in: Circle())
        .shadow(color: .black.opacity(0.15), radius: 6, y: 3)
    }

    private var reportButton: some View {
        Button { showReports = true } label: {
            Image(systemName: "exclamationmark.bubble.fill")
                .font(.title2)
                .foregroundStyle(.white)
                .frame(width: 64, height: 64)
                .background(Color.orange, in: Circle())
                .shadow(color: .orange.opacity(0.5), radius: 8, y: 4)
        }
        .disabled(location.coordinate == nil)
    }

    private func circleButton(system: String, tint: Color, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: system)
                .font(.title3)
                .foregroundStyle(tint)
                .frame(width: 48, height: 48)
                .background(.regularMaterial, in: Circle())
                .shadow(color: .black.opacity(0.15), radius: 5, y: 2)
        }
    }

    // MARK: - Feuille « Lieux »

    private var placesSheet: some View {
        NavigationStack {
            List {
                Section {
                    Button {
                        Task { await saveCurrent() }
                    } label: {
                        Label("Enregistrer ma position", systemImage: "mappin.and.ellipse")
                    }
                    .disabled(location.coordinate == nil)
                    Button {
                        Task {
                            await updateRoute()
                            fitRoute()
                            showPlaces = false
                        }
                    } label: {
                        Label("Itinéraire le plus simple", systemImage: "point.topleft.down.to.point.bottomright.curvepath")
                    }
                    .disabled(positions.count < 2)
                }
                if let s = stats {
                    Section("Statistiques") {
                        Text("\(s.count) positions · \(String(format: "%.1f", s.total_km)) km")
                            .font(.subheadline)
                    }
                }
                Section("Positions") {
                    ForEach(positions) { p in
                        VStack(alignment: .leading) {
                            Text(p.label).font(.headline)
                            Text(String(format: "%.4f, %.4f", p.lat, p.lon))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    .onDelete(perform: deletePositions)
                    if positions.isEmpty {
                        Text("Aucune position — utilise « Enregistrer ma position ».")
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .navigationTitle("MonCap GPS")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button(sharing ? "Stop partage" : "Partager", systemImage: sharing ? "dot.radiowaves.left.and.right" : "location") {
                        toggleSharing()
                    }
                    .tint(sharing ? .green : .blue)
                }
                ToolbarItemGroup(placement: .topBarTrailing) {
                    Button("GPX", systemImage: "square.and.arrow.up") { Task { await exportGPX() } }
                        .disabled(positions.isEmpty)
                    Button("OK") { showPlaces = false }
                }
            }
            .presentationDetents([.medium, .large])
        }
    }

    // MARK: - Feuille « Signaler »

    private var reportsSheet: some View {
        VStack(spacing: 16) {
            Text("Signaler").font(.headline).padding(.top, 8)
            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 16) {
                ForEach(alertTypes, id: \.category) { t in
                    Button {
                        report(category: t.category, label: t.label)
                        showReports = false
                    } label: {
                        VStack(spacing: 8) {
                            Text(t.emoji).font(.largeTitle)
                            Text(t.label).font(.subheadline.weight(.semibold))
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 18)
                        .background(t.color.opacity(0.15), in: RoundedRectangle(cornerRadius: 18))
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal)
        }
        .padding(.bottom, 16)
        .presentationDetents([.height(280)])
    }

    // MARK: - Actions

    private func refresh() async {
        positions = (try? await api.positions()) ?? []
        stats = try? await api.stats()
        await updateRoute()
    }

    /// Calcule l'itinéraire routier le plus simple (vert) et son résumé.
    private func updateRoute() async {
        let pts = positions.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
        guard pts.count >= 2 else {
            routeCoords = []
            routeInfo = nil
            return
        }
        if let r = await RouteService.roadRoute(through: pts) {
            routeCoords = r.coordinates
            routeInfo = String(format: "%.1f km · %.0f min", r.distanceKm, r.minutes)
        }
    }

    private func recenter() {
        guard let c = location.coordinate else { return }
        withAnimation {
            camera = .region(MKCoordinateRegion(center: c, latitudinalMeters: 1200, longitudinalMeters: 1200))
        }
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
    }

    private func deletePositions(at offsets: IndexSet) {
        let ids = offsets.map { positions[$0].id }
        Task { for id in ids { try? await api.delete(id: id) } }
    }

    /// Cadre la carte sur l'itinéraire calculé.
    private func fitRoute() {
        let pts = routeCoords.isEmpty
            ? positions.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
            : routeCoords
        guard pts.count >= 2 else { return }
        let lats = pts.map(\.latitude)
        let lons = pts.map(\.longitude)
        let center = CLLocationCoordinate2D(
            latitude: (lats.min()! + lats.max()!) / 2,
            longitude: (lons.min()! + lons.max()!) / 2)
        let span = MKCoordinateSpan(
            latitudeDelta: (lats.max()! - lats.min()!) * 1.4 + 0.01,
            longitudeDelta: (lons.max()! - lons.min()!) * 1.4 + 0.01)
        withAnimation {
            camera = .region(MKCoordinateRegion(center: center, span: span))
        }
    }

    private func exportGPX() async {
        if let url = try? await api.exportGPX() {
            gpxFile = IdentifiableURL(url: url)
        }
    }
}
