import SwiftUI
import MapKit
import UIKit

private let alertTypes: [(category: String, emoji: String, label: String, color: Color)] = [
    ("police", "🚓", "Police", .blue),
    ("accident", "💥", "Accident", .red),
    ("bouchon", "🚧", "Bouchon", .orange),
    ("danger", "⚠️", "Danger", .yellow),
    ("vehicule", "🚘", "Véhicule arrêté", .gray),
    ("objet", "📦", "Objet sur la route", .brown),
    ("travaux", "🏗️", "Travaux", .yellow),
    ("brouillard", "🌫️", "Brouillard", .teal),
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
    /// Dénivelé positif (côte) et négatif (descente) en mètres, si connus.
    var climb: Double?
    var descent: Double?
    /// L'itinéraire emprunte des sections à péage.
    var hasTolls = false
}

/// Profil d'altitude d'une suite de points via l'API open-meteo (gratuite,
/// sans clé). Renvoie une altitude (m) par point, ou nil si indisponible.
fileprivate func elevationProfile(_ coords: [CLLocationCoordinate2D]) async -> [Double]? {
    guard !coords.isEmpty else { return nil }
    let lats = coords.map { String(format: "%.5f", $0.latitude) }.joined(separator: ",")
    let lons = coords.map { String(format: "%.5f", $0.longitude) }.joined(separator: ",")
    var comps = URLComponents(string: "https://api.open-meteo.com/v1/elevation")!
    comps.queryItems = [
        URLQueryItem(name: "latitude", value: lats),
        URLQueryItem(name: "longitude", value: lons),
    ]
    guard let url = comps.url,
        let (data, _) = try? await URLSession.shared.data(from: url)
    else { return nil }
    struct Resp: Decodable { let elevation: [Double] }
    return (try? JSONDecoder().decode(Resp.self, from: data))?.elevation
}

/// Échantillonne au plus `n` points répartis le long d'un tracé.
fileprivate func sampleCoords(_ coords: [CLLocationCoordinate2D], max n: Int)
    -> [CLLocationCoordinate2D]
{
    guard coords.count > n, n > 1 else { return coords }
    let step = Double(coords.count - 1) / Double(n - 1)
    return (0..<n).map { coords[Int((Double($0) * step).rounded())] }
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
    var hasTolls = false
}

/// Préférences d'itinéraire voiture (péages / autoroutes), lues au moment du
/// calcul depuis les réglages persistés.
fileprivate func routePreferences(_ req: MKDirections.Request) {
    guard req.transportType == .automobile else { return }
    if UserDefaults.standard.bool(forKey: "moncap.avoidTolls") {
        req.tollPreference = .avoid
    }
    if UserDefaults.standard.bool(forKey: "moncap.avoidHighways") {
        req.highwayPreference = .avoid
    }
}

/// Calcule un ou plusieurs itinéraires entre deux points, selon le mode.
fileprivate func buildRoutes(
    from: CLLocationCoordinate2D, to: CLLocationCoordinate2D, alternates: Bool,
    transportType: MKDirectionsTransportType
) async -> [RouteBuild] {
    let req = MKDirections.Request()
    req.source = MKMapItem(placemark: MKPlacemark(coordinate: from))
    req.destination = MKMapItem(placemark: MKPlacemark(coordinate: to))
    req.transportType = transportType
    req.requestsAlternateRoutes = alternates
    routePreferences(req)
    guard let routes = try? await MKDirections(request: req).calculate().routes else { return [] }
    return routes.map { r in
        RouteBuild(
            coords: r.polyline.coordinates,
            steps: r.steps.map {
                NavStep(text: $0.instructions,
                        coord: $0.polyline.coordinates.last ?? $0.polyline.coordinate)
            },
            km: r.distance / 1000,
            minutes: r.expectedTravelTime / 60,
            hasTolls: r.hasTolls)
    }
}

/// Itinéraire passant par un point de passage : deux tronçons assemblés.
fileprivate func viaRoute(
    from: CLLocationCoordinate2D, via: CLLocationCoordinate2D, to: CLLocationCoordinate2D,
    transportType: MKDirectionsTransportType
) async -> RouteBuild? {
    guard let a = await buildRoutes(from: from, to: via, alternates: false,
                                    transportType: transportType).first,
        let b = await buildRoutes(from: via, to: to, alternates: false,
                                  transportType: transportType).first
    else { return nil }
    // On retire l'« arrivée » du 1er tronçon et le « départ » du 2nd pour ne
    // pas annoncer « Vous êtes arrivé » au point de passage, tout en gardant
    // les instructions Apple (avec noms de rue) des deux tronçons.
    let steps = Array(a.steps.dropLast()) + Array(b.steps.dropFirst())
    return RouteBuild(
        coords: a.coords + b.coords, steps: steps,
        km: a.km + b.km, minutes: a.minutes + b.minutes,
        hasTolls: a.hasTolls || b.hasTolls)
}

/// Cap (0 = nord) d'un point vers un autre.
fileprivate func courseBetween(_ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D) -> Double {
    let lat1 = a.latitude * .pi / 180, lat2 = b.latitude * .pi / 180
    let dLon = (b.longitude - a.longitude) * .pi / 180
    let y = sin(dLon) * cos(lat2)
    let x = cos(lat1) * sin(lat2) - sin(lat1) * cos(lat2) * cos(dLon)
    return (atan2(y, x) * 180 / .pi + 360).truncatingRemainder(dividingBy: 360)
}

fileprivate func metersBetween(_ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D) -> Double {
    CLLocation(latitude: a.latitude, longitude: a.longitude)
        .distance(from: CLLocation(latitude: b.latitude, longitude: b.longitude))
}

/// Génère un virage-par-virage à partir d'un tracé, en détectant les
/// changements de cap à chaque sommet. Utile pour les itinéraires sans
/// instructions fournies (ex. vélo BRouter).
fileprivate func turnSteps(from coords: [CLLocationCoordinate2D]) -> [NavStep] {
    guard coords.count >= 3 else {
        return coords.last.map { [NavStep(text: "Continuez tout droit", coord: $0)] } ?? []
    }
    var steps: [NavStep] = []
    var lastStep = coords.first!
    for i in 1..<(coords.count - 1) {
        let inb = courseBetween(coords[i - 1], coords[i])
        let outb = courseBetween(coords[i], coords[i + 1])
        var delta = outb - inb
        if delta > 180 { delta -= 360 }
        if delta < -180 { delta += 360 }
        let mag = abs(delta)
        // Ignore les petites courbes et les sommets trop rapprochés.
        guard mag >= 30, metersBetween(lastStep, coords[i]) >= 20 else { continue }
        let side = delta > 0 ? "à droite" : "à gauche"
        let text: String
        if mag >= 105 { text = "Tournez fortement \(side)" }
        else if mag >= 55 { text = "Tournez \(side)" }
        else { text = "Légèrement \(side)" }
        steps.append(NavStep(text: text, coord: coords[i]))
        lastStep = coords[i]
    }
    steps.append(NavStep(text: "Arrivée à destination", coord: coords.last!))
    return steps
}

/// Itinéraires vélo réels (pistes cyclables) via BRouter — moteur open-source,
/// serveur public gratuit, sans clé API. Renvoie jusqu'à 3 alternatives.
fileprivate func bikeRoutes(
    from: CLLocationCoordinate2D, to: CLLocationCoordinate2D
) async -> [RouteBuild] {
    var out: [RouteBuild] = []
    var seen = Set<Int>()
    for alt in 0..<3 {
        guard let b = await brouterRoute(from: from, to: to, alt: alt) else { continue }
        // BRouter peut renvoyer le même tracé pour un idx sans alternative.
        let key = Int(b.km * 100)
        if seen.insert(key).inserted { out.append(b) }
    }
    return out
}

/// Un itinéraire vélo via BRouter (profil « trekking »), format GeoJSON.
fileprivate func brouterRoute(
    from: CLLocationCoordinate2D, to: CLLocationCoordinate2D, alt: Int
) async -> RouteBuild? {
    // Attention : BRouter attend lon,lat (et non lat,lon).
    let lonlats = String(
        format: "%.6f,%.6f|%.6f,%.6f",
        from.longitude, from.latitude, to.longitude, to.latitude)
    var comps = URLComponents(string: "https://brouter.de/brouter")!
    comps.queryItems = [
        URLQueryItem(name: "lonlats", value: lonlats),
        URLQueryItem(name: "profile", value: "trekking"),
        URLQueryItem(name: "alternativeidx", value: String(alt)),
        URLQueryItem(name: "format", value: "geojson"),
    ]
    guard let url = comps.url,
        let (data, _) = try? await URLSession.shared.data(from: url)
    else { return nil }

    struct FC: Decodable { let features: [Feature] }
    struct Feature: Decodable { let geometry: Geom; let properties: Props }
    struct Geom: Decodable { let coordinates: [[Double]] }
    struct Props: Decodable {
        let trackLength: String?
        let totalTime: String?
        enum CodingKeys: String, CodingKey {
            case trackLength = "track-length"
            case totalTime = "total-time"
        }
    }
    guard let fc = try? JSONDecoder().decode(FC.self, from: data),
        let f = fc.features.first
    else { return nil }
    let coords = f.geometry.coordinates.compactMap { c -> CLLocationCoordinate2D? in
        c.count >= 2 ? CLLocationCoordinate2D(latitude: c[1], longitude: c[0]) : nil
    }
    guard coords.count >= 2 else { return nil }
    let km = (Double(f.properties.trackLength ?? "") ?? 0) / 1000
    let minutes = (Double(f.properties.totalTime ?? "") ?? 0) / 60
    // Virage-par-virage généré depuis la géométrie du tracé.
    let steps = turnSteps(from: coords)
    return RouteBuild(
        coords: coords, steps: steps,
        km: km > 0 ? km : 0,
        minutes: minutes > 0 ? minutes : km / 16 * 60)
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

/// Catégorie de lieux à explorer sur la carte (fast-foods, hôtels, tourisme…).
enum POIKind: String, CaseIterable, Identifiable {
    case water, fastFood, restaurant, hotel, tourism, hangout
    var id: String { rawValue }

    var label: String {
        switch self {
        case .water: return "Baignade"
        case .fastFood: return "Fast-food"
        case .restaurant: return "Restaurants"
        case .hotel: return "Hôtels"
        case .tourism: return "Tourisme"
        case .hangout: return "Sorties"
        }
    }

    var emoji: String {
        switch self {
        case .water: return "🏊"
        case .fastFood: return "🍔"
        case .restaurant: return "🍽️"
        case .hotel: return "🏨"
        case .tourism: return "🏛️"
        case .hangout: return "🎉"
        }
    }

    /// Requête MKLocalSearch correspondante (les points d'eau passent par
    /// OpenStreetMap, pas par MKLocalSearch).
    var query: String {
        switch self {
        case .water: return ""
        case .fastFood: return "fast food"
        case .restaurant: return "restaurant"
        case .hotel: return "hôtel"
        case .tourism: return "site touristique"
        case .hangout: return "bar café"
        }
    }
}

/// Un lieu affiché sur la carte (résultat de recherche par catégorie).
struct POI: Identifiable {
    let id = UUID()
    let name: String
    let coordinate: CLLocationCoordinate2D
}

/// Mode de déplacement : à pied, à vélo, en voiture.
enum TravelMode: String, CaseIterable, Identifiable {
    case walk, bike, car
    var id: String { rawValue }

    /// MapKit ne calcule pas d'itinéraire vélo : on l'approxime avec le tracé
    /// piéton (petits chemins, pas d'autoroute) et une durée recalculée.
    var transportType: MKDirectionsTransportType {
        self == .car ? .automobile : .walking
    }

    /// Vitesse imposée pour recalculer la durée (vélo ≈ 16 km/h). nil = durée
    /// renvoyée par MapKit (voiture, marche).
    var speedKmh: Double? { self == .bike ? 16 : nil }

    var icon: String {
        switch self {
        case .walk: return "figure.walk"
        case .bike: return "bicycle"
        case .car: return "car.fill"
        }
    }

    var label: String {
        switch self {
        case .walk: return "À pied"
        case .bike: return "Vélo"
        case .car: return "Voiture"
        }
    }
}

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
    @StateObject private var speedLimit = SpeedLimitService()
    @StateObject private var fuel = FuelPriceService()
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
    /// Mode de déplacement choisi (à pied / vélo / voiture).
    @State private var travelMode: TravelMode = .car
    /// Catégorie de lieux affichée sur la carte (nil = aucune).
    @State private var poiKind: POIKind?
    /// Lieux trouvés pour la catégorie choisie.
    @State private var pois: [POI] = []
    /// Région actuellement visible (pour chercher les lieux dans la zone).
    @State private var visibleRegion: MKCoordinateRegion?
    /// Région du dernier chargement de lieux (anti-rechargements inutiles).
    @State private var lastPOIRegion: MKCoordinateRegion?
    /// Lieu Apple natif touché directement sur la carte (resto, hôtel, musée…).
    @State private var selectedFeature: MapFeature?
    /// Signalement touché sur la carte (pour voter 👍/👎).
    @State private var alertToVote: Alert?
    /// Favoris Domicile / Travail (stockés sur l'appareil).
    @State private var homePlace = Session.home
    @State private var workPlace = Session.work
    /// Texte d'ETA à partager (« J'arrive vers HH:MM »).
    @State private var etaShare: IdentifiableText?
    /// Dernier avertissement de dépassement de vitesse (anti-spam).
    @State private var lastSpeedWarning = Date.distantPast
    /// Première position reçue → mise à jour immédiate de la limite de vitesse.
    @State private var didInitialSpeedFetch = false
    /// Dernière recherche d'itinéraire plus rapide (toutes les 90 s).
    @State private var lastFasterCheck = Date.distantPast
    /// Infos du compte (points de contribution).
    @State private var accountInfo: AccountInfo?
    /// Classement des contributeurs.
    @State private var leaders: [LeaderEntry] = []
    /// Confirmation de suppression du compte.
    @State private var confirmDelete = false
    /// Préférences d'itinéraire voiture.
    @AppStorage("moncap.avoidTolls") private var avoidTolls = false
    @AppStorage("moncap.avoidHighways") private var avoidHighways = false
    /// Mon véhicule : carburant et consommation (pour le coût des trajets).
    @AppStorage("moncap.fuelType") private var fuelType = "gazole"
    @AppStorage("moncap.consumption") private var consumption = 6.5
    /// Signalements déjà annoncés vocalement pendant cette navigation.
    @State private var announcedAlertIDs: Set<Int> = []
    /// Itinéraire actuellement prévisualisé (mis en avant) avant décision.
    @State private var selectedRouteID: UUID?
    /// Liste déroulée ou repliée (bandeau compact par défaut).
    @State private var routesExpanded = false
    @State private var pendingDestination: CLLocationCoordinate2D?
    /// Carte inclinée en 3D si l'utilisateur l'active.
    @State private var is3D = false
    /// Suivi automatique de la position en navigation (désactivé si on déplace
    /// la carte à la main ; réactivé via le bouton de recentrage).
    @State private var followsRoute = true
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
                    poiBar
                }
                Spacer()
            }
            .padding(.horizontal)
            .padding(.top, 8)

            // Bas : cartes d'info + barre d'actions. Les bandeaux ETA et
            // comparateur sont calés tout en bas (sous les boutons) pour
            // dégager la vue sur la route.
            VStack(spacing: 12) {
                if !nav.active {
                    if !multiRoutes.isEmpty {
                        multiRouteCard
                    } else if let info = routeInfo {
                        routeCard(info)
                    }
                }
                bottomBar
                if nav.active {
                    etaCard
                } else if !routeOptions.isEmpty {
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
        .sheet(item: $etaShare) { share in ShareSheet(items: [share.text]) }
        // Vote sur un signalement touché : toujours là / plus là.
        .confirmationDialog(
            alertToVote.map { "\(emoji(for: $0.category)) \($0.label.isEmpty ? $0.category : $0.label)" }
                ?? "Signalement",
            isPresented: Binding(
                get: { alertToVote != nil },
                set: { if !$0 { alertToVote = nil } }),
            titleVisibility: .visible
        ) {
            Button("👍 Toujours là") {
                if let a = alertToVote { Task { try? await api.voteAlert(id: a.id, up: true) } }
            }
            Button("👎 Plus là", role: .destructive) {
                if let a = alertToVote { Task { try? await api.voteAlert(id: a.id, up: false) } }
            }
            Button("Annuler", role: .cancel) {}
        } message: {
            if let a = alertToVote {
                Text("Confirmé \(a.confirms ?? 0) fois · infirmé \(a.denies ?? 0) fois")
            }
        }
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
                realtime.sendLive(lat: c.latitude, lon: c.longitude, label: driverName, avatar: liveAvatar)
            }
            // Limites de vitesse : mise à jour immédiate au lancement, puis en
            // continu (cache d'itinéraire d'abord, Overpass sinon) — même hors
            // navigation, comme Waze.
            if !didInitialSpeedFetch {
                didInitialSpeedFetch = true
                speedLimit.refreshNow(c)
            } else {
                speedLimit.update(c)
            }
            // Prix du carburant local (cache 6 h, pour le coût des trajets).
            fuel.refresh(near: c, type: fuelType)
            if location.speedKmh > 15 { checkOverspeed() }

            if nav.active {
                nav.update(c)
                recordTrackPoint(c)
                announceNearbyAlerts(from: c)
                maybeCheckFasterRoute(from: c)
                // Vue conduite 3D : la caméra suit, inclinée, dans le sens de la
                // marche — sauf si on a déplacé la carte à la main.
                if is3D, !previewing, followsRoute {
                    withAnimation(.easeOut(duration: 0.4)) {
                        camera = .camera(MapCamera(
                            centerCoordinate: c, distance: 500,
                            heading: location.course, pitch: 60))
                    }
                }
            }
        }
        .onChange(of: nav.active) { wasActive, isActive in
            // Empêche la mise en veille de l'écran pendant la navigation.
            UIApplication.shared.isIdleTimerDisabled = isActive
            // GPS + guidage vocal continuent en arrière-plan / écran verrouillé.
            location.setBackgroundTracking(isActive)
            // Fin de navigation : on vide le cache d'itinéraire mais on garde
            // la limite courante affichée.
            if !isActive { speedLimit.clearRoute() }
            // Fin de navigation (arrivée ou « Quitter ») → on enregistre le trajet.
            if wasActive && !isActive {
                Task { await saveRecordedTrip() }
            }
            // Début de navigation → nouvel enregistrement + suivi actif.
            if !wasActive && isActive {
                recordedTrack = location.coordinate.map { [$0] } ?? []
                tripStart = Date()
                followsRoute = true
                announcedAlertIDs = []
            }
        }
        .onChange(of: selectedRouteID) { _, id in
            // Prévient (vibration) quand l'itinéraire choisi a une grosse
            // côte / descente, si le dénivelé est déjà connu.
            if let o = routeOptions.first(where: { $0.id == id }), routeWarning(o) != nil {
                warnHaptic()
            }
        }
        .onChange(of: selectedFeature) { _, feature in
            // Un lieu Apple touché sur la carte → propose les itinéraires.
            guard let feature, !nav.active else { return }
            selectedFeature = nil
            Task { await presentRouteOptions(to: feature.coordinate) }
        }
        .onChange(of: avoidTolls) { _, _ in reroutePreferencesChanged() }
        .onChange(of: avoidHighways) { _, _ in reroutePreferencesChanged() }
        .onChange(of: fuelType) { _, newType in
            if let c = location.coordinate { fuel.refresh(near: c, type: newType) }
        }
    }

    /// Recalcule les itinéraires affichés quand une préférence change.
    private func reroutePreferencesChanged() {
        guard !nav.active, let dest = pendingDestination else { return }
        Task { await presentRouteOptions(to: dest) }
    }

    /// Cherche périodiquement (90 s) un itinéraire plus rapide vers la
    /// destination (mode voiture) et l'applique s'il fait gagner ≥ 2 min.
    private func maybeCheckFasterRoute(from c: CLLocationCoordinate2D) {
        guard travelMode == .car, !nav.rerouting,
            Date().timeIntervalSince(lastFasterCheck) > 90,
            let dest = nav.destination
        else { return }
        lastFasterCheck = Date()
        Task {
            guard let route = await drivingRoute(from: c, to: dest, transportType: .automobile)
            else { return }
            let saved = nav.etaMinutes - route.expectedTravelTime / 60
            if saved >= 2, nav.active {
                nav.applyFasterRoute(route: route, savedMinutes: Int(saved.rounded()))
                // Les limites de vitesse suivent le nouveau tracé.
                speedLimit.preload(along: route.polyline.coordinates)
            }
        }
    }

    /// Vrai si la vitesse actuelle dépasse la limite connue (marge 5 km/h).
    private var isOverspeeding: Bool {
        guard let lim = speedLimit.limitKmh else { return false }
        return location.speedKmh > Double(lim) + 5
    }

    /// Avertit (vibration + voix) en cas de dépassement, au plus toutes les 20 s.
    private func checkOverspeed() {
        guard isOverspeeding, let lim = speedLimit.limitKmh,
            Date().timeIntervalSince(lastSpeedWarning) > 20
        else { return }
        lastSpeedWarning = Date()
        warnHaptic()
        nav.announce("Attention, vous dépassez la limite de \(lim) kilomètres heure.")
    }

    /// Grade façon Waze selon les points de contribution.
    private func rankName(_ points: Int) -> String {
        switch points {
        case ..<50: return "Débutant"
        case ..<200: return "Éclaireur"
        case ..<500: return "Navigateur"
        case ..<1000: return "Capitaine"
        default: return "Légende de la route"
        }
    }

    /// Compose et partage l'heure d'arrivée estimée.
    private func shareETA() {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.dateFormat = "HH:mm"
        let eta = f.string(from: Date().addingTimeInterval(nav.etaMinutes * 60))
        etaShare = IdentifiableText(text: String(
            format: "J'arrive vers %@ (%.1f km restants) — MonCap GPS 🛰️",
            eta, nav.remainingKm))
    }

    /// Annonce vocalement (une seule fois chacun) les signalements à moins de
    /// 800 m pendant la navigation.
    private func announceNearbyAlerts(from c: CLLocationCoordinate2D) {
        for a in realtime.alerts where !announcedAlertIDs.contains(a.id) {
            let d = CLLocation(latitude: c.latitude, longitude: c.longitude)
                .distance(from: CLLocation(latitude: a.lat, longitude: a.lon))
            guard d < 800 else { continue }
            announcedAlertIDs.insert(a.id)
            let what = a.label.isEmpty ? a.category : a.label
            let meters = max(50, Int((d / 50).rounded()) * 50)
            nav.announce("Attention : \(what) à \(meters) mètres.")
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
        Map(position: $camera, selection: $selectedFeature) {
            UserAnnotation()
            // Mon avatar affiché à ma position (hors navigation).
            if let me = location.coordinate, !nav.active {
                Annotation("Moi", coordinate: me) {
                    avatarMarker(liveAvatar, size: 40)
                }
            }
            ForEach(positions) { p in
                Marker(p.label, coordinate: .init(latitude: p.lat, longitude: p.lon))
            }
            // Lieux de la catégorie choisie : un appui propose les itinéraires.
            if !nav.active {
                ForEach(pois) { p in
                    Annotation(p.name, coordinate: p.coordinate) {
                        Button {
                            Task { await presentRouteOptions(to: p.coordinate) }
                        } label: {
                            ZStack {
                                Circle().fill(.white)
                                    .frame(width: 34, height: 34)
                                    .shadow(radius: 2)
                                Text(poiKind?.emoji ?? "📍").font(.body)
                            }
                        }
                        .buttonStyle(.plain)
                    }
                }
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
                // Aperçu par itinéraire : bulle durée + coût au milieu du
                // tracé, tapable pour sélectionner l'itinéraire.
                ForEach(routeOptions.filter { $0.coordinates.count >= 2 }) { o in
                    Annotation("", coordinate: o.coordinates[o.coordinates.count / 2]) {
                        let isSel = o.id == selectedRouteID
                        Button {
                            withAnimation { selectedRouteID = o.id }
                        } label: {
                            VStack(spacing: 0) {
                                Text(String(format: "%.0f min", o.minutes))
                                    .font(.caption.weight(.bold))
                                if let cost = routeCostShort(o) {
                                    Text(cost).font(.caption2.weight(.semibold))
                                }
                            }
                            .foregroundStyle(isSel ? .white : .primary)
                            .padding(.vertical, 4)
                            .padding(.horizontal, 8)
                            .background(
                                isSel ? AnyShapeStyle(o.color) : AnyShapeStyle(.regularMaterial),
                                in: Capsule())
                            .overlay(Capsule().stroke(o.color, lineWidth: 2))
                            .shadow(color: .black.opacity(0.2), radius: 3, y: 1)
                        }
                        .buttonStyle(.plain)
                    }
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
                    avatarMarker(u.avatar, size: 44)
                }
            }
            ForEach(realtime.alerts) { a in
                Annotation(a.label.isEmpty ? a.category : a.label,
                           coordinate: .init(latitude: a.lat, longitude: a.lon)) {
                    // Toucher un signalement permet de le confirmer/infirmer.
                    Button { alertToVote = a } label: {
                        ZStack(alignment: .topTrailing) {
                            ZStack {
                                Circle().fill(.white).frame(width: 34, height: 34).shadow(radius: 2)
                                Text(emoji(for: a.category)).font(.body)
                            }
                            if let up = a.confirms, up > 0 {
                                Text("\(up)")
                                    .font(.caption2.weight(.bold))
                                    .foregroundStyle(.white)
                                    .padding(.horizontal, 4).padding(.vertical, 1)
                                    .background(Color.green, in: Capsule())
                                    .offset(x: 8, y: -6)
                            }
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .mapControls { MapCompass() }
        // Tous les lieux Apple (restos, hôtels, musées…) visibles en permanence
        // sur la carte, et tapables pour y aller.
        .mapStyle(.standard(pointsOfInterest: .all))
        // Suit la zone visible ; recharge les lieux de la catégorie active
        // quand on déplace la carte (fin de geste uniquement).
        .onMapCameraChange(frequency: .onEnd) { ctx in
            visibleRegion = ctx.region
            // Recharge les lieux seulement si la carte a vraiment bougé.
            if let kind = poiKind, !nav.active, regionChangedEnough(ctx.region) {
                Task { await loadPOIs(kind) }
            }
        }
        // Toucher (près d')un tracé le sélectionne directement sur la carte.
        .onTapGesture(coordinateSpace: .local) { point in
            handleMapTap(point, proxy: proxy)
        }
        // Déplacer la carte à la main débraye le suivi automatique.
        .simultaneousGesture(
            DragGesture(minimumDistance: 8).onChanged { _ in
                if followsRoute { followsRoute = false }
            }
        )
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

    /// Avatar diffusé/affiché selon le mode : voiture = avatar choisi ;
    /// à pied / à vélo = pictogramme dédié (pas la voiture).
    private var liveAvatar: String {
        switch travelMode {
        case .car: return avatar
        case .walk: return "walk"
        case .bike: return "bike"
        }
    }

    /// Marqueur d'un utilisateur sur la carte : image d'avatar, ou pictogramme
    /// piéton / vélo selon l'identifiant spécial.
    @ViewBuilder
    private func avatarMarker(_ id: String, size: CGFloat) -> some View {
        if id == "walk" {
            Image(systemName: "figure.walk")
                .font(.system(size: size * 0.52, weight: .bold))
                .foregroundStyle(.white)
                .frame(width: size, height: size)
                .background(Color.green, in: Circle())
                .shadow(radius: 2)
        } else if id == "bike" {
            Image(systemName: "bicycle")
                .font(.system(size: size * 0.5, weight: .bold))
                .foregroundStyle(.white)
                .frame(width: size, height: size)
                .background(Color.blue, in: Circle())
                .shadow(radius: 2)
        } else {
            Image(Avatars.asset(id))
                .resizable()
                .scaledToFit()
                .frame(width: size, height: size)
                .shadow(radius: 2)
        }
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

    /// Pastilles de catégories de lieux (fast-food, hôtels, tourisme…).
    private var poiBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(POIKind.allCases) { kind in
                    let isOn = poiKind == kind
                    Button {
                        togglePOIKind(kind)
                    } label: {
                        HStack(spacing: 5) {
                            Text(kind.emoji)
                            Text(kind.label).font(.caption.weight(.semibold))
                            if isOn && !pois.isEmpty {
                                Text("\(pois.count)")
                                    .font(.caption2.weight(.bold))
                                    .foregroundStyle(.white)
                                    .padding(.horizontal, 6).padding(.vertical, 1)
                                    .background(Color.blue, in: Capsule())
                            }
                        }
                        .padding(.vertical, 8)
                        .padding(.horizontal, 12)
                        .background(isOn ? AnyShapeStyle(Color.blue.opacity(0.18))
                                         : AnyShapeStyle(.regularMaterial),
                                    in: Capsule())
                        .overlay(Capsule().stroke(isOn ? Color.blue : .clear, lineWidth: 1.5))
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.vertical, 2)
        }
    }

    /// Active/désactive une catégorie de lieux et charge les résultats.
    private func togglePOIKind(_ kind: POIKind) {
        if poiKind == kind {
            poiKind = nil
            pois = []
            lastPOIRegion = nil
            return
        }
        poiKind = kind
        lastPOIRegion = nil
        Task { await loadPOIs(kind) }
    }

    /// Vrai si la carte a suffisamment bougé pour justifier un rechargement
    /// des lieux (≥ 25 % de déplacement ou changement de zoom net).
    private func regionChangedEnough(_ r: MKCoordinateRegion) -> Bool {
        guard let last = lastPOIRegion else { return true }
        let movedLat = abs(r.center.latitude - last.center.latitude)
            > last.span.latitudeDelta * 0.25
        let movedLon = abs(r.center.longitude - last.center.longitude)
            > last.span.longitudeDelta * 0.25
        let zoomed = r.span.latitudeDelta > last.span.latitudeDelta * 1.6
            || r.span.latitudeDelta < last.span.latitudeDelta / 1.6
        return movedLat || movedLon || zoomed
    }

    /// Cherche les lieux de la catégorie dans la zone visible de la carte.
    /// En cas d'échec réseau, les épingles existantes sont conservées.
    private func loadPOIs(_ kind: POIKind) async {
        let region = visibleRegion
            ?? location.coordinate.map {
                MKCoordinateRegion(center: $0, latitudinalMeters: 4000, longitudinalMeters: 4000)
            }
        guard let region else { return }

        // Baignade : plages, lacs, piscines… d'OpenStreetMap.
        if kind == .water {
            // Échec réseau → on garde ce qui est affiché.
            guard let found = await fetchSwimmingSpots(in: region) else { return }
            guard poiKind == kind else { return }
            pois = found
            lastPOIRegion = region
            return
        }

        let req = MKLocalSearch.Request()
        req.naturalLanguageQuery = kind.query
        req.region = region
        req.resultTypes = .pointOfInterest
        // Échec réseau → on garde ce qui est affiché.
        guard let resp = try? await MKLocalSearch(request: req).start() else { return }
        // La catégorie a pu changer pendant la recherche.
        guard poiKind == kind else { return }
        pois = resp.mapItems.prefix(25).map {
            POI(name: $0.name ?? "Lieu", coordinate: $0.placemark.coordinate)
        }
        lastPOIRegion = region
    }

    /// Lieux de baignade (OpenStreetMap) dans la zone visible : plages, zones
    /// de baignade en lac/rivière, piscines publiques, parcs aquatiques 🏊.
    /// Renvoie nil en cas d'échec réseau (pour conserver l'affichage actuel).
    private func fetchSwimmingSpots(in region: MKCoordinateRegion) async -> [POI]? {
        let south = region.center.latitude - region.span.latitudeDelta / 2
        let north = region.center.latitude + region.span.latitudeDelta / 2
        let west = region.center.longitude - region.span.longitudeDelta / 2
        let east = region.center.longitude + region.span.longitudeDelta / 2
        let bbox = "(\(south),\(west),\(north),\(east))"
        // nwr = nœuds + chemins + relations ; `out center` fournit un point
        // représentatif pour les surfaces (plages, lacs…).
        let q = "[out:json][timeout:12];("
            + "nwr[\"leisure\"=\"swimming_area\"]\(bbox);"
            + "nwr[\"natural\"=\"beach\"]\(bbox);"
            + "nwr[\"leisure\"=\"beach_resort\"]\(bbox);"
            + "nwr[\"leisure\"=\"water_park\"]\(bbox);"
            + "nwr[\"leisure\"=\"sports_centre\"][\"sport\"=\"swimming\"]\(bbox);"
            + ");out center 60;"
        // Miroir Overpass dédié, pour ne pas partager la limite de débit avec
        // les requêtes de limitations de vitesse.
        var comps = URLComponents(string: "https://overpass.kumi.systems/api/interpreter")!
        comps.queryItems = [URLQueryItem(name: "data", value: q)]
        guard let url = comps.url,
            let (data, _) = try? await URLSession.shared.data(from: url)
        else { return nil }
        struct Resp: Decodable { let elements: [Element] }
        struct Center: Decodable { let lat: Double; let lon: Double }
        struct Element: Decodable {
            let lat: Double?
            let lon: Double?
            let center: Center?
            let tags: [String: String]?
        }
        guard let resp = try? JSONDecoder().decode(Resp.self, from: data) else { return nil }
        var out: [POI] = []
        for el in resp.elements {
            let lat = el.lat ?? el.center?.lat
            let lon = el.lon ?? el.center?.lon
            guard let lat, let lon else { continue }
            let name = el.tags?["name"] ?? Self.swimName(fromTags: el.tags)
            out.append(POI(name: name,
                           coordinate: CLLocationCoordinate2D(latitude: lat, longitude: lon)))
        }
        return out
    }

    /// Libellé par défaut d'un lieu de baignade selon ses tags OSM.
    private static func swimName(fromTags tags: [String: String]?) -> String {
        if tags?["natural"] == "beach" { return "Plage" }
        if tags?["leisure"] == "swimming_area" { return "Baignade" }
        if tags?["leisure"] == "beach_resort" { return "Plage aménagée" }
        if tags?["leisure"] == "water_park" { return "Parc aquatique" }
        return "Piscine"
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
            // Partage de l'heure d'arrivée (« J'arrive vers HH:MM »).
            Button { shareETA() } label: {
                Image(systemName: "square.and.arrow.up")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.blue)
                    .frame(width: 40, height: 38)
                    .background(.thinMaterial, in: Capsule())
            }
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

    /// Infos principales d'un itinéraire : durée, distance, virages, arrivée.
    private func routeCoreText(_ o: RouteOption) -> String {
        String(format: "%.0f min · %.1f km · %d virages · arr. %@",
               o.minutes, o.km, o.turns, routeArrival(o.minutes))
    }

    /// Dénivelé (côte ↗ / descente ↘) si connu.
    private func routeElevationText(_ o: RouteOption) -> String? {
        guard let up = o.climb, let down = o.descent, up >= 5 || down >= 5 else { return nil }
        return String(format: "↗ %.0f m de côte · ↘ %.0f m de descente", up, down)
    }

    /// Seuil au-delà duquel une côte / descente est jugée « grosse » (mètres).
    private let bigElevation = 150.0

    /// Avertissement si l'itinéraire présente une grosse côte / descente.
    private func routeWarning(_ o: RouteOption) -> String? {
        let bigUp = (o.climb ?? 0) >= bigElevation
        let bigDown = (o.descent ?? 0) >= bigElevation
        switch (bigUp, bigDown) {
        case (true, true):
            return String(format: "⚠️ Forte côte (%.0f m) et forte descente (%.0f m)",
                          o.climb ?? 0, o.descent ?? 0)
        case (true, false):
            return String(format: "⚠️ Forte côte (%.0f m)", o.climb ?? 0)
        case (false, true):
            return String(format: "⚠️ Forte descente (%.0f m)", o.descent ?? 0)
        default:
            return nil
        }
    }

    /// Vibration d'alerte (grosse côte / descente).
    private func warnHaptic() {
        UINotificationFeedbackGenerator().notificationOccurred(.warning)
    }

    /// Coût estimé du trajet pour mon véhicule : carburant + péages.
    /// Les péages sont une estimation (part d'autoroute × ~0,078 €/km,
    /// tarif moyen français classe 1) — pas le tarif exact du concessionnaire.
    private func routeCosts(_ o: RouteOption) -> (fuel: Double, toll: Double)? {
        guard travelMode == .car else { return nil }
        let price = fuel.effectivePrice(type: fuelType)
        let fuelCost = o.km * consumption / 100 * price
        var toll = 0.0
        if o.hasTolls {
            // Part d'autoroute estimée d'après la vitesse moyenne.
            let avgKmh = o.minutes > 0 ? o.km / (o.minutes / 60) : 0
            toll = o.km * max(0, min(0.9, (avgKmh - 55) / 60)) * 0.078
        }
        return (fuelCost, toll)
    }

    /// Détail du coût (liste du comparateur).
    private func routeCostText(_ o: RouteOption) -> String? {
        guard let c = routeCosts(o) else { return nil }
        if c.toll > 0 {
            return String(format: "⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €",
                          c.fuel, c.toll, c.fuel + c.toll)
        }
        return String(format: "⛽ ~%.2f € · sans péage", c.fuel)
    }

    /// Coût total compact (« ~19,70 € ») pour les bulles sur la carte.
    private func routeCostShort(_ o: RouteOption) -> String? {
        guard let c = routeCosts(o) else { return nil }
        return String(format: "~%.2f €", c.fuel + c.toll)
    }

    /// Infos détaillées : ligne principale + dénivelé sur une ligne à part.
    private func routeDetails(_ o: RouteOption, compact: Bool = false) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(routeCoreText(o))
                .lineLimit(compact ? 1 : 2)
                .minimumScaleFactor(compact ? 0.6 : 0.85)
            if let elev = routeElevationText(o) {
                Text(elev)
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
            }
            if let cost = routeCostText(o) {
                Text(cost)
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
            }
            if let warning = routeWarning(o) {
                Text(warning)
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(.red)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .font(compact ? .caption : .caption.weight(.medium))
        .foregroundStyle(.primary)
        .fixedSize(horizontal: false, vertical: true)
    }

    /// Carte de choix d'itinéraire : bandeau compact (itinéraire choisi) qu'on
    /// déroule pour comparer toutes les infos, puis « Démarrer ».
    private var routeOptionsCard: some View {
        let selected = routeOptions.first { $0.id == selectedRouteID } ?? routeOptions.first
        let fastestID = routeOptions.min { $0.minutes < $1.minutes }?.id
        let shortestID = routeOptions.min { $0.km < $1.km }?.id
        return VStack(alignment: .leading, spacing: 8) {
            // Choix du mode : à pied / vélo / voiture (recalcule les itinéraires).
            Picker("Mode", selection: $travelMode) {
                ForEach(TravelMode.allCases) { m in
                    Label(m.label, systemImage: m.icon).tag(m)
                }
            }
            .pickerStyle(.segmented)
            .onChange(of: travelMode) { _, _ in
                if let dest = pendingDestination {
                    Task { await presentRouteOptions(to: dest) }
                }
            }

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
                            routeDetails(o, compact: true)
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
                                        Text("#\(idx + 1)").font(.caption2.weight(.semibold))
                                            .foregroundStyle(.primary)
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
            Text("\(Int(location.speedKmh))")
                .font(.title2.weight(.bold))
                .foregroundStyle(isOverspeeding ? .red : .primary)
            Text("km/h").font(.caption2).foregroundStyle(.secondary)
        }
        .frame(width: 70, height: 70)
        .background(.regularMaterial, in: Circle())
        .overlay(Circle().stroke(isOverspeeding ? Color.red : .clear, lineWidth: 3))
        .shadow(color: .black.opacity(0.15), radius: 6, y: 3)
        // Panneau de limitation (données OSM), affiché dès qu'il est connu.
        .overlay(alignment: .topTrailing) {
            if let lim = speedLimit.limitKmh {
                ZStack {
                    Circle().fill(.white)
                    Circle().stroke(Color.red, lineWidth: 4).padding(2)
                    Text("\(lim)")
                        .font(.caption.weight(.heavy))
                        .foregroundStyle(.black)
                        .minimumScaleFactor(0.6)
                }
                .frame(width: 36, height: 36)
                .shadow(radius: 2)
                .offset(x: 8, y: -8)
            }
        }
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
                // Mode de déplacement : à pied / vélo / voiture.
                Section {
                    Picker("Mode", selection: $travelMode) {
                        ForEach(TravelMode.allCases) { m in
                            Label(m.label, systemImage: m.icon).tag(m)
                        }
                    }
                    .pickerStyle(.segmented)
                    if travelMode == .car {
                        Toggle(isOn: $avoidTolls) {
                            Label("Éviter les péages", systemImage: "eurosign.circle")
                        }
                        Toggle(isOn: $avoidHighways) {
                            Label("Éviter les autoroutes", systemImage: "road.lanes")
                        }
                    }
                }
                // Raccourcis Domicile / Travail.
                Section("Favoris") {
                    favoriteRow(icon: "house.fill", title: "Domicile", place: homePlace) {
                        homePlace = $0
                        Session.home = $0
                    }
                    favoriteRow(icon: "briefcase.fill", title: "Travail", place: workPlace) {
                        workPlace = $0
                        Session.work = $0
                    }
                }
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

    /// Ligne de favori (Domicile / Travail) : y aller, définir, redéfinir, effacer.
    @ViewBuilder
    private func favoriteRow(
        icon: String, title: String, place: FavoritePlace?,
        set: @escaping (FavoritePlace?) -> Void
    ) -> some View {
        if let place {
            Button {
                showSearch = false
                Task {
                    await presentRouteOptions(
                        to: CLLocationCoordinate2D(latitude: place.lat, longitude: place.lon))
                }
            } label: {
                HStack {
                    Image(systemName: icon).foregroundStyle(.blue)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(title).font(.headline)
                        Text(place.label).font(.caption).foregroundStyle(.secondary).lineLimit(1)
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
            .contextMenu {
                Button {
                    if let c = location.coordinate {
                        set(FavoritePlace(lat: c.latitude, lon: c.longitude,
                                          label: "Défini sur ma position"))
                    }
                } label: {
                    Label("Redéfinir sur ma position", systemImage: "location.fill")
                }
                Button(role: .destructive) { set(nil) } label: {
                    Label("Effacer", systemImage: "trash")
                }
            }
        } else {
            Button {
                if let c = location.coordinate {
                    set(FavoritePlace(lat: c.latitude, lon: c.longitude,
                                      label: "Défini sur ma position"))
                }
            } label: {
                Label("Définir \(title) sur ma position", systemImage: icon)
            }
            .disabled(location.coordinate == nil)
        }
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
                Section("Mon véhicule") {
                    Picker("Carburant", selection: $fuelType) {
                        Text("Gazole").tag("gazole")
                        Text("SP95").tag("sp95")
                        Text("SP98").tag("sp98")
                        Text("E10").tag("e10")
                        Text("E85").tag("e85")
                    }
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text("Consommation")
                            Spacer()
                            Text(String(format: "%.1f L/100 km", consumption))
                                .foregroundStyle(.secondary)
                        }
                        Slider(value: $consumption, in: 3...15, step: 0.1)
                    }
                    HStack {
                        Label("Prix carburant", systemImage: "fuelpump")
                        Spacer()
                        Text(String(format: "%.3f €/L", fuel.effectivePrice(type: fuelType)))
                        Text(fuel.stationCount > 0
                            ? "(\(fuel.stationCount) stations)" : "(estimation)")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                if !leaders.isEmpty {
                    Section("Classement des contributeurs") {
                        ForEach(Array(leaders.enumerated()), id: \.offset) { i, e in
                            HStack {
                                Text(i == 0 ? "🥇" : i == 1 ? "🥈" : i == 2 ? "🥉" : "\(i + 1).")
                                    .frame(width: 34, alignment: .leading)
                                Text(e.name)
                                Spacer()
                                Text("\(e.points) pts")
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                Section("Compte") {
                    Label(auth.username, systemImage: "person.crop.circle")
                    if let info = accountInfo {
                        HStack {
                            Label("\(info.points) points · \(rankName(info.points))",
                                  systemImage: "trophy.fill")
                                .foregroundStyle(.orange)
                            Spacer()
                            Text("\(info.alerts) signalements · \(info.trips) trajets")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    Link(destination: api.privacyURL) {
                        Label("Politique de confidentialité", systemImage: "hand.raised")
                    }
                    Button(role: .destructive) {
                        showPlaces = false
                        auth.logout()
                    } label: {
                        Label("Déconnexion", systemImage: "rectangle.portrait.and.arrow.right")
                    }
                    Button(role: .destructive) {
                        confirmDelete = true
                    } label: {
                        Label("Supprimer mon compte", systemImage: "trash")
                    }
                    .confirmationDialog(
                        "Supprimer définitivement ton compte et toutes tes données (positions, trajets, recherches, signalements) ?",
                        isPresented: $confirmDelete, titleVisibility: .visible
                    ) {
                        Button("Tout supprimer définitivement", role: .destructive) {
                            Task {
                                try? await api.deleteAccount()
                                showPlaces = false
                                auth.logout()
                            }
                        }
                        Button("Annuler", role: .cancel) {}
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
        .presentationDetents([.medium])
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
            accountInfo = try? await api.accountInfo()
            leaders = (try? await api.leaderboard()) ?? leaders
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

        var builds: [RouteBuild]
        if travelMode == .bike {
            // Vélo : vrai routage cyclable (pistes) via BRouter, sans clé API.
            builds = await bikeRoutes(from: from, to: dest)
            if builds.isEmpty {
                // Repli si BRouter indisponible : tracé piéton, durée à ~16 km/h.
                builds = (await buildRoutes(from: from, to: dest, alternates: true,
                                            transportType: .walking))
                    .map {
                        RouteBuild(coords: $0.coords, steps: $0.steps, km: $0.km,
                                   minutes: $0.km / (travelMode.speedKmh ?? 16) * 60)
                    }
            }
        } else {
            let tt = travelMode.transportType
            // Alternatives directes d'Apple (souvent 2–3).
            builds = await buildRoutes(from: from, to: dest, alternates: true, transportType: tt)
            // En voiture, diversifie via des points de passage décalés de part et
            // d'autre du trajet direct, pour viser une quinzaine de choix (calcul
            // parallèle borné, anti-throttling). À pied, les alternatives directes
            // suffisent.
            if travelMode == .car {
                var vias: [CLLocationCoordinate2D] = []
                for f in [0.1, 0.22, 0.34, 0.46, 0.58, 0.7] {
                    for s in [1.0, -1.0] {
                        vias.append(offsetVia(from: from, to: dest, fraction: f, side: s))
                    }
                }
                await withTaskGroup(of: RouteBuild?.self) { group in
                    var next = 0
                    let maxConcurrent = min(6, vias.count)
                    while next < maxConcurrent {
                        let via = vias[next]; next += 1
                        group.addTask { await viaRoute(from: from, via: via, to: dest, transportType: tt) }
                    }
                    for await r in group {
                        if let r { builds.append(r) }
                        if next < vias.count {
                            let via = vias[next]; next += 1
                            group.addTask { await viaRoute(from: from, via: via, to: dest, transportType: tt) }
                        }
                    }
                }
            }
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
                turns: max(b.steps.count - 1, 0),
                hasTolls: b.hasTolls)
        }
        // Le « plus simple » = le moins de manœuvres (puis le plus rapide).
        if let best = options.indices.min(by: {
            (options[$0].turns, options[$0].minutes) < (options[$1].turns, options[$1].minutes)
        }) {
            options[best].isSimplest = true
        }
        // Le plus simple d'abord, puis par durée ; on plafonne à 15.
        options.sort { ($0.isSimplest ? 0 : 1, $0.minutes) < ($1.isSimplest ? 0 : 1, $1.minutes) }
        options = Array(options.prefix(15))

        // Couleur par itinéraire : vert pour le plus simple, palette pour les autres.
        let altColors: [Color] = [
            .blue, .orange, .purple, .pink, .teal, .red, .brown, .cyan, .indigo, .mint, .yellow,
        ]
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

        // Enrichit chaque itinéraire avec son dénivelé (en tâche de fond).
        Task { await loadElevations(for: options) }
    }

    /// Récupère le dénivelé (côte / descente) de chaque itinéraire et met à jour
    /// les infos au fur et à mesure.
    private func loadElevations(for options: [RouteOption]) async {
        for o in options {
            let sampled = sampleCoords(o.coordinates, max: 40)
            guard let elev = await elevationProfile(sampled), elev.count >= 2 else { continue }
            var up = 0.0, down = 0.0
            for i in 1..<elev.count {
                let d = elev[i] - elev[i - 1]
                if d > 1 { up += d } else if d < -1 { down += -d }
            }
            if let idx = routeOptions.firstIndex(where: { $0.id == o.id }) {
                routeOptions[idx].climb = up
                routeOptions[idx].descent = down
                // Prévient si l'itinéraire sélectionné se révèle avoir une
                // grosse côte / descente (dénivelé arrivé après coup).
                if routeOptions[idx].id == selectedRouteID,
                    routeWarning(routeOptions[idx]) != nil {
                    warnHaptic()
                }
            }
        }
    }

    /// Lance la navigation sur l'option d'itinéraire choisie.
    private func startNavigation(option: RouteOption) {
        guard let dest = pendingDestination else { return }
        previewTask?.cancel()
        previewing = false
        followsRoute = true
        // Précharge les limites de vitesse le long de l'itinéraire choisi.
        speedLimit.preload(along: option.coordinates)
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

    /// Recalcule l'itinéraire depuis la position actuelle (sortie de route),
    /// dans le mode de déplacement courant.
    private func recomputeRoute() async {
        guard let from = location.coordinate, let to = nav.destination else { return }
        if let route = await drivingRoute(from: from, to: to,
                                          transportType: travelMode.transportType) {
            nav.applyReroute(route: route)
            // Les limites de vitesse suivent le nouveau tracé.
            speedLimit.preload(along: route.polyline.coordinates)
        }
    }

    /// Itinéraire le plus simple entre deux points, selon le mode donné.
    private func drivingRoute(
        from: CLLocationCoordinate2D, to: CLLocationCoordinate2D,
        transportType: MKDirectionsTransportType = .automobile
    ) async -> MKRoute? {
        let request = MKDirections.Request()
        request.source = MKMapItem(placemark: MKPlacemark(coordinate: from))
        request.destination = MKMapItem(placemark: MKPlacemark(coordinate: to))
        request.transportType = transportType
        request.requestsAlternateRoutes = false
        routePreferences(request)
        return try? await MKDirections(request: request).calculate().routes.first
    }

    private func recenter() {
        followsRoute = true
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
        followsRoute = true
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
            realtime.sendLive(lat: c.latitude, lon: c.longitude, label: driverName, avatar: liveAvatar)
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
