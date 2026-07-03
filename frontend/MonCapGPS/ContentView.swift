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

/// Un trajet coloré affiché sur la carte (vers un point enregistré).
struct ColoredRoute: Identifiable {
    let id = UUID()
    let coordinates: [CLLocationCoordinate2D]
    let color: Color
    let label: String
}

/// Une option d'itinéraire vers une destination (le plus simple + alternatives).
struct RouteOption: Identifiable {
    let id = UUID()
    let coordinates: [CLLocationCoordinate2D]
    /// Étapes pour la navigation (assemblées si l'itinéraire a plusieurs tronçons).
    let steps: [NavStep]
    let minutes: Double
    let km: Double
    let turns: Int
    var isSimplest = false
    /// Couleur du tracé sur la carte (verte pour le plus simple).
    var color: Color = .gray
}

/// Étiquette d'un itinéraire (le plus simple / rapide / court…).
fileprivate struct RouteTag: Identifiable {
    var id: String { text }
    let text: String
    let color: Color
}

/// Tronçon calculé : tracé + étapes + métriques (assemblable).
fileprivate struct RouteBuild {
    let coords: [CLLocationCoordinate2D]
    let steps: [NavStep]
    let km: Double
    let minutes: Double
}

/// Calcule un ou plusieurs itinéraires voiture entre deux points.
fileprivate func buildRoutes(
    from: CLLocationCoordinate2D, to: CLLocationCoordinate2D, alternates: Bool
) async -> [RouteBuild] {
    let req = MKDirections.Request()
    req.source = MKMapItem(placemark: MKPlacemark(coordinate: from))
    req.destination = MKMapItem(placemark: MKPlacemark(coordinate: to))
    req.transportType = .automobile
    req.requestsAlternateRoutes = alternates
    guard let routes = try? await MKDirections(request: req).calculate().routes else { return [] }
    return routes.map { r in
        RouteBuild(
            coords: r.polyline.coordinates,
            steps: r.steps.map {
                NavStep(text: $0.instructions,
                        coord: $0.polyline.coordinates.last ?? $0.polyline.coordinate)
            },
            km: r.distance / 1000,
            minutes: r.expectedTravelTime / 60)
    }
}

/// Point de passage décalé perpendiculairement au trajet direct, pour
/// diversifier les itinéraires (fraction de la distance, côté ±1).
fileprivate func offsetVia(
    from a: CLLocationCoordinate2D, to b: CLLocationCoordinate2D,
    fraction: Double, side: Double
) -> CLLocationCoordinate2D {
    let midLat = (a.latitude + b.latitude) / 2
    let mPerLat = 111_320.0
    let mPerLon = 111_320.0 * cos(midLat * .pi / 180)
    let ax = a.longitude * mPerLon, ay = a.latitude * mPerLat
    let bx = b.longitude * mPerLon, by = b.latitude * mPerLat
    let mx = (ax + bx) / 2, my = (ay + by) / 2
    let dx = bx - ax, dy = by - ay
    let len = max(1, (dx * dx + dy * dy).squareRoot())
    let px = -dy / len, py = dx / len
    let off = len * fraction * side
    return CLLocationCoordinate2D(
        latitude: (my + py * off) / mPerLat,
        longitude: (mx + px * off) / mPerLon)
}

/// Palette de couleurs pour distinguer les trajets simultanés.
private let routePalette: [Color] = [
    .green, .blue, .orange, .purple, .red, .teal, .pink, .indigo, .brown, .cyan,
]

/// Point d'entrée : écran de connexion ou carte selon l'authentification.
struct ContentView: View {
    @StateObject private var auth = AuthStore()

    var body: some View {
        if auth.isAuthenticated {
            MapHomeView(auth: auth)
        } else {
            LoginView(auth: auth)
        }
    }
}

struct MapHomeView: View {
    @ObservedObject var auth: AuthStore
    @StateObject private var location = LocationManager()
    @StateObject private var realtime = RealtimeClient(url: APIClient().wsURL)
    @StateObject private var nav = NavigationManager()
    @StateObject private var placeSearch = PlaceSearch()
    private let api = APIClient()

    @State private var positions: [Position] = []
    @State private var stats: Stats?
    @State private var routeInfo: String?
    @State private var camera: MapCameraPosition = .userLocation(fallback: .automatic)
    @State private var showPlaces = false
    @State private var showSearch = false
    @State private var destQuery = ""
    @State private var recents: [RecentSearch] = Session.recentSearches
    @State private var showReports = false
    @State private var gpxFile: IdentifiableURL?
    // Partagé automatiquement pour que tout le monde apparaisse sur la carte.
    @State private var sharing = true
    // Nom affiché aux autres : l'e-mail de connexion.
    @State private var driverName = Session.username.isEmpty ? "Moi" : Session.username
    @State private var avatar = Session.avatar
    @State private var routeCoords: [CLLocationCoordinate2D] = []
    @State private var multiRoutes: [ColoredRoute] = []
    @State private var selectedIDs: Set<Int> = []
    @State private var routeOptions: [RouteOption] = []
    /// Itinéraire actuellement prévisualisé (mis en avant) avant décision.
    @State private var selectedRouteID: UUID?
    /// Liste déroulée ou repliée (bandeau compact par défaut).
    @State private var routesExpanded = false
    @State private var pendingDestination: CLLocationCoordinate2D?
    /// Carte inclinée en 3D si l'utilisateur l'active.
    @State private var is3D = false
    /// Prévisualisation (survol) de l'itinéraire en cours.
    @State private var previewing = false
    @State private var previewTask: Task<Void, Never>?

    // Historique des trajets parcourus.
    @State private var trips: [Trip] = []
    @State private var showTrips = false
    @State private var displayedTrip: Trip?
    /// Points réellement parcourus pendant la navigation en cours.
    @State private var recordedTrack: [CLLocationCoordinate2D] = []
    /// Heure de départ de la navigation en cours (pour la durée réelle).
    @State private var tripStart: Date?

    private var liveCars: [LiveUser] { Array(realtime.liveUsers.values) }

    var body: some View {
        ZStack(alignment: .bottom) {
            map.ignoresSafeArea()

            // Haut : bannière de navigation ou barre de recherche.
            VStack {
                if nav.active {
                    navBanner
                } else {
                    HStack(spacing: 10) {
                        searchBar
                        menuButton
                    }
                }
                Spacer()
            }
            .padding(.horizontal)
            .padding(.top, 8)

            // Bas : carte ETA/itinéraire + barre d'actions.
            VStack(spacing: 12) {
                if nav.active {
                    etaCard
                } else if !multiRoutes.isEmpty {
                    multiRouteCard
                } else if let info = routeInfo {
                    routeCard(info)
                }
                bottomBar
                // Comparateur d'itinéraires calé tout en bas (sous les boutons)
                // pour dégager la vue sur les tracés.
                if !nav.active && !routeOptions.isEmpty {
                    routeOptionsCard
                }
            }
            .padding(.horizontal)
            .padding(.bottom, 6)
        }
        .sheet(isPresented: $showPlaces) { placesSheet }
        .sheet(isPresented: $showSearch) { searchSheet }
        .sheet(isPresented: $showReports) { reportsSheet }
        .sheet(isPresented: $showTrips) { tripsSheet }
        .sheet(item: $gpxFile) { file in ShareSheet(items: [file.url]) }
        .task {
            location.start()
            realtime.onPositionsChanged = { Task { await refresh() } }
            nav.onReroute = { Task { await recomputeRoute() } }
            realtime.connect()
            await refresh()
            await loadTrips()
            await loadRecents()
        }
        .onReceive(location.$coordinate) { coord in
            guard let c = coord else { return }
            if sharing {
                realtime.sendLive(lat: c.latitude, lon: c.longitude, label: driverName, avatar: avatar)
            }
            if nav.active {
                nav.update(c)
                recordTrackPoint(c)
                // Vue conduite 3D : la caméra suit, inclinée, dans le sens de la marche.
                if is3D, !previewing {
                    withAnimation(.easeOut(duration: 0.4)) {
                        camera = .camera(MapCamera(
                            centerCoordinate: c, distance: 500,
                            heading: location.course, pitch: 60))
                    }
                }
            }
        }
        .onChange(of: nav.active) { wasActive, isActive in
            // Fin de navigation (arrivée ou « Quitter ») → on enregistre le trajet.
            if wasActive && !isActive {
                Task { await saveRecordedTrip() }
            }
            // Début de navigation → nouvel enregistrement.
            if !wasActive && isActive {
                recordedTrack = location.coordinate.map { [$0] } ?? []
                tripStart = Date()
            }
        }
    }

    /// Ajoute un point au tracé en cours, en filtrant les points trop proches
    /// (≥ 12 m) pour limiter la taille du tracé enregistré.
    private func recordTrackPoint(_ c: CLLocationCoordinate2D) {
        if let last = recordedTrack.last {
            let d = CLLocation(latitude: last.latitude, longitude: last.longitude)
                .distance(from: CLLocation(latitude: c.latitude, longitude: c.longitude))
            guard d >= 12 else { return }
        }
        recordedTrack.append(c)
    }

    /// Enregistre en base le trajet qui vient d'être parcouru.
    private func saveRecordedTrip() async {
        let track = recordedTrack
        let start = tripStart
        recordedTrack = []
        tripStart = nil
        guard track.count >= 2 else { return }
        let duration = start.map { Date().timeIntervalSince($0) / 60 } ?? 0
        let points = track.map { Coord(lat: $0.latitude, lon: $0.longitude) }
        let new = NewTrip(label: "", points: points, duration_min: duration)
        _ = try? await api.saveTrip(new)
        await loadTrips()
    }

    private func loadTrips() async {
        trips = (try? await api.trips()) ?? trips
    }

    /// Itinéraire affiché : celui de la navigation en cours, sinon le calcul multi-points.
    private var displayedRoute: [CLLocationCoordinate2D] {
        nav.active ? nav.routeCoords : routeCoords
    }

    // MARK: - Carte

    private var map: some View {
        MapReader { proxy in
        Map(position: $camera) {
            UserAnnotation()
            // Mon avatar affiché à ma position (hors navigation).
            if let me = location.coordinate, !nav.active {
                Annotation("Moi", coordinate: me) {
                    Image(Avatars.asset(avatar))
                        .resizable()
                        .scaledToFit()
                        .frame(width: 40, height: 40)
                        .shadow(radius: 2)
                }
            }
            ForEach(positions) { p in
                Marker(p.label, coordinate: .init(latitude: p.lat, longitude: p.lon))
            }
            // Options d'itinéraire : chacune sa couleur ; celle prévisualisée
            // est mise en avant (plus épaisse, opaque) et dessinée par-dessus.
            if !routeOptions.isEmpty && !nav.active {
                ForEach(routeOptions.filter { $0.id != selectedRouteID }) { o in
                    MapPolyline(coordinates: o.coordinates)
                        .stroke(o.color.opacity(0.45),
                                style: StrokeStyle(lineWidth: 5, lineCap: .round, lineJoin: .round))
                }
                ForEach(routeOptions.filter { $0.id == selectedRouteID }) { o in
                    MapPolyline(coordinates: o.coordinates)
                        .stroke(o.color,
                                style: StrokeStyle(lineWidth: 8, lineCap: .round, lineJoin: .round))
                }
            } else if !multiRoutes.isEmpty && !nav.active {
                // Plusieurs trajets simultanés (un par point), chacun sa couleur.
                ForEach(multiRoutes) { r in
                    MapPolyline(coordinates: r.coordinates)
                        .stroke(r.color, style: StrokeStyle(lineWidth: 6, lineCap: .round, lineJoin: .round))
                }
            } else if !displayedRoute.isEmpty {
                // Itinéraire routier le plus simple, en vert.
                MapPolyline(coordinates: displayedRoute)
                    .stroke(.green, style: StrokeStyle(lineWidth: 7, lineCap: .round, lineJoin: .round))
            } else if positions.count >= 2 {
                // Repli : tracé direct si le routage n'a rien renvoyé.
                MapPolyline(coordinates: positions.map { .init(latitude: $0.lat, longitude: $0.lon) })
                    .stroke(.green.opacity(0.5), style: StrokeStyle(lineWidth: 5, dash: [8, 6]))
            }
            // Trajet enregistré sélectionné dans l'historique (en indigo).
            if let t = displayedTrip, !nav.active {
                let coords = t.coordinates.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
                MapPolyline(coordinates: coords)
                    .stroke(.indigo, style: StrokeStyle(lineWidth: 6, lineCap: .round, lineJoin: .round))
                if let start = coords.first {
                    Marker("Départ", systemImage: "flag.fill", coordinate: start).tint(.indigo)
                }
                if let end = coords.last {
                    Marker("Arrivée", systemImage: "flag.checkered", coordinate: end).tint(.indigo)
                }
            }
            ForEach(liveCars) { u in
                Annotation(u.label, coordinate: .init(latitude: u.lat, longitude: u.lon)) {
                    Image(Avatars.asset(u.avatar))
                        .resizable()
                        .scaledToFit()
                        .frame(width: 44, height: 44)
                        .shadow(radius: 2)
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
        // Toucher (près d')un tracé le sélectionne directement sur la carte.
        .onTapGesture(coordinateSpace: .local) { point in
            handleMapTap(point, proxy: proxy)
        }
        }
    }

    /// Sélectionne l'itinéraire dont le tracé est le plus proche du point touché.
    private func handleMapTap(_ point: CGPoint, proxy: MapProxy) {
        guard !nav.active, !routeOptions.isEmpty,
            let tapped = proxy.convert(point, from: .local)
        else { return }
        // Tolérance ≈ largeur d'un doigt, convertie en mètres au zoom courant.
        let offset = proxy.convert(CGPoint(x: point.x + 24, y: point.y), from: .local)
        let tolerance = offset.map { distanceMeters(tapped, $0) } ?? 80

        var best: (id: UUID, dist: Double)?
        for o in routeOptions {
            let d = distanceToPolyline(tapped, o.coordinates)
            if best == nil || d < best!.dist { best = (o.id, d) }
        }
        if let b = best, b.dist <= max(tolerance, 40) {
            withAnimation { selectedRouteID = b.id }
        }
    }

    /// Distance (m) entre deux coordonnées.
    private func distanceMeters(_ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D) -> Double {
        CLLocation(latitude: a.latitude, longitude: a.longitude)
            .distance(from: CLLocation(latitude: b.latitude, longitude: b.longitude))
    }

    /// Distance minimale (m) d'un point à une polyligne (projection locale).
    private func distanceToPolyline(_ p: CLLocationCoordinate2D,
                                    _ coords: [CLLocationCoordinate2D]) -> Double {
        guard coords.count >= 2 else {
            return coords.first.map { distanceMeters(p, $0) } ?? .greatestFiniteMagnitude
        }
        let mPerDegLat = 111_320.0
        let mPerDegLon = 111_320.0 * cos(p.latitude * .pi / 180)
        func xy(_ c: CLLocationCoordinate2D) -> (Double, Double) {
            ((c.longitude - p.longitude) * mPerDegLon, (c.latitude - p.latitude) * mPerDegLat)
        }
        var best = Double.greatestFiniteMagnitude
        for i in 0..<(coords.count - 1) {
            let (ax, ay) = xy(coords[i])
            let (bx, by) = xy(coords[i + 1])
            let dx = bx - ax, dy = by - ay
            let len2 = dx * dx + dy * dy
            let t = len2 == 0 ? 0 : max(0, min(1, -(ax * dx + ay * dy) / len2))
            let cx = ax + t * dx, cy = ay + t * dy
            best = min(best, (cx * cx + cy * cy).squareRoot())
        }
        return best
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
        .onTapGesture { showSearch = true }
    }

    /// Bouton menu (lieux enregistrés, avatar, compte, partage…).
    private var menuButton: some View {
        Button { showPlaces = true } label: {
            Image(systemName: "line.3.horizontal")
                .font(.title3)
                .foregroundStyle(.blue)
                .frame(width: 52, height: 52)
                .background(.regularMaterial, in: Circle())
                .shadow(color: .black.opacity(0.15), radius: 6, y: 3)
        }
    }

    private var navBanner: some View {
        HStack(spacing: 14) {
            Image(systemName: nav.rerouting
                ? "arrow.triangle.2.circlepath"
                : "arrow.triangle.turn.up.right.diamond.fill")
                .font(.title)
                .foregroundStyle(.white)
                .symbolEffect(.pulse, isActive: nav.rerouting)
            VStack(alignment: .leading, spacing: 2) {
                if !nav.rerouting, nav.distanceToNext > 0 {
                    Text("\(Int(nav.distanceToNext)) m").font(.title3.weight(.bold)).foregroundStyle(.white)
                }
                Text(nav.instruction)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white)
                    .lineLimit(2)
            }
            Spacer()
        }
        .padding(16)
        .background((nav.rerouting ? Color.orange : Color.green).gradient, in: RoundedRectangle(cornerRadius: 18))
        .shadow(color: .black.opacity(0.2), radius: 8, y: 4)
    }

    private var etaCard: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(String(format: "%.0f min", nav.etaMinutes)).font(.title3.weight(.bold))
                Text(String(format: "%.1f km restants", nav.remainingKm))
                    .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Button { nav.stop() } label: {
                Text("Quitter")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.vertical, 10).padding(.horizontal, 18)
                    .background(Color.red, in: Capsule())
            }
        }
        .padding(14)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        .shadow(color: .black.opacity(0.15), radius: 6, y: 3)
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

    /// Heure d'arrivée estimée (maintenant + durée).
    private func routeArrival(_ minutes: Double) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.dateFormat = "HH:mm"
        return f.string(from: Date().addingTimeInterval(minutes * 60))
    }

    /// Étiquettes d'un itinéraire (le plus simple / rapide / court).
    private func routeTags(_ o: RouteOption, fastestID: UUID?, shortestID: UUID?) -> [RouteTag] {
        var tags: [RouteTag] = []
        if o.isSimplest { tags.append(RouteTag(text: "Le plus simple", color: .green)) }
        if o.id == fastestID { tags.append(RouteTag(text: "Le plus rapide", color: .blue)) }
        if o.id == shortestID { tags.append(RouteTag(text: "Le plus court", color: .orange)) }
        if tags.isEmpty { tags.append(RouteTag(text: "Alternative", color: .gray)) }
        return tags
    }

    /// Ligne d'infos détaillées d'un itinéraire : durée, distance, virages, arrivée.
    private func routeDetails(_ o: RouteOption) -> some View {
        HStack(spacing: 12) {
            Label(String(format: "%.0f min", o.minutes), systemImage: "clock")
            Label(String(format: "%.1f km", o.km), systemImage: "ruler")
            Label("\(o.turns) virages", systemImage: "arrow.triangle.turn.up.right.diamond")
            Label("arr. \(routeArrival(o.minutes))", systemImage: "flag.checkered")
        }
        .font(.caption2)
        .foregroundStyle(.secondary)
        .lineLimit(1)
        .minimumScaleFactor(0.7)
    }

    /// Carte de choix d'itinéraire : bandeau compact (itinéraire choisi) qu'on
    /// déroule pour comparer toutes les infos, puis « Démarrer ».
    private var routeOptionsCard: some View {
        let selected = routeOptions.first { $0.id == selectedRouteID } ?? routeOptions.first
        let fastestID = routeOptions.min { $0.minutes < $1.minutes }?.id
        let shortestID = routeOptions.min { $0.km < $1.km }?.id
        return VStack(alignment: .leading, spacing: 8) {
            // En-tête = menu déroulant montrant l'itinéraire sélectionné.
            Button {
                withAnimation { routesExpanded.toggle() }
            } label: {
                HStack(spacing: 10) {
                    Circle().fill(selected?.color ?? .green).frame(width: 12, height: 12)
                    if let o = selected {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(routeTags(o, fastestID: fastestID, shortestID: shortestID).first?.text ?? "Itinéraire")
                                .font(.subheadline.weight(.semibold)).lineLimit(1)
                            routeDetails(o)
                        }
                    }
                    Spacer(minLength: 6)
                    Image(systemName: routesExpanded ? "chevron.up" : "chevron.down")
                        .font(.subheadline.weight(.bold)).foregroundStyle(.secondary)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            // Liste déroulée : tous les itinéraires avec toutes leurs infos.
            if routesExpanded {
                ScrollView {
                    VStack(spacing: 4) {
                        ForEach(Array(routeOptions.enumerated()), id: \.element.id) { idx, o in
                            let isSel = o.id == selectedRouteID
                            Button {
                                withAnimation { selectedRouteID = o.id; routesExpanded = false }
                            } label: {
                                HStack(spacing: 10) {
                                    VStack(spacing: 2) {
                                        Circle().fill(o.color).frame(width: 12, height: 12)
                                        Text("#\(idx + 1)").font(.caption2).foregroundStyle(.secondary)
                                    }
                                    VStack(alignment: .leading, spacing: 4) {
                                        // Étiquettes (le plus simple / rapide / court).
                                        HStack(spacing: 4) {
                                            ForEach(routeTags(o, fastestID: fastestID, shortestID: shortestID)) { tag in
                                                Text(tag.text)
                                                    .font(.caption2.weight(.semibold))
                                                    .foregroundStyle(tag.color)
                                                    .padding(.vertical, 2).padding(.horizontal, 6)
                                                    .background(tag.color.opacity(0.14), in: Capsule())
                                            }
                                        }
                                        routeDetails(o)
                                    }
                                    Spacer(minLength: 6)
                                    Image(systemName: isSel ? "checkmark.circle.fill" : "circle")
                                        .foregroundStyle(isSel ? o.color : .secondary)
                                }
                                .contentShape(Rectangle())
                                .padding(.vertical, 6).padding(.horizontal, 8)
                                .background(
                                    RoundedRectangle(cornerRadius: 10)
                                        .fill(isSel ? o.color.opacity(0.12) : .clear))
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .frame(maxHeight: 260)
            }

            HStack(spacing: 10) {
                Button {
                    if let o = selected { startNavigation(option: o) }
                } label: {
                    Label("Démarrer", systemImage: "location.north.line.fill")
                        .font(.subheadline.weight(.semibold))
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(.green)
                .disabled(selected == nil)

                Button { previewRoute() } label: {
                    Image(systemName: previewing ? "stop.fill" : "eye.fill")
                        .font(.subheadline.weight(.semibold))
                        .frame(width: 40, height: 34)
                }
                .buttonStyle(.bordered)
                .tint(.blue)
                .disabled(selected == nil)

                Button {
                    previewTask?.cancel()
                    previewing = false
                    routeOptions = []
                    selectedRouteID = nil
                    routesExpanded = false
                    pendingDestination = nil
                } label: {
                    Image(systemName: "xmark")
                        .font(.subheadline.weight(.semibold))
                        .frame(width: 40, height: 34)
                }
                .buttonStyle(.bordered)
                .tint(.secondary)
            }
        }
        .padding(14)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        .shadow(color: .black.opacity(0.15), radius: 6, y: 3)
    }

    /// Carte listant les trajets simultanés affichés (avec leur couleur).
    private var multiRouteCard: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 6) {
                Text("\(multiRoutes.count) trajets affichés")
                    .font(.subheadline.weight(.semibold))
                ForEach(multiRoutes) { r in
                    HStack(spacing: 8) {
                        Circle().fill(r.color).frame(width: 10, height: 10)
                        Text(r.label).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            }
            Spacer()
            Button { multiRoutes = [] } label: {
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
                map3DButton
                circleButton(system: "location.fill", tint: .blue) { recenter() }
                reportButton
            }
        }
    }

    /// Bascule 2D / 3D de la carte.
    private var map3DButton: some View {
        Button { toggle3D() } label: {
            Image(systemName: "view.3d")
                .font(.title3)
                .foregroundStyle(is3D ? Color.white : .blue)
                .frame(width: 48, height: 48)
                .background {
                    if is3D {
                        Circle().fill(Color.blue)
                    } else {
                        Circle().fill(.regularMaterial)
                    }
                }
                .shadow(color: .black.opacity(0.15), radius: 5, y: 2)
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

    // MARK: - Feuille « Destination » (recherche d'adresse/lieu)

    private var searchSheet: some View {
        NavigationStack {
            List {
                // Résultats de la recherche en cours.
                ForEach(Array(placeSearch.results.enumerated()), id: \.offset) { _, item in
                    Button {
                        startSearchNavigation(to: item)
                    } label: {
                        HStack {
                            Image(systemName: "mappin.circle.fill").foregroundStyle(.red)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(item.name ?? "Destination").font(.headline)
                                if let sub = item.placemark.title {
                                    Text(sub).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                }
                            }
                            Spacer()
                            Label("Y aller", systemImage: "location.north.line.fill")
                                .labelStyle(.iconOnly)
                                .foregroundStyle(.green)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(location.coordinate == nil)
                }

                // Champ vide : on propose les recherches récentes.
                if placeSearch.results.isEmpty {
                    if recents.isEmpty {
                        Text("Tape une adresse ou un lieu, puis choisis une destination.")
                            .foregroundStyle(.secondary)
                    } else {
                        Section {
                            ForEach(recents) { r in
                                Button {
                                    goToRecent(r)
                                } label: {
                                    HStack {
                                        Image(systemName: "clock.arrow.circlepath")
                                            .foregroundStyle(.secondary)
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(r.name).font(.headline)
                                            if !r.subtitle.isEmpty {
                                                Text(r.subtitle).font(.caption)
                                                    .foregroundStyle(.secondary).lineLimit(1)
                                            }
                                        }
                                        Spacer()
                                        Label("Y aller", systemImage: "location.north.line.fill")
                                            .labelStyle(.iconOnly)
                                            .foregroundStyle(.green)
                                    }
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .disabled(location.coordinate == nil)
                            }
                            .onDelete(perform: deleteRecents)
                        } header: {
                            HStack {
                                Text("Récentes")
                                Spacer()
                                Button("Tout effacer") {
                                    Session.clearRecents()
                                    recents = []
                                    Task { try? await api.clearSearches() }
                                }
                                .font(.caption)
                            }
                        }
                    }
                }
            }
            .navigationTitle("Où allez-vous ?")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(
                text: $destQuery,
                placement: .navigationBarDrawer(displayMode: .always),
                prompt: "Adresse ou lieu")
            .onChange(of: destQuery) { _, value in
                placeSearch.center = location.coordinate
                placeSearch.search(value)
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { showSearch = false }
                }
            }
        }
        .presentationDetents([.large])
    }

    /// Propose les itinéraires vers un résultat de recherche.
    private func startSearchNavigation(to item: MKMapItem) {
        let dest = item.placemark.coordinate
        destQuery = ""
        showSearch = false
        saveRecent(NewSearch(
            name: item.name ?? "Destination",
            subtitle: item.placemark.title ?? "",
            lat: dest.latitude, lon: dest.longitude))
        Task { await presentRouteOptions(to: dest) }
    }

    /// Relance un itinéraire vers une recherche récente (et la remonte en tête).
    private func goToRecent(_ r: RecentSearch) {
        destQuery = ""
        showSearch = false
        saveRecent(NewSearch(name: r.name, subtitle: r.subtitle, lat: r.lat, lon: r.lon))
        Task {
            await presentRouteOptions(
                to: CLLocationCoordinate2D(latitude: r.lat, longitude: r.lon))
        }
    }

    /// Charge les recherches récentes depuis le serveur, avec repli sur le cache
    /// local si le réseau est indisponible.
    private func loadRecents() async {
        if let remote = try? await api.searches() {
            recents = remote
            Session.recentSearches = remote
        } else {
            recents = Session.recentSearches
        }
    }

    /// Mémorise une recherche : mise à jour locale immédiate + envoi au serveur,
    /// puis rechargement pour refléter le dédoublonnage côté serveur.
    private func saveRecent(_ s: NewSearch) {
        // Optimiste : affichage immédiat même hors ligne.
        Session.addRecent(RecentSearch(
            name: s.name, subtitle: s.subtitle, lat: s.lat, lon: s.lon))
        recents = Session.recentSearches
        Task {
            _ = try? await api.saveSearch(s)
            await loadRecents()
        }
    }

    /// Supprime des recherches récentes (par balayage), en base et localement.
    private func deleteRecents(at offsets: IndexSet) {
        let removed = offsets.map { recents[$0] }
        var list = recents
        list.remove(atOffsets: offsets)
        Session.recentSearches = list
        recents = list
        Task {
            for r in removed {
                if let sid = r.serverId { try? await api.deleteSearch(id: sid) }
            }
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
                    Button {
                        Task {
                            await showRoutes(for: positions)
                            showPlaces = false
                        }
                    } label: {
                        Label("Un trajet vers chaque point", systemImage: "arrow.triangle.branch")
                    }
                    .disabled(location.coordinate == nil || positions.isEmpty)
                    Button {
                        Task {
                            await showRoutes(for: positions.filter { selectedIDs.contains($0.id) })
                            showPlaces = false
                        }
                    } label: {
                        Label(
                            "Afficher les trajets cochés (\(selectedIDs.count))",
                            systemImage: "checklist")
                    }
                    .disabled(location.coordinate == nil || selectedIDs.isEmpty)
                    Button(role: .destructive) {
                        Task { await deleteSelected() }
                    } label: {
                        Label(
                            "Supprimer les points cochés (\(selectedIDs.count))",
                            systemImage: "trash")
                    }
                    .disabled(selectedIDs.isEmpty)
                    Button {
                        showPlaces = false
                        showTrips = true
                        Task { await loadTrips() }
                    } label: {
                        Label("Mes trajets (\(trips.count))", systemImage: "clock.arrow.circlepath")
                    }
                }
                if let s = stats {
                    Section("Statistiques") {
                        Text("\(s.count) positions · \(String(format: "%.1f", s.total_km)) km")
                            .font(.subheadline)
                    }
                }
                Section("Destinations") {
                    if !positions.isEmpty {
                        Text("Coche des points pour tracer plusieurs trajets à la fois.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    ForEach(positions) { p in
                        HStack(spacing: 12) {
                            Button {
                                if selectedIDs.contains(p.id) {
                                    selectedIDs.remove(p.id)
                                } else {
                                    selectedIDs.insert(p.id)
                                }
                            } label: {
                                Image(systemName: selectedIDs.contains(p.id)
                                    ? "checkmark.circle.fill" : "circle")
                                    .imageScale(.large)
                                    .foregroundStyle(selectedIDs.contains(p.id) ? .blue : .secondary)
                            }
                            .buttonStyle(.borderless)

                            Button {
                                showPlaces = false
                                Task {
                                    await presentRouteOptions(
                                        to: CLLocationCoordinate2D(latitude: p.lat, longitude: p.lon))
                                }
                            } label: {
                                HStack {
                                    VStack(alignment: .leading) {
                                        Text(p.label).font(.headline)
                                        Text(String(format: "%.4f, %.4f", p.lat, p.lon))
                                            .font(.caption).foregroundStyle(.secondary)
                                    }
                                    Spacer()
                                    Label("Y aller", systemImage: "location.north.line.fill")
                                        .labelStyle(.iconOnly)
                                        .foregroundStyle(.green)
                                }
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .disabled(location.coordinate == nil)
                        }
                    }
                    .onDelete(perform: deletePositions)
                    if positions.isEmpty {
                        Text("Aucune position — utilise « Enregistrer ma position ».")
                            .foregroundStyle(.secondary)
                    }
                }
                Section("Mon avatar") {
                    HStack(spacing: 12) {
                        Image(Avatars.asset(avatar))
                            .resizable().scaledToFit()
                            .frame(width: 60, height: 60)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Avatar actuel").font(.subheadline.weight(.semibold))
                            Text(Avatars.labels[avatar] ?? avatar)
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 12) {
                            ForEach(Avatars.all, id: \.self) { id in
                                Image(Avatars.asset(id))
                                    .resizable()
                                    .scaledToFit()
                                    .frame(width: 54, height: 54)
                                    .padding(6)
                                    .background(
                                        RoundedRectangle(cornerRadius: 12)
                                            .stroke(avatar == id ? Color.blue : Color.gray.opacity(0.3),
                                                    lineWidth: avatar == id ? 3 : 1)
                                    )
                                    .onTapGesture {
                                        avatar = id
                                        Session.avatar = id
                                    }
                            }
                        }
                        .padding(.vertical, 4)
                    }
                }
                Section("Compte") {
                    Label(auth.username, systemImage: "person.crop.circle")
                    Button(role: .destructive) {
                        showPlaces = false
                        auth.logout()
                    } label: {
                        Label("Déconnexion", systemImage: "rectangle.portrait.and.arrow.right")
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

    // MARK: - Feuille « Mes trajets » (historique)

    private var tripsSheet: some View {
        NavigationStack {
            List {
                if trips.isEmpty {
                    Text("Aucun trajet enregistré pour l'instant. Lance une navigation : ton trajet sera sauvegardé automatiquement à l'arrivée.")
                        .foregroundStyle(.secondary)
                }
                ForEach(trips) { t in
                    Button {
                        showTrip(t)
                    } label: {
                        HStack {
                            Image(systemName: "map.fill").foregroundStyle(.indigo)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(t.label).font(.headline)
                                Text(tripSubtitle(t))
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer()
                            Image(systemName: "eye").foregroundStyle(.indigo)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                .onDelete(perform: deleteTrips)
            }
            .navigationTitle("Mes trajets")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { showTrips = false }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }

    /// Résumé d'un trajet : date, distance et durée.
    private func tripSubtitle(_ t: Trip) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.dateFormat = "d MMM yyyy, HH:mm"
        let when = f.string(from: t.date)
        return String(format: "%@ · %.1f km · %.0f min", when, t.distance_km, t.duration_min)
    }

    /// Affiche un trajet enregistré sur la carte.
    private func showTrip(_ t: Trip) {
        displayedTrip = t
        showTrips = false
        multiRoutes = []
        routeOptions = []
        routeInfo = nil
        let coords = t.coordinates.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
        fitCoordinates(coords)
    }

    private func deleteTrips(at offsets: IndexSet) {
        let ids = offsets.map { trips[$0].id }
        Task {
            for id in ids { try? await api.deleteTrip(id: id) }
            if let shown = displayedTrip, ids.contains(shown.id) {
                displayedTrip = nil
            }
            await loadTrips()
        }
    }

    // MARK: - Actions

    private func refresh() async {
        do {
            positions = try await api.positions()
            stats = try? await api.stats()
            await updateRoute()
        } catch APIError.unauthorized {
            auth.logout()
        } catch {
            // Réseau indisponible : on garde l'état courant.
        }
    }

    /// Calcule l'itinéraire routier le plus simple (vert) et son résumé.
    private func updateRoute() async {
        multiRoutes = []
        routeOptions = []
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

    /// Calcule et affiche un trajet vers chacun des points donnés, chacun
    /// d'une couleur différente (plusieurs trajets simultanés sur la carte).
    private func showRoutes(for points: [Position]) async {
        guard let from = location.coordinate, !points.isEmpty else { return }
        routeCoords = []
        routeInfo = nil
        routeOptions = []
        var routes: [ColoredRoute] = []
        for (i, p) in points.enumerated() {
            let to = CLLocationCoordinate2D(latitude: p.lat, longitude: p.lon)
            if let r = await drivingRoute(from: from, to: to) {
                routes.append(
                    ColoredRoute(
                        coordinates: r.polyline.coordinates,
                        color: routePalette[i % routePalette.count],
                        label: p.label))
            }
        }
        multiRoutes = routes
        var all = routes.flatMap { $0.coordinates }
        all.append(from)
        fitCoordinates(all)
    }

    /// Cadre la carte pour englober un ensemble de coordonnées.
    private func fitCoordinates(_ pts: [CLLocationCoordinate2D]) {
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

    /// Démarre la navigation depuis ma position vers une destination.
    private func startNavigation(to dest: Position) async {
        guard let from = location.coordinate else { return }
        multiRoutes = []
        let to = CLLocationCoordinate2D(latitude: dest.lat, longitude: dest.lon)
        guard let route = await drivingRoute(from: from, to: to) else { return }
        nav.start(route: route, destination: to)
        showPlaces = false
        // Vue « conduite » : la carte suit et tourne dans le sens de la marche.
        withAnimation {
            camera = .userLocation(followsHeading: true, fallback: .automatic)
        }
    }

    /// Calcule plusieurs itinéraires vers une destination et les propose :
    /// le plus simple (moins de virages) est mis en avant en vert.
    private func presentRouteOptions(to dest: CLLocationCoordinate2D) async {
        guard let from = location.coordinate else { return }
        multiRoutes = []
        routeCoords = []
        routeInfo = nil

        // 1) Alternatives directes d'Apple (souvent 2–3).
        var builds = await buildRoutes(from: from, to: dest, alternates: true)

        // 2) Diversifie via des points de passage décalés de part et d'autre du
        //    trajet direct, calculés en parallèle, pour obtenir une dizaine de choix.
        var vias: [CLLocationCoordinate2D] = []
        for f in [0.13, 0.27, 0.42] {
            for s in [1.0, -1.0] { vias.append(offsetVia(from: from, to: dest, fraction: f, side: s)) }
        }
        await withTaskGroup(of: RouteBuild?.self) { group in
            for via in vias {
                group.addTask {
                    guard let a = await buildRoutes(from: from, to: via, alternates: false).first,
                        let b = await buildRoutes(from: via, to: dest, alternates: false).first
                    else { return nil }
                    return RouteBuild(
                        coords: a.coords + b.coords, steps: a.steps + b.steps,
                        km: a.km + b.km, minutes: a.minutes + b.minutes)
                }
            }
            for await r in group { if let r { builds.append(r) } }
        }
        guard !builds.isEmpty else { return }

        // Dédoublonne (distance + point médian arrondis).
        var seen = Set<String>()
        var unique: [RouteBuild] = []
        for b in builds where b.coords.count >= 2 {
            let mid = b.coords[b.coords.count / 2]
            let key = String(format: "%.1f|%.3f,%.3f",
                             (b.km * 2).rounded() / 2, mid.latitude, mid.longitude)
            if seen.insert(key).inserted { unique.append(b) }
        }

        var options = unique.map { b in
            RouteOption(
                coordinates: b.coords, steps: b.steps,
                minutes: b.minutes, km: b.km,
                turns: max(b.steps.count - 1, 0))
        }
        // Le « plus simple » = le moins de manœuvres (puis le plus rapide).
        if let best = options.indices.min(by: {
            (options[$0].turns, options[$0].minutes) < (options[$1].turns, options[$1].minutes)
        }) {
            options[best].isSimplest = true
        }
        // Le plus simple d'abord, puis par durée ; on plafonne à 10.
        options.sort { ($0.isSimplest ? 0 : 1, $0.minutes) < ($1.isSimplest ? 0 : 1, $1.minutes) }
        options = Array(options.prefix(10))

        // Couleur par itinéraire : vert pour le plus simple, palette pour les autres.
        let altColors: [Color] = [.blue, .orange, .purple, .pink, .teal, .red, .brown, .cyan, .indigo]
        var alt = 0
        options = options.map { o in
            var oo = o
            if o.isSimplest {
                oo.color = .green
            } else {
                oo.color = altColors[alt % altColors.count]
                alt += 1
            }
            return oo
        }

        pendingDestination = dest
        routeOptions = options
        selectedRouteID = options.first { $0.isSimplest }?.id ?? options.first?.id
        routesExpanded = false
        fitCoordinates(options.flatMap { $0.coordinates } + [from])
    }

    /// Lance la navigation sur l'option d'itinéraire choisie.
    private func startNavigation(option: RouteOption) {
        guard let dest = pendingDestination else { return }
        previewTask?.cancel()
        previewing = false
        nav.start(steps: option.steps, coordinates: option.coordinates,
                  distanceKm: option.km, etaMinutes: option.minutes, destination: dest)
        routeOptions = []
        selectedRouteID = nil
        routesExpanded = false
        pendingDestination = nil
        withAnimation {
            if is3D, let c = location.coordinate {
                camera = .camera(MapCamera(centerCoordinate: c, distance: 500,
                                           heading: location.course, pitch: 60))
            } else {
                camera = .userLocation(followsHeading: true, fallback: .automatic)
            }
        }
    }

    /// Recalcule l'itinéraire depuis la position actuelle (sortie de route).
    private func recomputeRoute() async {
        guard let from = location.coordinate, let to = nav.destination else { return }
        if let route = await drivingRoute(from: from, to: to) {
            nav.applyReroute(route: route)
        }
    }

    /// Itinéraire voiture le plus simple entre deux points.
    private func drivingRoute(from: CLLocationCoordinate2D, to: CLLocationCoordinate2D) async -> MKRoute? {
        let request = MKDirections.Request()
        request.source = MKMapItem(placemark: MKPlacemark(coordinate: from))
        request.destination = MKMapItem(placemark: MKPlacemark(coordinate: to))
        request.transportType = .automobile
        request.requestsAlternateRoutes = false
        return try? await MKDirections(request: request).calculate().routes.first
    }

    private func recenter() {
        if nav.active {
            if is3D, let c = location.coordinate {
                withAnimation {
                    camera = .camera(MapCamera(
                        centerCoordinate: c, distance: 500,
                        heading: location.course, pitch: 60))
                }
            } else {
                withAnimation { camera = .userLocation(followsHeading: true, fallback: .automatic) }
            }
            return
        }
        guard let c = location.coordinate else { return }
        withAnimation { camera = userCamera(c) }
    }

    /// Caméra centrée sur un point, inclinée si le mode 3D est actif.
    private func userCamera(_ c: CLLocationCoordinate2D, distance: Double = 1200) -> MapCameraPosition {
        if is3D {
            return .camera(MapCamera(centerCoordinate: c, distance: distance, heading: 0, pitch: 60))
        }
        return .region(MKCoordinateRegion(
            center: c, latitudinalMeters: distance, longitudinalMeters: distance))
    }

    /// Active/désactive la vue 3D et l'applique immédiatement.
    private func toggle3D() {
        is3D.toggle()
        guard let c = location.coordinate else { return }
        if nav.active {
            withAnimation {
                camera = is3D
                    ? .camera(MapCamera(centerCoordinate: c, distance: 500,
                                        heading: location.course, pitch: 60))
                    : .userLocation(followsHeading: true, fallback: .automatic)
            }
        } else {
            withAnimation { camera = userCamera(c) }
        }
    }

    /// Cap (0 = nord) d'un point vers un autre.
    private func bearingBetween(_ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D) -> Double {
        let lat1 = a.latitude * .pi / 180, lat2 = b.latitude * .pi / 180
        let dLon = (b.longitude - a.longitude) * .pi / 180
        let y = sin(dLon) * cos(lat2)
        let x = cos(lat1) * sin(lat2) - sin(lat1) * cos(lat2) * cos(dLon)
        return (atan2(y, x) * 180 / .pi + 360).truncatingRemainder(dividingBy: 360)
    }

    /// Prévisualise (survole) l'itinéraire sélectionné, caméra le long du tracé.
    private func previewRoute() {
        if previewing {
            previewTask?.cancel()
            previewing = false
            return
        }
        guard let o = routeOptions.first(where: { $0.id == selectedRouteID }) ?? routeOptions.first
        else { return }
        let coords = o.coordinates
        guard coords.count > 1 else { return }
        previewing = true
        previewTask?.cancel()
        previewTask = Task { @MainActor in
            let n = coords.count
            let step = max(1, n / 60)
            var i = 0
            while i < n && !Task.isCancelled {
                let c = coords[i]
                let j = min(i + step, n - 1)
                let heading = bearingBetween(c, coords[j])
                withAnimation(.linear(duration: 0.3)) {
                    camera = .camera(MapCamera(
                        centerCoordinate: c, distance: 700,
                        heading: heading, pitch: is3D ? 70 : 45))
                }
                try? await Task.sleep(nanoseconds: 300_000_000)
                i += step
            }
            if !Task.isCancelled {
                withAnimation { fitCoordinates(coords) }
            }
            previewing = false
        }
    }

    private func toggleSharing() {
        sharing.toggle()
        if sharing, let c = location.coordinate {
            realtime.sendLive(lat: c.latitude, lon: c.longitude, label: driverName, avatar: avatar)
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
        Task {
            for id in ids { try? await api.delete(id: id) }
            selectedIDs.subtract(ids)
            await refresh()
        }
    }

    /// Supprime tous les points cochés.
    private func deleteSelected() async {
        let ids = Array(selectedIDs)
        for id in ids { try? await api.delete(id: id) }
        selectedIDs.removeAll()
        multiRoutes = []
        await refresh()
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
