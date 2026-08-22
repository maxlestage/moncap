import StoreKit
import SwiftUI
import MapKit
import UIKit
import UniformTypeIdentifiers

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
    /// Durée totale, arrêts aux étapes compris — modifiable quand un temps
    /// d'arrêt change sans que le tracé bouge.
    var minutes: Double
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
    /// Tronçons de transports en commun, pour l'onglet Transports. Présents,
    /// la carte d'option montre la chaîne des lignes et les horaires plutôt
    /// que kilomètres et virages — qui ne veulent rien dire dans un tram.
    var transit: [TransitLeg]?
    /// « 18:40 → 19:23 », heures réelles de départ et d'arrivée.
    var horaire: String?
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

/// Enchaîne des tronçons MapKit sur une suite de points — départ, étapes,
/// destination — en un seul trajet. À chaque étape, l'« arrivée » du tronçon
/// est remplacée par une consigne d'étape nommée, et le « départ » du tronçon
/// suivant est retiré : on ne dit pas « vous êtes arrivé » chez Paul quand on
/// ne fait qu'y passer.
fileprivate func chaineEtapes(
    points: [CLLocationCoordinate2D], noms: [String],
    transportType: MKDirectionsTransportType
) async -> RouteBuild? {
    guard points.count >= 2 else { return nil }
    var coords: [CLLocationCoordinate2D] = []
    var steps: [NavStep] = []
    var km = 0.0
    var minutes = 0.0
    var tolls = false
    for i in 0..<(points.count - 1) {
        guard
            let leg = await buildRoutes(
                from: points[i], to: points[i + 1],
                alternates: false, transportType: transportType
            ).first
        else { return nil }
        var legSteps = leg.steps
        if i > 0 { legSteps = Array(legSteps.dropFirst()) }
        if i < points.count - 2 {
            legSteps = Array(legSteps.dropLast())
            let nom = i < noms.count ? noms[i] : ""
            legSteps.append(
                NavStep(
                    text: String(
                        format: NSLocalizedString("Étape : %@", comment: ""), nom),
                    coord: points[i + 1]))
        }
        coords += leg.coords
        steps += legSteps
        km += leg.km
        minutes += leg.minutes
        tolls = tolls || leg.hasTolls
    }
    return RouteBuild(coords: coords, steps: steps, km: km, minutes: minutes, hasTolls: tolls)
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
    case fuel, parking, parkingAll, charging, toilets, water, surf, skate
    case pharmacy, atm, supermarket, camping, drinkingWater, viewpoint
    case fastFood, restaurant, hotel, tourism, hangout
    // La deuxième fournée (2026) : le quotidien — santé, commerces,
    // services — et la route (aires, gares, vélos). Toutes OpenStreetMap :
    // les tags sont stables, et l'annuaire hors ligne pourra les rejoindre
    // à la prochaine génération des annexes.
    case bakery, cafe, bars, bank, post, hospital, doctor, vet
    case carRepair, carWash, picnic, park, playground, trainStation
    case bikeRental, market, cinema, library, gym, grocery
    case hairdresser, recycling, restArea
    var id: String { rawValue }

    var label: String {
        switch self {
        case .fuel: return "Essence"
        case .parking: return "Handicapé"
        case .parkingAll: return "Parking"
        case .charging: return "Recharge"
        case .toilets: return "Toilettes"
        case .water: return "Baignade"
        case .surf: return "Surf"
        case .skate: return "Skatepark"
        case .pharmacy: return "Pharmacie"
        case .atm: return "Distributeur"
        case .supermarket: return "Supermarché"
        case .camping: return "Camping"
        case .drinkingWater: return "Eau potable"
        case .viewpoint: return "Point de vue"
        case .fastFood: return "Fast-food"
        case .restaurant: return "Restaurants"
        case .hotel: return "Hôtels"
        case .tourism: return "Tourisme"
        case .hangout: return "Sorties"
        case .bakery: return "Boulangerie"
        case .cafe: return "Café"
        case .bars: return "Bars"
        case .bank: return "Banque"
        case .post: return "Poste"
        case .hospital: return "Hôpital"
        case .doctor: return "Médecin"
        case .vet: return "Vétérinaire"
        case .carRepair: return "Garage"
        case .carWash: return "Lavage auto"
        case .picnic: return "Pique-nique"
        case .park: return "Parcs"
        case .playground: return "Aire de jeux"
        case .trainStation: return "Gares"
        case .bikeRental: return "Vélos"
        case .market: return "Marché"
        case .cinema: return "Cinéma"
        case .library: return "Bibliothèque"
        case .gym: return "Salle de sport"
        case .grocery: return "Épicerie"
        case .hairdresser: return "Coiffeur"
        case .recycling: return "Recyclage"
        case .restArea: return "Aire de repos"
        }
    }

    var emoji: String {
        switch self {
        case .fuel: return "⛽"
        case .parking: return "♿"
        case .parkingAll: return "🅿️"
        case .charging: return "⚡"
        case .toilets: return "🚻"
        case .water: return "🏊"
        case .surf: return "🏄"
        case .skate: return "🛹"
        case .pharmacy: return "💊"
        case .atm: return "🏧"
        case .supermarket: return "🛒"
        case .camping: return "🏕️"
        case .drinkingWater: return "🚰"
        case .viewpoint: return "⛰️"
        case .fastFood: return "🍔"
        case .restaurant: return "🍽️"
        case .hotel: return "🏨"
        case .tourism: return "🏛️"
        case .hangout: return "🎉"
        case .bakery: return "🥖"
        case .cafe: return "☕"
        case .bars: return "🍺"
        case .bank: return "🏦"
        case .post: return "📮"
        case .hospital: return "🏥"
        case .doctor: return "🩺"
        case .vet: return "🐾"
        case .carRepair: return "🔧"
        case .carWash: return "🧽"
        case .picnic: return "🧺"
        case .park: return "🌳"
        case .playground: return "🛝"
        case .trainStation: return "🚉"
        case .bikeRental: return "🚲"
        case .market: return "🍅"
        case .cinema: return "🎬"
        case .library: return "📚"
        case .gym: return "🏋️"
        case .grocery: return "🏪"
        case .hairdresser: return "💇"
        case .recycling: return "♻️"
        case .restArea: return "💤"
        }
    }

    /// Vrai pour les catégories interrogées via Overpass (OpenStreetMap) :
    /// l'aiguillage de `loadPOIs` s'appuie dessus, plutôt qu'une liste à
    /// tenir à jour à chaque catégorie ajoutée.
    var estOSM: Bool {
        switch self {
        case .fuel, .parking, .water, .fastFood, .restaurant, .hotel,
            .tourism, .hangout:
            return false
        default:
            return true
        }
    }

    /// Requête MKLocalSearch correspondante. Essence (open data) et les
    /// catégories OpenStreetMap passent par des sources dédiées ; seules les
    /// catégories « lieux Apple » utilisent une requête en langage naturel.
    var query: String {
        switch self {
        case .fastFood: return "fast food"
        case .restaurant: return "restaurant"
        case .hotel: return "hôtel"
        case .tourism: return "site touristique"
        case .hangout: return "bar café"
        default: return ""
        }
    }
}

/// Un lieu affiché sur la carte (résultat de recherche par catégorie).
struct POI: Identifiable {
    /// Identité STABLE (nom + position) : lors d'un rechargement, une épingle
    /// inchangée garde le même id, donc SwiftUI ne reconstruit que ce qui
    /// change au lieu de redessiner toutes les épingles (fluidité de la carte).
    var id: String { "\(name)|\(Int(coordinate.latitude * 1e5))|\(Int(coordinate.longitude * 1e5))" }
    let name: String
    let coordinate: CLLocationCoordinate2D
}

/// Mode de déplacement : à pied, à vélo, en voiture.
/// Le moment d'un trajet en transports en commun.
enum TransitQuand: String, CaseIterable, Identifiable {
    case maintenant, partirA, arriverA
    var id: String { rawValue }

    var libelle: String {
        switch self {
        case .maintenant: return "Maintenant"
        case .partirA: return "Partir à"
        case .arriverA: return "Arriver avant"
        }
    }
}

enum TravelMode: String, CaseIterable, Identifiable {
    case walk, bike, car, transit
    var id: String { rawValue }

    /// MapKit ne calcule pas d'itinéraire vélo : on l'approxime avec le tracé
    /// piéton (petits chemins, pas d'autoroute) et une durée recalculée.
    var transportType: MKDirectionsTransportType {
        self == .car ? .automobile : .walking
    }

    /// Vitesse imposée pour recalculer la durée (vélo ≈ 16 km/h). nil = durée
    /// renvoyée par MapKit (voiture, marche).
    var speedKmh: Double? { self == .bike ? 16 : nil }

    /// Couloir de sortie d'itinéraire, en mètres : au-delà, on recalcule.
    ///
    /// Un seul seuil valait pour tous les modes — 55 m, taillé pour la
    /// voiture. À pied, être à 55 m de son chemin, c'est être perdu depuis
    /// trois rues : le piéton mérite un couloir serré. Le vélo entre les
    /// deux. Les bornes restent au-dessus du bruit GPS urbain (10-20 m),
    /// l'anti-rebond de trois relevés faisant le reste.
    var seuilEcartM: Double {
        switch self {
        case .walk: return 30
        case .bike: return 40
        case .car, .transit: return 55
        }
    }

    /// Fonction de coût Valhalla pour le calcul sur l'appareil.
    ///
    /// Les trois modes se calculent sur les mêmes tuiles : le graphe porte
    /// tous les chemins, la fonction de coût choisit lesquels emprunter. Et
    /// contrairement à MapKit, Valhalla a un **vrai** profil vélo — pas une
    /// marche accélérée à 16 km/h.
    var costing: String {
        switch self {
        case .walk: return "pedestrian"
        case .bike: return "bicycle"
        // Réglages Mon véhicule : la « voiture » peut être un fourgon au
        // gabarit déclaré (truck), ou une moto (motorcycle) — même graphe,
        // autre fonction de coût. L'utilitaire gagne : on ne coche pas les
        // deux, et les ponts bas n'attendent pas.
        case .car:
            if UserDefaults.standard.bool(forKey: "moncap.utilitaire") {
                return "truck"
            }
            return UserDefaults.standard.bool(forKey: "moncap.moto")
                ? "motorcycle" : "auto"
        // Jamais employé : le transit est calculé par le serveur, et les
        // chemins locaux sont gardés en amont. La marche est le repli le
        // moins faux si un appel se glissait quand même jusqu'ici.
        case .transit: return "pedestrian"
        }
    }

    var icon: String {
        switch self {
        case .walk: return "figure.walk"
        case .bike: return "bicycle"
        case .car: return "car.fill"
        case .transit: return "tram.fill"
        }
    }

    var label: String {
        switch self {
        case .walk: return "À pied"
        case .bike: return "Vélo"
        case .car: return "Voiture"
        case .transit: return "Transports"
        }
    }
}

/// Point d'entrée : écran de garde/chargement au lancement, puis écran de
/// connexion ou carte selon l'authentification.
struct ContentView: View {
    @StateObject private var auth = AuthStore()
    /// Affiche l'écran de garde le temps du lancement, puis se fond dans l'app.
    @State private var showSplash = true
    /// La découverte du premier lancement, vue une fois pour toutes.
    @AppStorage("moncap.decouverte.vue") private var decouverteVue = false

    var body: some View {
        ZStack {
            if auth.isAuthenticated {
                MapHomeView(auth: auth)
            } else {
                LoginView(auth: auth)
            }
            if showSplash {
                SplashView()
                    // L'écran de garde s'efface en s'élargissant un peu : on
                    // le traverse plutôt qu'il ne s'éteint, et la carte qui
                    // apparaît dessous paraît avoir été là tout du long.
                    .transition(.opacity.combined(with: .scale(scale: 1.06)))
                    .zIndex(1)
            }
        }
        .task {
            // Laisse l'écran de garde s'afficher un court instant, puis fond
            // vers l'application. Le délai laisse à l'aiguille le temps de se
            // poser : elle atteint le nord vers 1,25 s, et partir avant
            // reviendrait à couper le geste au milieu.
            try? await Task.sleep(nanoseconds: 1_800_000_000)
            withAnimation(.easeInOut(duration: 0.5)) { showSplash = false }
        }
        // La découverte, une fois l'écran de garde passé, une fois pour
        // toutes : trois écrans qui disent la valeur avant la carte nue.
        .fullScreenCover(
            isPresented: Binding(
                get: { !decouverteVue && !showSplash }, set: { _ in })
        ) {
            DecouverteView { decouverteVue = true }
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
    @StateObject private var weather = WeatherService()
    @StateObject private var airAutour = AirAutour()
    @StateObject private var fires = FireService()
    /// Calcul d'itinéraire sur l'appareil : observé pour suivre l'installation
    /// des tuiles depuis les réglages.
    @ObservedObject private var router = OfflineRouter.shared
    @StateObject private var traffic = TrafficService()
    /// Clé TomTom optionnelle saisie sur l'appareil : si présente, l'app
    /// interroge TomTom directement (sinon elle passe par le backend).
    @AppStorage("moncap.tomtomKey") private var tomtomKey = ""
    /// Calculer les itinéraires sur l'appareil, sans jamais interroger le
    /// réseau. Choisi, et non subi : partir en montagne ou à l'étranger se
    /// décide avant de perdre le signal, pas après.
    @AppStorage("moncap.horsLigne") private var horsLigne = false
    /// L'état réel du réseau : le badge « hors ligne » ne doit pas dépendre
    /// d'un réglage mais d'un fait observé.
    @ObservedObject private var reseau = Reseau.partage
    /// Dernier état de réseau vu par l'annonce vocale : on n'annonce que les
    /// vraies bascules, pas la valeur courante rejouée à l'abonnement.
    @State private var reseauCoupePrecedent: Bool?
    private let api = APIClient()

    @State private var positions: [Position] = []
    /// Filtre sur la liste des régions d'itinéraire : deux cents paquets dans
    /// cinquante pays ne se parcourent pas au doigt.
    @State private var stats: Stats?
    @State private var routeInfo: String?
    /// Cadrage de départ sur la France : tant qu'aucune position n'est
    /// connue, mieux vaut cela que le cadrage par défaut du style, qui est le
    /// monde entier centré sur le golfe de Guinée.
    @State private var camera: MapCam? = MapCam(
        center: CLLocationCoordinate2D(latitude: 46.6, longitude: 2.4), zoom: 4.6)
    @State private var showPlaces = false
    @State private var showSearch = false
    @State private var destQuery = ""
    @State private var recents: [RecentSearch] = Session.recentSearches
    @State private var showReports = false
    /// Message d'erreur de partage (ex. serveur de suivi injoignable).
    @State private var shareError: String?
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
    /// Signalement touché sur la carte (pour voter 👍/👎).
    @State private var alertToVote: Alert?
    /// Favoris Domicile / Travail (stockés sur l'appareil).
    /// Pourquoi aucun itinéraire n'a pu être proposé. Rendre la main sans
    /// rien dire laissait croire à un bouton mort.
    @State private var itineraireImpossible: String?
    @State private var homePlace = Session.home
    @State private var workPlace = Session.work
    /// Dernier avertissement de dépassement de vitesse (anti-spam).
    @State private var lastSpeedWarning = Date.distantPast
    /// Première position reçue → mise à jour immédiate de la limite de vitesse.
    @State private var didInitialSpeedFetch = false
    /// Dernière recherche d'itinéraire plus rapide (toutes les 90 s).
    @State private var lastFasterCheck = Date.distantPast
    /// Anti-bouchon (« toujours avancer ») : depuis quand on rampe, et dernier
    /// contournement déclenché (anti-répétition).
    @State private var stuckSince: Date?
    @State private var lastAntiJamReroute = Date.distantPast
    /// Bouchons (communautaires + trafic réel) : dernier scan, et bouchons déjà
    /// contournés (pour ne pas rerouter en boucle sur le même).
    @State private var lastCommunityJamCheck = Date.distantPast
    @State private var avoidedJamIDs: Set<String> = []
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
    /// Mode moto : la « voiture » devient un deux-roues (costing Valhalla).
    @AppStorage("moncap.moto") private var modeMoto = false
    @AppStorage("moncap.motoPlaisir") private var motoPlaisir = false
    /// Gabarit utilitaire (Pro) : costing truck aux dimensions déclarées.
    @AppStorage("moncap.utilitaire") private var modeUtilitaire = false
    @AppStorage("moncap.util.hauteur") private var utilHauteur = 2.7
    @AppStorage("moncap.util.largeur") private var utilLargeur = 2.05
    @AppStorage("moncap.util.poids") private var utilPoids = 3.5

    /// L'interrupteur Utilitaire, gardé par Pro : sans la formule, il mène
    /// au paywall au lieu de s'allumer — et il éteint le mode moto.
    private var lienUtilitaire: Binding<Bool> {
        Binding(
            get: { modeUtilitaire },
            set: { veut in
                if veut, !plus.accesPro {
                    apresMenu = { showPaywall = true }
                    showPlaces = false
                    return
                }
                modeUtilitaire = veut
                if veut { modeMoto = false }
            })
    }
    @AppStorage("moncap.consumption") private var consumption = 6.5
    /// Signalements déjà annoncés vocalement pendant cette navigation.
    @State private var announcedAlertIDs: Set<Int> = []
    /// Notifications locales d'alertes à proximité (bannière + son), y compris
    /// hors navigation et écran verrouillé.
    @AppStorage("moncap.notifyNearby") private var notifyNearbyAlerts = false
    @StateObject private var nearbyNotifier = NearbyAlertNotifier()
    /// Itinéraire actuellement prévisualisé (mis en avant) avant décision.
    @State private var selectedRouteID: UUID?
    /// Liste déroulée ou repliée (bandeau compact par défaut).
    @State private var routesExpanded = false
    @State private var pendingDestination: CLLocationCoordinate2D?
    /// Carte inclinée en 3D si l'utilisateur l'active.
    @State private var is3D = false
    /// Suivi automatique de la position en navigation (désactivé si on déplace
    /// la carte à la main ; réactivé après 8 s ou via le bouton de recentrage).
    @State private var followsRoute = true
    /// Dernière manipulation manuelle de la carte.
    @State private var lastMapInteraction = Date.distantPast
    /// Dernière caméra 3D appliquée (anti-tremblement à l'arrêt).
    @State private var lastCamCoord: CLLocationCoordinate2D?
    @State private var lastCamHeading: Double = -1
    /// Roulait-on au dernier recadrage ? Sert à dézoomer dès l'arrêt.
    @State private var wasDriving = false
    /// Itinéraire interrompu retrouvé au lancement, en attente de reprise.
    @State private var resumable: SavedRoute?
    /// Carte hors ligne (tuiles téléchargées, écran dédié).
    @State private var showOfflineMap = false
    /// Serveur de tuiles pour les cartes hors ligne. Vide = fonction inactive.
    @AppStorage(TileSource.defaultsKey) private var tileURL = ""
    /// Style vectoriel MapLibre : renseigné, il prime sur les tuiles images.
    @AppStorage(TileSource.styleKey) private var styleURL = ""
    /// Mode nuit automatique (selon la position réelle du soleil).
    @State private var nightMode = false
    /// Dernier calcul jour/nuit (recalcul limité à 1×/min : le soleil bouge
    /// lentement, inutile de refaire la trigo à chaque point GPS).
    @State private var lastNightCheck = Date.distantPast
    /// Dernier envoi de position live (throttle du WebSocket de partage).
    @State private var lastLiveSend = Date.distantPast
    @State private var lastLiveCoord: CLLocationCoordinate2D?
    /// Prévisualisation (survol) de l'itinéraire en cours.
    @State private var previewing = false
    @State private var previewTask: Task<Void, Never>?

    // Historique des trajets parcourus.
    @State private var trips: [Trip] = []
    @State private var showTrips = false
    /// « Prévenir un proche » (message d'arrivée à un contact).
    @State private var showArrive = false
    /// Carte des feux de forêt (contours sur satellite, écran dédié).
    @State private var showFireMap = false
    @State private var showAirMap = false
    /// Profil du vélo pour le calcul local (Road / Hybrid / Mountain).
    @AppStorage("moncap.veloType") private var veloType = "Hybrid"
    @AppStorage("moncap.veloEviteCotes") private var veloEviteCotes = false
    /// Le moment du trajet en transports : maintenant, partir à, ou arriver
    /// avant. Les horaires sont toute la donnée d'un tram — les figer sur
    /// « maintenant » interdisait de préparer le bus de demain matin.
    @State private var transitQuand = TransitQuand.maintenant
    @State private var transitDate = Date()
    /// Les étapes intermédiaires du trajet en préparation, dans l'ordre.
    @State private var etapes: [CLLocationCoordinate2D] = []
    @State private var etapeNoms: [String] = []
    /// Minutes d'arrêt prévues à chaque étape (0 = simple passage). Comptées
    /// dans la durée affichée : un détour chez Paul, c'est le trajet plus le
    /// quart d'heure chez Paul.
    @State private var etapeArrets: [Int] = []
    /// Créneaux « avant HH:MM » (Pro), en minutes du jour, par coordonnée
    /// d'étape quantifiée : la clé survit au réordonnancement.
    @State private var etapeCreneaux: [String: Int] = [:]
    /// Vrai le temps de choisir un fichier GPX dans le sélecteur.
    @State private var importeGPX = false
    /// La trace GPX en cours de suivi, s'il y en a une. Elle change la règle
    /// du recalcul : la trace est la route, on ne la remplace pas par un
    /// itinéraire calculé quand on s'en écarte.
    @State private var traceGPX: RouteOption?
    /// Vrai le temps de désigner la prochaine étape sur la carte.
    @State private var ajouteEtape = false
    /// Étapes déjà atteintes pendant le guidage (index dans `etapes`) : le
    /// recalcul d'écart ne repasse pas par elles.
    @State private var etapesAtteintes: Set<Int> = []
    /// Ce que chaque variante rencontre en chemin — feux, pluie — par
    /// identifiant.
    @State private var risquesParOption: [UUID: [String]] = [:]
    /// Ce qui attend à l'arrivée, commun à toutes les variantes : air, pluie.
    @State private var risquesArrivee: [String] = []
    /// Clé NASA FIRMS optionnelle (sinon backend). Vide = via serveur.
    @AppStorage("moncap.firmsKey") private var firmsKey = ""
    /// Partage de position en direct.
    @StateObject private var liveShare = LiveShareManager()
    /// L'abonnement MonCap Plus : l'état vient de StoreKit, partagé partout.
    @ObservedObject private var plus = MonCapPlus.shared
    /// Trois paywalls, un par contexte de présentation : la carte, la
    /// feuille des trajets, la feuille de recherche — un même binding servi
    /// à plusieurs `.sheet` se présenterait de travers.
    @State private var showPaywall = false
    @State private var showPaywallTrips = false
    @State private var showPaywallRecherche = false
    @State private var showFacturation = false
    @State private var showAttestation = false
    @State private var showCollerAdresses = false
    @State private var showStationsCarburant = false
    @State private var showRecapAnnuel = false
    @State private var showEntretien = false
    @State private var showRappelDepart = false
    /// Proposition « j'arrive » : visible, et déjà faite pour ce trajet.
    @State private var proposeArrivee = false
    @State private var arriveeProposee = false
    /// Paywall et gestion d'abonnement présentés depuis le menu Lieux.
    @State private var showPaywallMenu = false
    @State private var gereAbonnement = false
    /// Ce qui doit se produire une fois le menu Lieux refermé — présenter
    /// une autre feuille, ouvrir un partage. Exécuté dans `onDismiss` : le
    /// « ferme puis attends 0,35 s » devinait la durée de l'animation, et
    /// iOS 26+ a cessé de se laisser deviner — les boutons semblaient morts.
    @State private var apresMenu: (() -> Void)?
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
                // Le badge hors ligne, sous le bandeau : on doit être SÛR
                // qu'on roule sans réseau, d'un coup d'œil — coupure subie
                // en orange, mode choisi en bleu.
                badgeHorsLigne
                Spacer()
            }
            .padding(.horizontal)
            .padding(.top, 8)
            // Les cartes flottantes sur la carte sont denses par nature : aux
            // tailles de texte d'accessibilité, leurs lignes débordaient et
            // leurs boutons s'étiraient (le Démarrer en colonne verte pleine
            // hauteur, constaté sur appareil). Le texte y est plafonné à
            // xxLarge — comme le fait Plans — les feuilles (recherche,
            // réglages…), défilantes, gardent elles la taille du réglage.
            .dynamicTypeSize(...DynamicTypeSize.xxLarge)

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
                    if proposeArrivee {
                        arriveeBanner
                    }
                    etaCard
                } else if !routeOptions.isEmpty {
                    routeOptionsCard
                }
            }
            .padding(.horizontal)
            .padding(.bottom, 6)
            // Même plafond que la pile du haut, même raison.
            .dynamicTypeSize(...DynamicTypeSize.xxLarge)
        }
        .sheet(
            isPresented: $showPlaces,
            onDismiss: {
                let action = apresMenu
                apresMenu = nil
                action?()
            }
        ) { placesSheet }
        .sheet(isPresented: $showSearch) { searchSheet }
        .sheet(isPresented: $showReports) { reportsSheet }
        .sheet(isPresented: $showTrips) { tripsSheet }
        .sheet(isPresented: $showArrive) {
            ArriveNotifyView()
                .presentationBackground(Color(.systemBackground))
        }
        .sheet(isPresented: $showPaywall) { PaywallView() }
        .sheet(isPresented: $showRappelDepart) {
            RappelDepartView(
                minutesTrajet: (routeOptions.first { $0.id == selectedRouteID }
                    ?? routeOptions.first)?.minutes ?? 0)
        }
        .sheet(isPresented: $showEntretien) {
            EntretienView(kmJournal: trips.reduce(0) { $0 + $1.distance_km })
        }
        .sheet(isPresented: $showStationsCarburant) {
            StationsCarburantView(
                itineraire: routeOptions.first { $0.id == selectedRouteID }?.coordinates
                    ?? routeOptions.first?.coordinates ?? [],
                type: fuelType,
                prixMoyen: fuel.effectivePrice(type: fuelType)
            ) { station in
                etapes.append(station.coord)
                // Cinq minutes d'arrêt : le temps du plein.
                etapeArrets.append(5)
                etapeNoms.append(station.nom)
                if let dest = pendingDestination {
                    Task { await presentRouteOptions(to: dest) }
                }
            }
        }
        .sheet(isPresented: $showCollerAdresses) {
            CollerAdressesView { arrets in
                for a in arrets {
                    etapes.append(a.coord)
                    etapeArrets.append(0)
                    etapeNoms.append(a.nom)
                }
                if let dest = pendingDestination {
                    Task { await presentRouteOptions(to: dest) }
                }
            }
        }
        // Le verrou de la maison : l'essai fini et rien de payé, l'app se
        // ferme derrière le paywall — rien de gratuit passé quatorze jours,
        // pour personne. Le lien est calculé : dès qu'un achat (ou une
        // restauration, un code, l'invitation) rouvre l'accès, il tombe.
        .fullScreenCover(
            isPresented: Binding(get: { !plus.accesPlus }, set: { _ in })
        ) {
            PaywallView(bloquant: true, surChangeCompte: { auth.logout() })
        }
        .alert(
            "Aucun itinéraire", isPresented: Binding(
                get: { itineraireImpossible != nil },
                set: { if !$0 { itineraireImpossible = nil } })
        ) {
            Button("OK", role: .cancel) { itineraireImpossible = nil }
        } message: {
            // Déjà traduit à la source.
            Text(itineraireImpossible ?? "")
        }
        .alert(
            "Partage impossible", isPresented: Binding(
                get: { shareError != nil }, set: { if !$0 { shareError = nil } })
        ) {
            Button("OK", role: .cancel) { shareError = nil }
        } message: {
            Text(shareError ?? "")
        }
        // Pendant le guidage, la voix double le badge : la coupure comme le
        // retour du réseau se disent au moment où ils surviennent — les yeux
        // restent sur la route. Le premier état reçu à l'abonnement n'est pas
        // une bascule : il ne s'annonce pas.
        .onReceive(reseau.$coupe) { coupe in
            defer { reseauCoupePrecedent = coupe }
            guard let avant = reseauCoupePrecedent, avant != coupe, nav.active
            else { return }
            nav.announce(NSLocalizedString(
                coupe
                    ? "Réseau coupé. Le guidage continue sur l'appareil."
                    : "Le réseau est de retour.",
                comment: ""))
        }
        // Un lien moncap://trajet?… — depuis la page de partage, un message,
        // un autre appareil : le trajet s'ouvre ici comme si on l'avait posé.
        .onOpenURL { ouvreLien($0) }
        // Une demande Siri (« Ramène-moi à la maison ») : app ouverte, la
        // notification arrive ; lancée à froid, la relève au premier passage
        // suffit — la demande attend dans UserDefaults.
        .onReceive(NotificationCenter.default.publisher(for: GuidageDemande.signal)) { _ in
            traiteGuidageDemande()
        }
        .onAppear { traiteGuidageDemande() }
        .fullScreenCover(isPresented: $showFireMap) {
            FireMapScreen(service: fires)
        }
        .fullScreenCover(isPresented: $showAirMap) {
            AirMapScreen(center: location.coordinate)
        }
        .fullScreenCover(isPresented: $showOfflineMap) {
            OfflineMapScreen(center: location.coordinate)
        }
        // Reprise d'une navigation interrompue : le guidage repart sur
        // l'itinéraire mémorisé, sans avoir besoin du réseau.
        .alert(
            "Reprendre la navigation ?", isPresented: Binding(
                get: { resumable != nil }, set: { if !$0 { resumable = nil } })
        ) {
            Button("Reprendre") {
                if let r = resumable {
                    nav.start(
                        steps: r.navSteps, coordinates: r.coordinates,
                        distanceKm: r.km, etaMinutes: r.minutes,
                        destination: r.destinationCoordinate,
                        // Reprise en cours de route : le guidage doit repartir
                        // d'ici, pas du départ du trajet.
                        resumingAt: location.coordinate)
                    speedLimit.preload(along: r.coordinates)
                    followsRoute = true
                }
                resumable = nil
            }
            Button("Abandonner", role: .destructive) {
                RouteStore.clear()
                resumable = nil
            }
        } message: {
            Text(
                resumable.map { String(format: NSLocalizedString("Itinéraire de %.1f km en cours.", comment: ""), $0.km) }
                    ?? "")
        }
        // Vote sur un signalement touché : toujours là / plus là.
        .confirmationDialog(
            alertToVote.map {
                let brut = $0.label.isEmpty ? $0.category : $0.label
                return "\(emoji(for: $0.category)) \(NSLocalizedString(brut, comment: ""))"
            }
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
            // Jour / nuit dès l'ouverture (centre de la France en repli, la
            // vraie position affine dès le premier point GPS).
            nightMode = DayNight.isNight(lat: 46.6, lon: 2.5)
            location.start()
            realtime.onPositionsChanged = { Task { await refresh() } }
            nav.onReroute = { Task { await recomputeRoute() } }
            // Navigation interrompue (app relancée, téléphone redémarré) : on
            // propose de la reprendre. Utile surtout hors réseau, où elle ne
            // pourrait pas être recalculée.
            if !nav.active { resumable = RouteStore.load() }
            realtime.connect()
            await refresh()
            await loadTrips()
            await loadRecents()
        }
        .onReceive(location.$coordinate) { coord in
            guard let c = coord else { return }
            if sharing {
                // Throttle : au plus 1 envoi / 2 s, et seulement si on a bougé
                // d'au moins 8 m — évite d'inonder le WebSocket à la cadence GPS.
                let movedLive = lastLiveCoord.map { metersBetween($0, c) } ?? .infinity
                if Date().timeIntervalSince(lastLiveSend) >= 2, movedLive >= 8 {
                    lastLiveSend = Date()
                    lastLiveCoord = c
                    realtime.sendLive(
                        lat: c.latitude, lon: c.longitude, label: driverName, avatar: liveAvatar)
                }
            }
            // Limites de vitesse : mise à jour immédiate au lancement, puis en
            // continu (cache d'itinéraire d'abord, Overpass sinon) — même hors
            // navigation.
            if !didInitialSpeedFetch {
                didInitialSpeedFetch = true
                speedLimit.refreshNow(c)
            } else {
                speedLimit.update(c)
            }
            // Prix du carburant local (cache 6 h, pour le coût des trajets).
            fuel.refresh(near: c, type: fuelType)
            // Météo en temps réel (température + conditions).
            weather.update(c)
            // Qualité de l'air autour de soi (throttle interne à 30 min).
            airAutour.update(c)
            // Incendies actifs NASA FIRMS (throttle interne à 15 min).
            fires.refresh(key: firmsKey)
            // Partage de position en direct (throttle interne à 5 s).
            liveShare.update(c, heading: location.course, speed: location.speedKmh)
            // Jour / nuit selon la position réelle du soleil : recalcul au plus
            // 1×/min (le soleil bouge lentement — pas besoin à chaque point GPS).
            if Date().timeIntervalSince(lastNightCheck) >= 60 {
                lastNightCheck = Date()
                nightMode = DayNight.isNight(lat: c.latitude, lon: c.longitude)
            }
            if location.speedKmh > 15 { checkOverspeed() }

            // Notifications d'alertes à proximité (bannière + son), même hors
            // navigation, si l'option est activée.
            if notifyNearbyAlerts {
                nearbyNotifier.check(from: c, alerts: realtime.alerts) { emoji(for: $0) }
            }

            if nav.active {
                nav.update(c, seuilEcart: travelMode.seuilEcartM)
                // Une étape est atteinte à moins de cent cinquante mètres :
                // le recalcul d'écart ne repassera pas par elle.
                for (i, e) in etapes.enumerated()
                where !etapesAtteintes.contains(i) && metersBetween(c, e) < 150 {
                    etapesAtteintes.insert(i)
                }
                recordTrackPoint(c)
                announceNearbyAlerts(from: c)
                // À dix minutes de l'arrivée, on propose une fois de
                // prévenir — dans Plus : tout le monde est attendu quelque
                // part, pas seulement ceux qui roulent pour travailler.
                if plus.accesPlus, !arriveeProposee, nav.etaMinutes > 0,
                    nav.etaMinutes <= 10
                {
                    arriveeProposee = true
                    proposeArrivee = true
                }
                maybeCheckFasterRoute(from: c)
                // « Toujours avancer » : évite en amont les bouchons connus
                // (trafic réel + signalements) et contourne si on est bloqué.
                refreshTraffic(from: c)
                checkJamAhead(from: c)
                checkStuckInTraffic(from: c)
                // Recentre en permanence : si on a touché la carte, le suivi
                // reprend tout seul après 8 s.
                if !followsRoute, Date().timeIntervalSince(lastMapInteraction) > 8 {
                    followsRoute = true
                }
                // Caméra de suivi, 2D comme 3D : recadrée en cas de vrai
                // mouvement (le bruit GPS à l'arrêt ferait trembler la vue) et
                // au passage arrêt ↔ conduite, qui change le niveau de zoom.
                if !previewing, followsRoute {
                    let heading = location.course
                    let driving = location.speedKmh >= Self.stoppedKmh
                    let moved = lastCamCoord.map { metersBetween($0, c) } ?? .infinity
                    var headingDelta = abs(heading - lastCamHeading)
                    if headingDelta > 180 { headingDelta = 360 - headingDelta }
                    let paceChanged = driving != wasDriving
                    if moved >= 2 || (driving && headingDelta >= 3) || paceChanged {
                        lastCamCoord = c
                        if driving { lastCamHeading = heading }
                        wasDriving = driving
                        withAnimation(.easeInOut(duration: 1.0)) {
                            camera = followCamera(c)
                        }
                    }
                }
            } else if !previewing, followsRoute {
                // Hors navigation aussi : sans cela rien ne posait jamais de
                // caméra, et la carte restait sur le cadrage par défaut du
                // style — le monde entier — jusqu'à ce qu'on appuie sur
                // recentrer. Le suivi s'arrête dès qu'on touche la carte.
                let moved = lastCamCoord.map { metersBetween($0, c) } ?? .infinity
                if moved >= 20 {
                    lastCamCoord = c
                    withAnimation(.easeInOut(duration: 1.0)) { camera = userCamera(c) }
                }
            }
        }
        .onReceive(realtime.$alerts) { alerts in
            // Nouvelle alerte reçue : notifie si elle est proche (utile même à
            // l'arrêt, quand on ne bouge pas vers l'alerte).
            guard notifyNearbyAlerts, let c = location.coordinate else { return }
            nearbyNotifier.check(from: c, alerts: alerts) { emoji(for: $0) }
        }
        .onChange(of: notifyNearbyAlerts) { _, on in
            // À l'activation : demande l'autorisation de notification.
            if on { nearbyNotifier.requestAuthorizationIfNeeded() }
        }
        .onChange(of: firmsKey) { _, key in
            // Nouvelle clé FIRMS saisie : on recharge les feux tout de suite.
            fires.refresh(key: key)
        }
        .task {
            // Précharge les feux dès le lancement (indépendant de la position) →
            // la carte des feux s'ouvre instantanément.
            fires.refresh(key: firmsKey)
        }
        .onChange(of: nav.active) { wasActive, isActive in
            // Empêche la mise en veille de l'écran pendant la navigation.
            UIApplication.shared.isIdleTimerDisabled = isActive
            // GPS + guidage vocal continuent en arrière-plan / écran verrouillé.
            location.setBackgroundTracking(isActive)
            // Fin de navigation : on vide le cache d'itinéraire mais on garde
            // la limite courante affichée, et on oublie les bouchons contournés.
            if !isActive {
                speedLimit.clearRoute()
                avoidedJamIDs = []
                stuckSince = nil
            }
            // Chaque navigation a droit à sa proposition « j'arrive ».
            arriveeProposee = false
            proposeArrivee = false
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
        .onChange(of: avoidTolls) { _, _ in reroutePreferencesChanged() }
        .onChange(of: avoidHighways) { _, _ in reroutePreferencesChanged() }
        .onChange(of: fuelType) { _, newType in
            if let c = location.coordinate { fuel.refresh(near: c, type: newType) }
        }
        // Mode nuit automatique : l'app suit le soleil (coucher → sombre).
        .preferredColorScheme(nightMode ? .dark : .light)
    }

    /// Recalcule les itinéraires affichés quand une préférence change.
    private func reroutePreferencesChanged() {
        guard !nav.active, let dest = pendingDestination else { return }
        Task { await presentRouteOptions(to: dest) }
    }

    /// Cherche périodiquement (90 s) un itinéraire plus rapide vers la
    /// destination (mode voiture) et l'applique s'il fait gagner ≥ 2 min.
    /// Demande les alternatives : le temps estimé de MapKit tient compte du
    /// trafic, on retient donc le plus rapide selon les conditions du moment.
    private func maybeCheckFasterRoute(from c: CLLocationCoordinate2D) {
        guard travelMode == .car, !nav.rerouting,
            Date().timeIntervalSince(lastFasterCheck) > 90,
            let dest = nav.destination
        else { return }
        lastFasterCheck = Date()
        Task {
            let routes = await drivingRoutes(from: c, to: dest, transportType: .automobile)
            guard let route = routes.min(by: { $0.expectedTravelTime < $1.expectedTravelTime })
            else { return }
            let saved = nav.etaMinutes - route.expectedTravelTime / 60
            if saved >= 2, nav.active {
                nav.applyFasterRoute(route: route, savedMinutes: Int(saved.rounded()))
                // Les limites de vitesse suivent le nouveau tracé.
                speedLimit.preload(along: route.polyline.coordinates)
            }
        }
    }

    /// « Toujours avancer » : si on rampe (< 7 km/h) trop longtemps en voiture,
    /// c'est le signe d'un bouchon. On cherche alors un contournement réel (un
    /// itinéraire qui s'écarte de la route actuelle) et on bascule dessus s'il
    /// fait gagner du temps. Le garde-fou « plus rapide et divergent » évite de
    /// recalculer à un simple feu rouge : à un feu, rester sur place reste le
    /// plus rapide, donc rien ne change.
    private func checkStuckInTraffic(from c: CLLocationCoordinate2D) {
        guard travelMode == .car, nav.active, !nav.rerouting else {
            stuckSince = nil
            return
        }
        // On roule normalement : pas de bouchon.
        guard location.speedKmh < 7 else {
            stuckSince = nil
            return
        }
        // Tout près de l'arrivée : un arrêt est normal, on ne contourne pas.
        guard nav.remainingKm > 0.3 else {
            stuckSince = nil
            return
        }
        let now = Date()
        if stuckSince == nil { stuckSince = now }
        guard let since = stuckSince,
            now.timeIntervalSince(since) >= 35,
            now.timeIntervalSince(lastAntiJamReroute) > 120
        else { return }
        lastAntiJamReroute = now
        stuckSince = nil

        let currentCoords = nav.routeCoords
        let currentEta = nav.etaMinutes
        Task {
            guard let dest = nav.destination else { return }
            let routes = await drivingRoutes(from: c, to: dest, transportType: .automobile)
            guard routes.count >= 2, nav.active else { return }
            // Sépare les itinéraires qui suivent la route actuelle de ceux qui
            // s'en écartent (les vrais contournements).
            let detours = routes.filter { Self.routeDiverges($0, from: currentCoords) }
            let staying = routes.filter { !Self.routeDiverges($0, from: currentCoords) }
            guard let detour = detours.min(by: { $0.expectedTravelTime < $1.expectedTravelTime })
            else { return }  // aucune alternative pour contourner → on reste
            // Référence = temps réel (trafic inclus) en restant sur la route
            // actuelle ; à défaut, l'ETA courante.
            let baseSecs = staying.map { $0.expectedTravelTime }.min() ?? (currentEta * 60)
            let saved = (baseSecs - detour.expectedTravelTime) / 60
            if saved >= 1 {
                nav.applyDetour(route: detour, savedMinutes: Int(saved.rounded()))
                speedLimit.preload(along: detour.polyline.coordinates)
            }
        }
    }

    /// Vrai si `route` s'écarte nettement (> 250 m) du corridor `current` sur
    /// ses ~3 premiers km — donc propose un vrai contournement, pas la même
    /// route. `nonisolated static` : aucune donnée de vue touchée.
    nonisolated private static func routeDiverges(
        _ route: MKRoute, from current: [CLLocationCoordinate2D]
    ) -> Bool {
        guard !current.isEmpty else { return true }
        let cand = route.polyline.coordinates
        guard cand.count > 1 else { return false }
        var walked = 0.0
        var prev = cand[0]
        for c in cand {
            walked += CLLocation(latitude: prev.latitude, longitude: prev.longitude)
                .distance(from: CLLocation(latitude: c.latitude, longitude: c.longitude))
            prev = c
            if walked > 3000 { break }  // on ne juge que le début du trajet
            var best = Double.greatestFiniteMagnitude
            for p in current {
                let d = CLLocation(latitude: c.latitude, longitude: c.longitude)
                    .distance(from: CLLocation(latitude: p.latitude, longitude: p.longitude))
                if d < best {
                    best = d
                    if best < 120 { break }  // proche du corridor : inutile de continuer
                }
            }
            if best > 250 { return true }
        }
        return false
    }

    /// Rafraîchit les bouchons de trafic réel (TomTom, via le backend) autour de
    /// la position — emprise ~±20 km. Le service temporise en interne (60 s).
    private func refreshTraffic(from c: CLLocationCoordinate2D) {
        guard travelMode == .car, nav.active else { return }
        let d = 0.2  // ~ ±20 km de marge autour de la position
        traffic.refresh(
            minLon: c.longitude - d, minLat: c.latitude - d,
            maxLon: c.longitude + d, maxLat: c.latitude + d,
            key: tomtomKey)
    }

    /// Bouchons sur le trajet : si un bouchon (signalé par la communauté ou
    /// remonté par le trafic réel) se trouve devant nous, on cherche un
    /// itinéraire qui l'évite et on bascule dessus — de façon proactive, avant
    /// même d'y être bloqué.
    private func checkJamAhead(from c: CLLocationCoordinate2D) {
        guard travelMode == .car, nav.active, !nav.rerouting,
            Date().timeIntervalSince(lastCommunityJamCheck) > 20,
            Date().timeIntervalSince(lastAntiJamReroute) > 120
        else { return }
        lastCommunityJamCheck = Date()

        guard let jam = jamAhead(from: c), !avoidedJamIDs.contains(jam.id) else { return }
        let jamCoord = jam.coord
        lastAntiJamReroute = Date()
        avoidedJamIDs.insert(jam.id)
        Task {
            guard let dest = nav.destination else { return }
            let routes = await drivingRoutes(from: c, to: dest, transportType: .automobile)
            guard nav.active else { return }
            // Sépare les itinéraires qui évitent le bouchon de ceux qui le
            // traversent.
            let avoiding = routes.filter { Self.route($0, avoids: jamCoord, within: 250) }
            let crossing = routes.filter { !Self.route($0, avoids: jamCoord, within: 250) }
            guard let best = avoiding.min(by: { $0.expectedTravelTime < $1.expectedTravelTime })
            else { return }  // impossible d'éviter → la détection vitesse prendra le relais
            // On bascule pour éviter le bouchon, sauf si le détour est vraiment
            // disproportionné (> 5 min de plus que de rester). Comme les temps
            // MapKit intègrent le trafic, un vrai bouchon rend souvent l'évitement
            // plus rapide de toute façon.
            let stayingSecs = crossing.map { $0.expectedTravelTime }.min()
            if let stay = stayingSecs, best.expectedTravelTime > stay + 300 { return }
            nav.applyReportedJamDetour(route: best)
            speedLimit.preload(along: best.polyline.coordinates)
        }
    }

    /// Tous les bouchons connus (communauté + trafic réel TomTom), avec un id
    /// stable pour l'anti-répétition.
    private func knownJams() -> [(id: String, coord: CLLocationCoordinate2D)] {
        var out: [(id: String, coord: CLLocationCoordinate2D)] = []
        for a in realtime.alerts where a.category == "bouchon" {
            out.append(
                (id: "alert-\(a.id)",
                    coord: CLLocationCoordinate2D(latitude: a.lat, longitude: a.lon)))
        }
        for j in traffic.jams {
            out.append((id: "traffic-\(j.id)", coord: j.coordinate))
        }
        return out
    }

    /// Renvoie un bouchon situé sur l'itinéraire, devant nous (entre ~400 m et
    /// ~8 km) et à moins de 300 m du tracé — ou nil.
    private func jamAhead(from c: CLLocationCoordinate2D)
        -> (id: String, coord: CLLocationCoordinate2D)?
    {
        let coords = nav.routeCoords
        guard coords.count >= 2 else { return nil }
        let here = CLLocation(latitude: c.latitude, longitude: c.longitude)
        let hereIdx = Self.nearestIndex(on: coords, to: c)
        for jam in knownJams() {
            // Pré-filtre bon marché : la distance le long du tracé est toujours
            // ≥ la distance à vol d'oiseau. Un bouchon à plus de ~8,5 km à vol
            // d'oiseau ne peut donc pas être dans notre fenêtre de 8 km sur la
            // route → on l'écarte sans le coûteux nearestIndex sur tout le tracé
            // (jusqu'à 300 bouchons en trafic réel, 20 s d'intervalle).
            let asCrow = here.distance(
                from: CLLocation(latitude: jam.coord.latitude, longitude: jam.coord.longitude))
            guard asCrow <= 8500 else { continue }
            let jIdx = Self.nearestIndex(on: coords, to: jam.coord)
            guard jIdx > hereIdx else { continue }  // devant nous seulement
            let perp = CLLocation(latitude: coords[jIdx].latitude, longitude: coords[jIdx].longitude)
                .distance(from: CLLocation(latitude: jam.coord.latitude, longitude: jam.coord.longitude))
            guard perp < 300 else { continue }  // pas vraiment sur la route
            // Distance le long du tracé jusqu'au bouchon.
            var ahead = 0.0
            var i = hereIdx
            while i < jIdx, ahead <= 8000 {
                ahead += CLLocation(latitude: coords[i].latitude, longitude: coords[i].longitude)
                    .distance(
                        from: CLLocation(
                            latitude: coords[i + 1].latitude, longitude: coords[i + 1].longitude))
                i += 1
            }
            if ahead >= 400, ahead <= 8000 { return jam }
        }
        return nil
    }

    /// Indice du point de `coords` le plus proche de `p`.
    nonisolated private static func nearestIndex(
        on coords: [CLLocationCoordinate2D], to p: CLLocationCoordinate2D
    ) -> Int {
        let target = CLLocation(latitude: p.latitude, longitude: p.longitude)
        var bestI = 0
        var best = Double.greatestFiniteMagnitude
        for (i, c) in coords.enumerated() {
            let d = CLLocation(latitude: c.latitude, longitude: c.longitude).distance(from: target)
            if d < best {
                best = d
                bestI = i
            }
        }
        return bestI
    }

    /// Vrai si `route` ne passe jamais à moins de `within` mètres de `point`
    /// (donc l'évite). `nonisolated static` : aucune donnée de vue touchée.
    nonisolated private static func route(
        _ route: MKRoute, avoids point: CLLocationCoordinate2D, within: Double
    ) -> Bool {
        let target = CLLocation(latitude: point.latitude, longitude: point.longitude)
        for c in route.polyline.coordinates {
            if CLLocation(latitude: c.latitude, longitude: c.longitude).distance(from: target)
                < within
            {
                return false
            }
        }
        return true
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

    /// Grade selon les points de contribution.
    private func rankName(_ points: Int) -> String {
        switch points {
        case ..<50: return "Débutant"
        case ..<200: return "Éclaireur"
        case ..<500: return "Navigateur"
        case ..<1000: return "Capitaine"
        default: return "Légende de la route"
        }
    }

    /// Démarre le partage de position en direct, puis présente le lien à envoyer.
    private func startLiveShare() {
        Task {
            let name = Session.username.isEmpty ? "Un ami" : Session.username
            // Le lien s'obtient d'abord, le menu se ferme ensuite : le
            // partage part dans onDismiss, quand la feuille est vraiment
            // partie — deviner la durée de l'animation ne marche plus sur
            // iOS 26+.
            if let url = await liveShare.start(name: name) {
                // Continue d'émettre la position même écran verrouillé.
                location.setBackgroundTracking(true)
                apresMenu = { Sharer.present([url]) }
            } else {
                apresMenu = {
                    shareError =
                        "Le serveur de suivi est injoignable pour le moment. Réessaie dans un instant."
                }
            }
            showPlaces = false
        }
    }

    /// Compose et partage l'heure d'arrivée estimée.
    private func shareETA() {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.dateFormat = "HH:mm"
        let eta = f.string(from: Date().addingTimeInterval(nav.etaMinutes * 60))
        let text = String(
            format: "J'arrive vers %@ (%.1f km restants) — MonCap GPS 🛰️",
            eta, nav.remainingKm)
        Sharer.present([text])
    }

    /// Annonce vocalement (une seule fois chacun) les signalements à moins de
    /// 800 m pendant la navigation.
    private func announceNearbyAlerts(from c: CLLocationCoordinate2D) {
        for a in realtime.alerts where !announcedAlertIDs.contains(a.id) {
            let d = CLLocation(latitude: c.latitude, longitude: c.longitude)
                .distance(from: CLLocation(latitude: a.lat, longitude: a.lon))
            guard d < 800 else { continue }
            announcedAlertIDs.insert(a.id)
            // Le fil porte le libellé français : chaque appareil le traduit
            // à l'affichage, sinon un signalement espagnol s'afficherait en
            // espagnol chez un francophone.
            let what = NSLocalizedString(a.label.isEmpty ? a.category : a.label, comment: "")
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

    /// La portabilité RGPD : le JSON complet du compte, remis par la
    /// feuille de partage — à soi, pour soi.
    private func exporteDonnees() {
        Task {
            guard let donnees = try? await api.exportData() else { return }
            let url = FileManager.default.temporaryDirectory
                .appendingPathComponent("moncap-donnees.json")
            try? donnees.write(to: url)
            Sharer.present([url])
        }
    }

    /// Pousse Domicile et Travail au compte, en silence : un échec réseau
    /// se rattrapera au prochain rafraîchissement.
    private func pousseFavoris() {
        Task {
            try? await api.savePrefs(
                APIClient.Prefs(home: Session.home, work: Session.work))
        }
    }

    private func loadTrips() async {
        trips = (try? await api.trips()) ?? trips
        // Le journal vient de parler : les rappels d'entretien écoutent.
        Entretien.shared.verifie(kmJournal: trips.reduce(0) { $0 + $1.distance_km })
    }

    /// Itinéraire affiché : celui de la navigation en cours, sinon le calcul multi-points.
    private var displayedRoute: [CLLocationCoordinate2D] {
        nav.active ? nav.routeCoords : routeCoords
    }

    // MARK: - Carte

    /// Fond de carte réellement affiché : celui de l'utilisateur s'il répond,
    /// sinon le nôtre.
    ///
    /// Une adresse personnelle injoignable donnait une carte blanche sans rien
    /// pour le comprendre. En attendant la vérification, le fond par défaut est
    /// affiché : mieux vaut une carte tout de suite que rien.
    @State private var mapStyle = URL(string: TileSource.defaultStyle)!

    private var map: some View {
        MainMapView(
            styleURL: mapStyle,
            cam: camera,
            pins: mapPins,
            lines: mapLines,
            buildings3D: is3D,
            onPinTap: handlePinTap,
            onPlaceTap: { coord, _ in
                // Un lieu du fond de carte touché → propose les itinéraires,
                // ou pose l'étape qu'on est en train de désigner.
                guard !nav.active else { return }
                if ajouteEtape { ajouterEtape(coord); return }
                Task { await presentRouteOptions(to: coord) }
            },
            onMapTap: handleMapTap,
            // Appui long : la seule façon de désigner une destination sans
            // réseau. La recherche passe par MKLocalSearch et les positions
            // enregistrées par l'API — en mode avion, ni l'une ni l'autre ne
            // rend quoi que ce soit, et le paquet d'itinéraires installé
            // restait sans emploi faute de but à lui nommer.
            onMapLongPress: { coord in
                guard !nav.active else { return }
                if ajouteEtape { ajouterEtape(coord); return }
                Task { await presentRouteOptions(to: coord) }
            },
            onRegionChange: { region in
                visibleRegion = region
                // Recharge les lieux seulement si la carte a vraiment bougé.
                if let kind = poiKind, !nav.active, regionChangedEnough(region) {
                    Task { await loadPOIs(kind) }
                }
            },
            onUserGesture: userTouchedMap
        )
        .ignoresSafeArea()
        .task(id: styleURL) {
            mapStyle = await TileSource.resolvedStyle(custom: styleURL)
        }
    }

    /// Épingles de la carte, dans l'ordre où elles s'empilent.
    private var mapPins: [MapPin] {
        var pins: [MapPin] = []
        // Les incendies ne surchargent plus la carte GPS : ils ont leur écran
        // dédié (bouton 🔥 → carte des feux sur satellite).
        // Mon avatar à ma position. Il marque seul l'endroit où je suis : la
        // pastille bleue de la carte est éteinte.
        if let me = location.coordinate {
            pins.append(
                MapPin(id: "moi", coordinate: me, title: "Moi", look: .avatar(liveAvatar, 44)))
        }
        for p in positions {
            pins.append(
                MapPin(
                    id: "pos-\(p.id)",
                    coordinate: CLLocationCoordinate2D(latitude: p.lat, longitude: p.lon),
                    title: p.label, look: .marker("mappin", "#FF3B30")))
        }
        // Lieux de la catégorie choisie : un appui propose les itinéraires.
        if !nav.active {
            for p in pois {
                pins.append(
                    MapPin(
                        id: "poi-\(p.id)", coordinate: p.coordinate, title: p.name,
                        look: .emoji(poiKind?.emoji ?? "📍", 34)))
            }
        }
        // Aperçu par itinéraire : bulle durée + coût au milieu du tracé,
        // tapable pour sélectionner l'itinéraire.
        if !routeOptions.isEmpty && !nav.active {
            for o in routeOptions where o.coordinates.count >= 2 {
                pins.append(
                    MapPin(
                        id: "opt-\(o.id)",
                        coordinate: o.coordinates[o.coordinates.count / 2],
                        title: "",
                        look: .bubble(
                            String(format: "%.0f min", o.minutes), routeCostShort(o),
                            o.color.hex, o.id == selectedRouteID)))
            }
        }
        if let t = displayedTrip, !nav.active {
            let coords = t.coordinates.map {
                CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon)
            }
            if let start = coords.first {
                pins.append(
                    MapPin(
                        id: "trip-start", coordinate: start, title: "Départ",
                        look: .marker("flag.fill", "#5856D6")))
            }
            if let end = coords.last {
                pins.append(
                    MapPin(
                        id: "trip-end", coordinate: end, title: "Arrivée",
                        look: .marker("flag.checkered", "#5856D6")))
            }
        }
        for u in liveCars {
            pins.append(
                MapPin(
                    id: "live-\(u.id)",
                    coordinate: CLLocationCoordinate2D(latitude: u.lat, longitude: u.lon),
                    title: u.label, look: .avatar(u.avatar, 44)))
        }
        for a in realtime.alerts {
            pins.append(
                MapPin(
                    id: "alert-\(a.id)",
                    coordinate: CLLocationCoordinate2D(latitude: a.lat, longitude: a.lon),
                    title: NSLocalizedString(a.label.isEmpty ? a.category : a.label, comment: ""),
                    look: .report(emoji(for: a.category), a.confirms ?? 0)))
        }
        return pins
    }

    /// Tracés de la carte, du fond vers le dessus.
    private var mapLines: [MapLine] {
        var lines: [MapLine] = []
        // Options d'itinéraire : chacune sa couleur ; celle prévisualisée est
        // mise en avant (plus épaisse, opaque) et dessinée par-dessus.
        if !routeOptions.isEmpty && !nav.active {
            for o in routeOptions where o.id != selectedRouteID {
                lines.append(
                    MapLine(
                        id: "opt-\(o.id)", coordinates: o.coordinates, color: o.color.hex,
                        width: 5, opacity: 0.45))
            }
            for o in routeOptions where o.id == selectedRouteID {
                lines.append(
                    MapLine(
                        id: "opt-\(o.id)", coordinates: o.coordinates, color: o.color.hex,
                        width: 8))
            }
        } else if !multiRoutes.isEmpty && !nav.active {
            // Plusieurs trajets simultanés (un par point), chacun sa couleur.
            for r in multiRoutes {
                lines.append(
                    MapLine(
                        id: "multi-\(r.id)", coordinates: r.coordinates, color: r.color.hex,
                        width: 6))
            }
        } else if !displayedRoute.isEmpty {
            // Itinéraire routier le plus simple, en vert.
            lines.append(
                MapLine(id: "route", coordinates: displayedRoute, color: "#34C759", width: 7))
        } else if positions.count >= 2 {
            // Repli : tracé direct si le routage n'a rien renvoyé.
            lines.append(
                MapLine(
                    id: "direct",
                    coordinates: positions.map {
                        CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon)
                    },
                    color: "#34C759", width: 5, opacity: 0.5, dashed: true))
        }
        // Trajet enregistré sélectionné dans l'historique (en indigo).
        if let t = displayedTrip, !nav.active {
            lines.append(
                MapLine(
                    id: "trip",
                    coordinates: t.coordinates.map {
                        CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon)
                    },
                    color: "#5856D6", width: 6))
        }
        return lines
    }

    /// Un appui sur une épingle, selon ce qu'elle représente.
    private func handlePinTap(_ id: String) {
        if let raw = id.strippingPrefix("opt-") {
            if let o = routeOptions.first(where: { "\($0.id)" == raw }) {
                withAnimation { selectedRouteID = o.id }
            }
        } else if let raw = id.strippingPrefix("poi-") {
            if let p = pois.first(where: { "\($0.id)" == raw }) {
                Task { await presentRouteOptions(to: p.coordinate) }
            }
        } else if let raw = id.strippingPrefix("alert-") {
            // Toucher un signalement permet de le confirmer/infirmer.
            alertToVote = realtime.alerts.first(where: { "\($0.id)" == raw })
        }
    }

    /// Un geste manuel sur la carte (déplacement ou zoom) débraye le suivi
    /// automatique de la caméra ; il reprend seul 8 s après le dernier geste
    /// (pendant la navigation).
    private func userTouchedMap() {
        lastMapInteraction = Date()
        if followsRoute { followsRoute = false }
    }

    /// Sélectionne l'itinéraire dont le tracé est le plus proche du point touché.
    private func handleMapTap(_ tapped: CLLocationCoordinate2D) {
        guard !nav.active, !routeOptions.isEmpty else { return }
        // Tolérance ≈ largeur d'un doigt : environ 5 % de la largeur visible,
        // ce qui la fait grandir quand on dézoome.
        let visibleMeters =
            (visibleRegion?.span.longitudeDelta ?? 0.05) * 111_320
            * cos(tapped.latitude * .pi / 180)
        let tolerance = max(40, visibleMeters * 0.05)

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
        // En transports, on est un piéton entre deux véhicules.
        case .transit: return "walk"
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

    /// Catégories triées par ordre alphabétique de leur libellé (comparaison
    /// localisée pour gérer les accents, ex. « Hôtels »).
    private var poiKindsSorted: [POIKind] {
        POIKind.allCases.sorted {
            $0.label.localizedStandardCompare($1.label) == .orderedAscending
        }
    }

    /// Pastilles de catégories de lieux (fast-food, hôtels, tourisme…).
    private var poiBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(poiKindsSorted) { kind in
                    let isOn = poiKind == kind
                    Button {
                        togglePOIKind(kind)
                    } label: {
                        HStack(spacing: 5) {
                            Text(kind.emoji)
                            Text(LocalizedStringKey(kind.label)).font(.caption.weight(.semibold))
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
    /// des lieux : déplacement ≥ 15 % de la zone, ou zoom/dézoom ≥ 25 %.
    private func regionChangedEnough(_ r: MKCoordinateRegion) -> Bool {
        guard let last = lastPOIRegion else { return true }
        // Rechargement réactif mais pas incessant : dès ~12 % de déplacement ou
        // ~20 % de zoom. Les identités d'épingles étant stables, un rechargement
        // ne redessine que ce qui a changé.
        let movedLat = abs(r.center.latitude - last.center.latitude)
            > last.span.latitudeDelta * 0.12
        let movedLon = abs(r.center.longitude - last.center.longitude)
            > last.span.longitudeDelta * 0.12
        let zoomed = r.span.latitudeDelta > last.span.latitudeDelta * 1.2
            || r.span.latitudeDelta < last.span.latitudeDelta / 1.2
        return movedLat || movedLon || zoomed
    }

    /// Cherche les lieux de la catégorie dans la zone visible de la carte.
    /// En cas d'échec réseau, les épingles existantes sont conservées.
    private func loadPOIs(_ kind: POIKind) async {
        let region = visibleRegion
            ?? location.coordinate.map {
                MKCoordinateRegion(center: $0, latitudinalMeters: 8000, longitudinalMeters: 8000)
            }
        guard let region else { return }
        // Recherche large : on interroge une zone bien plus grande que l'écran
        // (3×) pour ramener aussi les lieux au-delà des bords visibles, sur
        // toutes les catégories.
        let searchRegion = Self.expandedRegion(region, factor: 3.0)

        // Hors ligne assumé : l'annuaire embarqué répond seul, sans attendre
        // qu'une requête réseau échoue.
        if horsLigne {
            await poisEmbarques(kind, dans: searchRegion, visible: region)
            return
        }

        // Baignade : plages, lacs, piscines… d'OpenStreetMap.
        if kind == .water {
            // Échec réseau → l'annuaire embarqué, puis ce qui est affiché.
            guard let found = await fetchSwimmingSpots(in: searchRegion) else {
                await poisEmbarques(kind, dans: searchRegion, visible: region)
                return
            }
            guard poiKind == kind else { return }
            pois = found
            lastPOIRegion = region
            return
        }

        // Essence : toutes les stations de la zone (open data français), avec
        // le prix du carburant choisi si disponible.
        if kind == .fuel {
            guard let found = await fetchFuelStations(in: searchRegion, type: fuelType) else {
                await poisEmbarques(kind, dans: searchRegion, visible: region)
                return
            }
            guard poiKind == kind else { return }
            pois = found
            lastPOIRegion = region
            return
        }

        // Places handicapé : stationnement PMR d'OpenStreetMap.
        if kind == .parking {
            guard let found = await fetchDisabledParking(in: searchRegion) else {
                await poisEmbarques(kind, dans: searchRegion, visible: region)
                return
            }
            guard poiKind == kind else { return }
            pois = found
            lastPOIRegion = region
            return
        }

        // Catégories OpenStreetMap.
        if kind.estOSM {
            // Échec réseau → l'annuaire embarqué, puis ce qui est affiché.
            guard let found = await fetchOSMCategory(kind, in: searchRegion) else {
                await poisEmbarques(kind, dans: searchRegion, visible: region)
                return
            }
            guard poiKind == kind else { return }
            pois = found
            lastPOIRegion = region
            return
        }

        let req = MKLocalSearch.Request()
        req.naturalLanguageQuery = kind.query
        req.region = searchRegion
        req.resultTypes = .pointOfInterest
        // Échec réseau → l'annuaire embarqué, puis ce qui est affiché.
        guard let resp = try? await MKLocalSearch(request: req).start() else {
            await poisEmbarques(kind, dans: searchRegion, visible: region)
            return
        }
        // La catégorie a pu changer pendant la recherche.
        guard poiKind == kind else { return }
        pois = resp.mapItems.prefix(50).map {
            POI(name: $0.name ?? "Lieu", coordinate: $0.placemark.coordinate)
        }
        lastPOIRegion = region
    }

    /// Le repli sans réseau : l'annuaire embarqué des régions installées.
    /// S'il ne connaît rien ici — aucune région, ou annexe pas encore
    /// téléchargée — l'affichage en place est conservé, comme pour un échec
    /// réseau ordinaire.
    private func poisEmbarques(
        _ kind: POIKind, dans zone: MKCoordinateRegion, visible: MKCoordinateRegion
    ) async {
        let found = await LieuxHorsLigne.shared.lieux(kind, dans: zone)
        guard poiKind == kind, !found.isEmpty else { return }
        pois = found
        lastPOIRegion = visible
    }

    /// Agrandit une région autour de son centre (recherche « agressive » : on
    /// ramène aussi les lieux juste au-delà des bords de l'écran). Span plafonné
    /// pour éviter des requêtes démesurées au dézoom.
    private static func expandedRegion(_ r: MKCoordinateRegion, factor: Double) -> MKCoordinateRegion {
        MKCoordinateRegion(
            center: r.center,
            span: MKCoordinateSpan(
                latitudeDelta: min(r.span.latitudeDelta * factor, 12.0),
                longitudeDelta: min(r.span.longitudeDelta * factor, 12.0)))
    }

    /// Toutes les stations essence de la zone visible via l'open data français
    /// (data.economie.gouv.fr), avec le prix du carburant choisi si disponible.
    /// Renvoie nil en cas d'échec réseau (pour conserver l'affichage actuel).
    /// France uniquement (hors de France : liste vide).
    private func fetchFuelStations(in region: MKCoordinateRegion, type: String) async -> [POI]? {
        // Rayon = moitié de la hauteur visible, plafonné (~40 km) pour rester
        // rapide ; `rows` plafonne déjà le nombre de stations renvoyées.
        let radiusM = min(max(region.span.latitudeDelta * 111_320 / 2, 3000), 60_000)
        var comps = URLComponents(
            string: "https://data.economie.gouv.fr/api/records/1.0/search/")!
        comps.queryItems = [
            URLQueryItem(name: "dataset",
                         value: "prix-des-carburants-en-france-flux-instantane-v2"),
            URLQueryItem(name: "rows", value: "100"),
            URLQueryItem(
                name: "geofilter.distance",
                value: "\(region.center.latitude),\(region.center.longitude),\(Int(radiusM))"),
        ]
        guard let url = comps.url,
            let (data, _) = try? await URLSession.shared.data(from: url),
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let records = json["records"] as? [[String: Any]]
        else { return nil }

        let priceKey = "\(type)_prix"
        var out: [POI] = []
        for record in records {
            guard let fields = record["fields"] as? [String: Any],
                let geom = fields["geom"] as? [Any], geom.count == 2,
                let lat = (geom[0] as? NSNumber)?.doubleValue,
                let lon = (geom[1] as? NSNumber)?.doubleValue
            else { continue }
            // Prix du carburant choisi (Double ou chaîne « 1,72 »), format FR.
            var priceText: String?
            if let v = fields[priceKey] as? Double {
                priceText = String(format: "%.3f", v).replacingOccurrences(of: ".", with: ",") + " €"
            } else if let s = fields[priceKey] as? String,
                let v = Double(s.replacingOccurrences(of: ",", with: ".")) {
                priceText = String(format: "%.3f", v).replacingOccurrences(of: ".", with: ",") + " €"
            }
            let ville = (fields["ville"] as? String) ?? "Station"
            let name = priceText.map { "\($0) · \(ville)" } ?? ville
            out.append(POI(name: name, coordinate: CLLocationCoordinate2D(latitude: lat, longitude: lon)))
        }
        return out
    }

    /// Lieux de baignade (OpenStreetMap) dans la zone visible : plages, zones
    /// de baignade en lac/rivière, piscines publiques, parcs aquatiques 🏊.
    /// Renvoie nil en cas d'échec réseau (pour conserver l'affichage actuel).
    private func fetchSwimmingSpots(in region: MKCoordinateRegion) async -> [POI]? {
        // Dézoom extrême : on plafonne la zone interrogée (~250 km) pour que
        // la requête reste rapide ; `out 60` limite déjà le nombre de lieux.
        let latSpan = min(region.span.latitudeDelta, 2.5)
        let lonSpan = min(region.span.longitudeDelta, 3.5)
        var south = region.center.latitude - latSpan / 2
        var north = region.center.latitude + latSpan / 2
        var west = region.center.longitude - lonSpan / 2
        var east = region.center.longitude + lonSpan / 2

        // Overpass couvre le monde entier : la zone n'est plus découpée à la
        // France. Le plafond d'étendue au-dessus suffit à borner le coût de la
        // requête, où qu'on soit.
        south = max(south, -85)
        north = min(north, 85)
        west = max(west, -180)
        east = min(east, 180)
        guard south < north, west < east else { return [] }

        let bbox = "(\(south),\(west),\(north),\(east))"
        // nwr = nœuds + chemins + relations ; `out center` fournit un point
        // représentatif pour les surfaces (plages, lacs…).
        let q = "[out:json][timeout:15];("
            + "nwr[\"leisure\"=\"swimming_area\"]\(bbox);"
            + "nwr[\"natural\"=\"beach\"]\(bbox);"
            + "nwr[\"leisure\"=\"beach_resort\"]\(bbox);"
            + "nwr[\"leisure\"=\"water_park\"]\(bbox);"
            + "nwr[\"leisure\"=\"sports_centre\"][\"sport\"=\"swimming\"]\(bbox);"
            + ");out center 100;"
        // Serveur principal puis miroir de secours (vérifiés fonctionnels) :
        // on ne renvoie nil que si tous échouent.
        let endpoints = [
            "https://overpass-api.de/api/interpreter",
            "https://maps.mail.ru/osm/tools/overpass/api/interpreter",
        ]
        var data: Data?
        for endpoint in endpoints {
            var comps = URLComponents(string: endpoint)!
            comps.queryItems = [URLQueryItem(name: "data", value: q)]
            guard let url = comps.url else { continue }
            var req = URLRequest(url: url)
            req.timeoutInterval = 20
            if let (d, resp) = try? await URLSession.shared.data(for: req),
                (resp as? HTTPURLResponse)?.statusCode == 200 {
                data = d
                break
            }
        }
        guard let data else { return nil }
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

    /// Exécute une requête Overpass sur la zone visible (plafonnée à latCap ×
    /// lonCap pour rester rapide) et renvoie des POI. `clauses(bbox)` fournit le
    /// corps de l'union ; `name(tags)` calcule le libellé. Renvoie nil en cas
    /// d'échec réseau (pour conserver l'affichage). Chaîne d'endpoints de
    /// secours partagée par toutes les catégories OpenStreetMap.
    private func fetchOverpass(
        in region: MKCoordinateRegion,
        latCap: Double, lonCap: Double, limit: Int,
        clauses: (String) -> String,
        name: ([String: String]?) -> String
    ) async -> [POI]? {
        let latSpan = min(region.span.latitudeDelta, latCap)
        let lonSpan = min(region.span.longitudeDelta, lonCap)
        let south = region.center.latitude - latSpan / 2
        let north = region.center.latitude + latSpan / 2
        let west = region.center.longitude - lonSpan / 2
        let east = region.center.longitude + lonSpan / 2
        let bbox = "(\(south),\(west),\(north),\(east))"
        let q = "[out:json][timeout:20];(\(clauses(bbox)));out center \(limit);"

        let endpoints = [
            "https://overpass-api.de/api/interpreter",
            "https://maps.mail.ru/osm/tools/overpass/api/interpreter",
        ]
        var data: Data?
        for endpoint in endpoints {
            var comps = URLComponents(string: endpoint)!
            comps.queryItems = [URLQueryItem(name: "data", value: q)]
            guard let url = comps.url else { continue }
            var req = URLRequest(url: url)
            req.timeoutInterval = 20
            if let (d, resp) = try? await URLSession.shared.data(for: req),
                (resp as? HTTPURLResponse)?.statusCode == 200 {
                data = d
                break
            }
        }
        guard let data else { return nil }
        struct Resp: Decodable { let elements: [Element] }
        struct Center: Decodable { let lat: Double; let lon: Double }
        struct Element: Decodable {
            let lat: Double?
            let lon: Double?
            let center: Center?
            let tags: [String: String]?
        }
        // Décodage JSON (jusqu'à ~150 éléments) hors du thread principal : il
        // survient en plein pan de carte, à ne surtout pas y bloquer l'UI.
        let elements = await Task.detached {
            try? JSONDecoder().decode(Resp.self, from: data).elements
        }.value
        guard let elements else { return nil }
        var out: [POI] = []
        for el in elements {
            let lat = el.lat ?? el.center?.lat
            let lon = el.lon ?? el.center?.lon
            guard let lat, let lon else { continue }
            out.append(POI(name: name(el.tags),
                           coordinate: CLLocationCoordinate2D(latitude: lat, longitude: lon)))
        }
        return out
    }

    /// Places de stationnement handicapé (PMR) via OpenStreetMap : places
    /// individuelles (`wheelchair=designated` / `parking_space=disabled`) et
    /// parkings annonçant des places réservées (`capacity:disabled`).
    private func fetchDisabledParking(in region: MKCoordinateRegion) async -> [POI]? {
        await fetchOverpass(
            in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
            clauses: { b in
                "nwr[\"amenity\"=\"parking_space\"][\"wheelchair\"=\"designated\"]\(b);"
                    + "nwr[\"amenity\"=\"parking_space\"][\"parking_space\"=\"disabled\"]\(b);"
                    + "nwr[\"amenity\"=\"parking\"][\"capacity:disabled\"]\(b);"
            },
            name: { Self.parkingName(fromTags: $0) })
    }

    /// Catégories OpenStreetMap : parkings 🅿️, recharge ⚡, toilettes 🚻,
    /// surf 🏄, skateparks 🛹.
    private func fetchOSMCategory(_ kind: POIKind, in region: MKCoordinateRegion) async -> [POI]? {
        switch kind {
        case .parkingAll:
            return await fetchOverpass(
                in: region, latCap: 1.2, lonCap: 1.6, limit: 150,
                clauses: { "nwr[\"amenity\"=\"parking\"]\($0);" },
                name: { Self.namedOr($0, "Parking") })
        case .charging:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"amenity\"=\"charging_station\"]\($0);" },
                name: { Self.namedOr($0, "Borne de recharge") })
        case .toilets:
            return await fetchOverpass(
                in: region, latCap: 1.2, lonCap: 1.6, limit: 150,
                clauses: { "nwr[\"amenity\"=\"toilets\"]\($0);" },
                name: { Self.toiletName(fromTags: $0) })
        case .surf:
            // Surf, densifié : spots + écoles (sans les magasins `[!shop]`),
            // TOUTES les plages (c'est là qu'on surfe) et tout ce qui est tagué
            // surfable. Zone large (les spots sont côtiers).
            return await fetchOverpass(
                in: region, latCap: 2.5, lonCap: 3.5, limit: 150,
                clauses: { b in
                    "nwr[\"sport\"=\"surfing\"][!\"shop\"]\(b);"
                        + "nwr[\"natural\"=\"beach\"]\(b);"
                        + "nwr[\"surfing\"=\"yes\"]\(b);"
                },
                name: { Self.surfName(fromTags: $0) })
        case .skate:
            return await fetchOverpass(
                in: region, latCap: 1.5, lonCap: 2.0, limit: 150,
                clauses: { b in
                    "nwr[\"leisure\"=\"skatepark\"]\(b);"
                        + "nwr[\"sport\"=\"skateboard\"]\(b);"
                },
                name: { Self.namedOr($0, "Skatepark") })
        case .pharmacy:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"amenity\"=\"pharmacy\"]\($0);" },
                name: { Self.namedOr($0, "Pharmacie") })
        case .atm:
            return await fetchOverpass(
                in: region, latCap: 0.8, lonCap: 1.1, limit: 150,
                clauses: { "nwr[\"amenity\"=\"atm\"]\($0);" },
                name: { Self.namedOr($0, "Distributeur") })
        case .supermarket:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"shop\"=\"supermarket\"]\($0);" },
                name: { Self.namedOr($0, "Supermarché") })
        case .camping:
            return await fetchOverpass(
                in: region, latCap: 2.5, lonCap: 3.5, limit: 120,
                clauses: { b in
                    "nwr[\"tourism\"=\"camp_site\"]\(b);"
                        + "nwr[\"tourism\"=\"caravan_site\"]\(b);"
                },
                name: { Self.namedOr($0, "Camping") })
        case .drinkingWater:
            return await fetchOverpass(
                in: region, latCap: 0.8, lonCap: 1.1, limit: 150,
                clauses: { "nwr[\"amenity\"=\"drinking_water\"]\($0);" },
                name: { Self.namedOr($0, "Eau potable") })
        case .viewpoint:
            return await fetchOverpass(
                in: region, latCap: 2.5, lonCap: 3.5, limit: 120,
                clauses: { "nwr[\"tourism\"=\"viewpoint\"]\($0);" },
                name: { Self.namedOr($0, "Point de vue") })
        case .bakery:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"shop\"=\"bakery\"]\($0);" },
                name: { Self.namedOr($0, "Boulangerie") })
        case .cafe:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"amenity\"=\"cafe\"]\($0);" },
                name: { Self.namedOr($0, "Café") })
        case .bars:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { b in
                    "nwr[\"amenity\"=\"bar\"]\(b);"
                        + "nwr[\"amenity\"=\"pub\"]\(b);"
                },
                name: { Self.namedOr($0, "Bar") })
        case .bank:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"amenity\"=\"bank\"]\($0);" },
                name: { Self.namedOr($0, "Banque") })
        case .post:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"amenity\"=\"post_office\"]\($0);" },
                name: { Self.namedOr($0, "Bureau de poste") })
        case .hospital:
            // Hôpitaux et cliniques ensemble : quand on cherche l'un, l'autre
            // fait très bien l'affaire.
            return await fetchOverpass(
                in: region, latCap: 1.5, lonCap: 2.0, limit: 150,
                clauses: { b in
                    "nwr[\"amenity\"=\"hospital\"]\(b);"
                        + "nwr[\"amenity\"=\"clinic\"]\(b);"
                },
                name: { Self.namedOr($0, "Hôpital") })
        case .doctor:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"amenity\"=\"doctors\"]\($0);" },
                name: { Self.namedOr($0, "Médecin") })
        case .vet:
            return await fetchOverpass(
                in: region, latCap: 1.5, lonCap: 2.0, limit: 150,
                clauses: { "nwr[\"amenity\"=\"veterinary\"]\($0);" },
                name: { Self.namedOr($0, "Vétérinaire") })
        case .carRepair:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"shop\"=\"car_repair\"]\($0);" },
                name: { Self.namedOr($0, "Garage") })
        case .carWash:
            return await fetchOverpass(
                in: region, latCap: 1.2, lonCap: 1.6, limit: 150,
                clauses: { "nwr[\"amenity\"=\"car_wash\"]\($0);" },
                name: { Self.namedOr($0, "Lavage auto") })
        case .picnic:
            return await fetchOverpass(
                in: region, latCap: 1.5, lonCap: 2.0, limit: 150,
                clauses: { b in
                    "nwr[\"tourism\"=\"picnic_site\"]\(b);"
                        + "nwr[\"leisure\"=\"picnic_table\"]\(b);"
                },
                name: { Self.namedOr($0, "Aire de pique-nique") })
        case .park:
            return await fetchOverpass(
                in: region, latCap: 1.2, lonCap: 1.6, limit: 120,
                clauses: { "nwr[\"leisure\"=\"park\"]\($0);" },
                name: { Self.namedOr($0, "Parc") })
        case .playground:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"leisure\"=\"playground\"]\($0);" },
                name: { Self.namedOr($0, "Aire de jeux") })
        case .trainStation:
            // Les gares voyageurs ; `station=subway` en est écarté pour ne
            // pas noyer une gare TER sous les bouches de métro.
            return await fetchOverpass(
                in: region, latCap: 2.0, lonCap: 2.8, limit: 120,
                clauses: { "nwr[\"railway\"=\"station\"][\"station\"!=\"subway\"]\($0);" },
                name: { Self.namedOr($0, "Gare") })
        case .bikeRental:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"amenity\"=\"bicycle_rental\"]\($0);" },
                name: { Self.namedOr($0, "Vélos en location") })
        case .market:
            return await fetchOverpass(
                in: region, latCap: 1.5, lonCap: 2.0, limit: 150,
                clauses: { "nwr[\"amenity\"=\"marketplace\"]\($0);" },
                name: { Self.namedOr($0, "Marché") })
        case .cinema:
            return await fetchOverpass(
                in: region, latCap: 1.5, lonCap: 2.0, limit: 150,
                clauses: { "nwr[\"amenity\"=\"cinema\"]\($0);" },
                name: { Self.namedOr($0, "Cinéma") })
        case .library:
            return await fetchOverpass(
                in: region, latCap: 1.2, lonCap: 1.6, limit: 150,
                clauses: { "nwr[\"amenity\"=\"library\"]\($0);" },
                name: { Self.namedOr($0, "Bibliothèque") })
        case .gym:
            return await fetchOverpass(
                in: region, latCap: 1.2, lonCap: 1.6, limit: 150,
                clauses: { "nwr[\"leisure\"=\"fitness_centre\"]\($0);" },
                name: { Self.namedOr($0, "Salle de sport") })
        case .grocery:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"shop\"=\"convenience\"]\($0);" },
                name: { Self.namedOr($0, "Épicerie") })
        case .hairdresser:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"shop\"=\"hairdresser\"]\($0);" },
                name: { Self.namedOr($0, "Coiffeur") })
        case .recycling:
            return await fetchOverpass(
                in: region, latCap: 1.0, lonCap: 1.4, limit: 150,
                clauses: { "nwr[\"amenity\"=\"recycling\"]\($0);" },
                name: { Self.namedOr($0, "Point de recyclage") })
        case .restArea:
            // Les aires d'autoroute, repos comme services : c'est sur la
            // route qu'on les cherche, la zone interrogée est donc large.
            return await fetchOverpass(
                in: region, latCap: 2.5, lonCap: 3.5, limit: 120,
                clauses: { b in
                    "nwr[\"highway\"=\"rest_area\"]\(b);"
                        + "nwr[\"highway\"=\"services\"]\(b);"
                },
                name: { Self.namedOr($0, "Aire de repos") })
        default:
            return nil
        }
    }

    /// Nom OSM (`name`) s'il existe, sinon le libellé par défaut fourni.
    private static func namedOr(_ tags: [String: String]?, _ fallback: String) -> String {
        if let name = tags?["name"], !name.isEmpty { return name }
        return fallback
    }

    /// Libellé d'un lieu de surf : nom OSM s'il existe (ex. « Plage de la
    /// Piste »), sinon « Plage » pour une plage, « Spot de surf » sinon.
    private static func surfName(fromTags tags: [String: String]?) -> String {
        if let name = tags?["name"], !name.isEmpty { return name }
        if tags?["natural"] == "beach" { return "Plage" }
        return "Spot de surf"
    }

    /// Libellé de toilettes : signale l'accès PMR et le caractère payant.
    private static func toiletName(fromTags tags: [String: String]?) -> String {
        var s = namedOr(tags, "Toilettes")
        if tags?["wheelchair"] == "yes" { s += " ♿" }
        if tags?["fee"] == "yes" { s += " · payant" }
        return s
    }

    /// Libellé d'une place / d'un parking handicapé selon ses tags OSM.
    private static func parkingName(fromTags tags: [String: String]?) -> String {
        if let cap = tags?["capacity:disabled"], let n = Int(cap), n > 0 {
            let base = tags?["name"] ?? "Parking"
            return "\(base) · \(n) place\(n > 1 ? "s" : "") PMR"
        }
        if let name = tags?["name"], !name.isEmpty { return name }
        return "Place PMR"
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

    /// Le badge hors ligne. Deux états, deux couleurs : l'orange dit que le
    /// réseau est réellement coupé (constaté par le moniteur), le bleu que le
    /// mode hors ligne est un choix. Dans les deux cas, la même certitude :
    /// l'itinéraire se calcule sur l'appareil.
    @ViewBuilder private var badgeHorsLigne: some View {
        if reseau.coupe {
            HStack(spacing: 6) {
                Image(systemName: "wifi.slash")
                Text("Sans réseau — calcul sur l'appareil")
            }
            .font(.caption.weight(.bold))
            .foregroundStyle(.white)
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .background(Color.orange.gradient, in: Capsule())
            .shadow(color: .black.opacity(0.2), radius: 5, y: 2)
        } else if horsLigne {
            HStack(spacing: 6) {
                Image(systemName: "airplane")
                Text("Hors ligne choisi — calcul sur l'appareil")
            }
            .font(.caption.weight(.bold))
            .foregroundStyle(.white)
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .background(Color.blue.gradient, in: Capsule())
            .shadow(color: .black.opacity(0.2), radius: 5, y: 2)
        }
    }

    private var navBanner: some View {
        HStack(spacing: 14) {
            // La flèche suit la manœuvre réelle (elle était figée sur « à
            // droite », même pour un virage à gauche).
            Image(systemName: nav.rerouting
                ? "arrow.triangle.2.circlepath"
                : nav.maneuver.icon)
                .font(.title.weight(.bold))
                .foregroundStyle(.white)
                .symbolEffect(.pulse, isActive: nav.rerouting)
            VStack(alignment: .leading, spacing: 2) {
                if !nav.rerouting, nav.distanceToNext > 0 {
                    Text("\(Int(nav.distanceToNext)) m").font(.title3.weight(.bold)).foregroundStyle(.white)
                }
                Text(nav.instruction)
                    .font(.title3.weight(.bold))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                // Précision utile (numéro de sortie, virage serré) : sur une
                // seconde ligne, pour garder la consigne lisible d'un coup d'œil.
                if let detail = nav.detail, !nav.rerouting {
                    Text(detail)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(0.85))
                        .lineLimit(1)
                }
            }
            Spacer()

            // Les voies à l'approche, gauche → droite : la file qui mène la
            // manœuvre en plein blanc, les autres estompées. À moins de
            // 400 m seulement — avant, on ne se rabat pas encore — et jamais
            // pendant un recalcul. Seuls les itinéraires Valhalla les
            // portent : MapKit ne publie pas les voies.
            if !nav.rerouting, !nav.voies.isEmpty, nav.distanceToNext < 400 {
                HStack(spacing: 5) {
                    ForEach(Array(nav.voies.enumerated()), id: \.offset) { _, v in
                        Image(systemName: v.symbole)
                            .font(.callout.weight(.bold))
                            .foregroundStyle(.white.opacity(v.active ? 1 : 0.35))
                    }
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .background(.black.opacity(0.18), in: RoundedRectangle(cornerRadius: 10))
            }
        }
        .padding(16)
        .background((nav.rerouting ? Color.orange : Color.green).gradient, in: RoundedRectangle(cornerRadius: 18))
        .shadow(color: .black.opacity(0.2), radius: 8, y: 4)
    }

    /// À l'approche : un mot prêt à envoyer, avec le lien live si le partage
    /// tourne — celui qui attend n'a plus à demander « t'es où ? ».
    private var arriveeBanner: some View {
        HStack(spacing: 10) {
            Image(systemName: "hand.wave.fill").foregroundStyle(.orange)
            Button {
                shareArrivee()
            } label: {
                Text(
                    String(
                        format: NSLocalizedString(
                            "J'arrive dans ~%d min — prévenir ?", comment: ""),
                        max(1, Int(nav.etaMinutes.rounded()))))
                    .font(.subheadline.weight(.semibold))
                    .multilineTextAlignment(.leading)
            }
            Spacer()
            Button {
                proposeArrivee = false
            } label: {
                Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
            }
        }
        .padding(12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
        .shadow(color: .black.opacity(0.15), radius: 6, y: 3)
    }

    /// Le message d'approche : minutes restantes, et le lien de suivi en
    /// direct quand il existe.
    private func shareArrivee() {
        proposeArrivee = false
        let mins = max(1, Int(nav.etaMinutes.rounded()))
        var texte = String(
            format: NSLocalizedString("J'arrive dans %d min.", comment: ""), mins)
        if liveShare.active, let url = liveShare.shareURL {
            texte +=
                " "
                + String(
                    format: NSLocalizedString("Suis-moi en direct : %@", comment: ""),
                    url.absoluteString)
        }
        Sharer.present([texte])
    }

    private var etaCard: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(String(format: "%.0f min", nav.etaMinutes)).font(.title3.weight(.bold))
                    // L'heure à côté des minutes : l'une répond à « c'est
                    // encore long ? », l'autre à « j'y suis quand ? ».
                    Text(verbatim: heureArriveeCourte)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.orange)
                }
                Text(String(format: NSLocalizedString("%.1f km restants", comment: ""), nav.remainingKm))
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

    /// « 18:42 » : l'heure d'arrivée, dans le format de la langue de l'app.
    private var heureArriveeCourte: String {
        let f = DateFormatter()
        f.locale = Locale(identifier: Bundle.main.preferredLocalizations.first ?? "fr")
        f.setLocalizedDateFormatFromTemplate("Hm")
        return f.string(from: Date().addingTimeInterval(nav.etaMinutes * 60))
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

    /// L'âge d'une mesure, dès qu'il compte : rien avant deux heures — la
    /// donnée est horaire, plus frais n'apprend rien — puis « il y a N h ».
    /// C'est ce qui rend honnête une pastille resservie sans réseau.
    private func ageTexte(_ date: Date?) -> String? {
        guard let date else { return nil }
        let heures = Int(Date().timeIntervalSince(date) / 3600)
        guard heures >= 2 else { return nil }
        return String(format: NSLocalizedString("il y a %d h", comment: ""), heures)
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
    ///
    /// En transports, ni kilomètres ni virages — la durée, les
    /// correspondances et les heures réelles, qui sont ce qu'on compare. Trois
    /// libellés entiers plutôt qu'un pluriel bricolé dans la phrase : une
    /// interpolation `%d correspondance(s)` fabriquerait une clé illisible
    /// pour les traducteurs.
    private func routeCoreText(_ o: RouteOption) -> String {
        if o.transit != nil {
            let horaire = o.horaire ?? routeArrival(o.minutes)
            switch o.turns {
            case 0:
                return String(
                    format: NSLocalizedString("%.0f min · direct · %@", comment: ""),
                    o.minutes, horaire)
            case 1:
                return String(
                    format: NSLocalizedString("%.0f min · 1 correspondance · %@", comment: ""),
                    o.minutes, horaire)
            default:
                return String(
                    format: NSLocalizedString("%.0f min · %d correspondances · %@", comment: ""),
                    o.minutes, o.turns, horaire)
            }
        }
        return String(format: NSLocalizedString("%.0f min · %.1f km · %d virages · arr. %@", comment: ""),
               o.minutes, o.km, o.turns, routeArrival(o.minutes))
    }

    /// La chaîne des lignes d'un itinéraire en transports : les pastilles des
    /// véhicules, la marche réduite à son pictogramme.
    private func transitChaine(_ legs: [TransitLeg]) -> some View {
        // Défilement horizontal plutôt qu'étirement : une chaîne de cinq
        // tronçons ne se comprime pas, et sans ce fourreau elle imposait sa
        // largeur à toute la carte d'option — qui débordait de l'écran, rognée
        // des deux côtés. Ici la chaîne longue se fait glisser du doigt, et la
        // rangée garde la largeur de l'écran.
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 4) {
                ForEach(Array(legs.enumerated()), id: \.offset) { i, l in
                    if i > 0 {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 7, weight: .bold))
                            .foregroundStyle(.secondary)
                    }
                    if l.estVehicule {
                        HStack(spacing: 3) {
                            Image(systemName: l.symbole).font(.caption2)
                            if let ligne = l.ligne {
                                Text(verbatim: ligne)
                                    .font(.caption2.weight(.bold))
                                    .lineLimit(1)
                                    .fixedSize()
                            }
                            // Le retard temps réel, dans la pastille même :
                            // « +3′ » à côté de la ligne. Une minute de seuil,
                            // en deçà c'est du bruit d'horloge.
                            if let r = l.retard_s, abs(r) >= 60 {
                                Text(verbatim: r > 0 ? "+\(r / 60)′" : "\(r / 60)′")
                                    .font(.caption2.weight(.heavy))
                                    .lineLimit(1)
                                    .fixedSize()
                            }
                        }
                        .padding(.vertical, 2).padding(.horizontal, 5)
                        .background(l.teinte.opacity(0.9), in: RoundedRectangle(cornerRadius: 5))
                        .foregroundStyle(.white)
                    } else {
                        Image(systemName: "figure.walk")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
        // Une hauteur tenue : la pastille est un insigne, pas une vignette —
        // rien ne doit pouvoir la faire enfler avec la rangée.
        .frame(height: 20)
    }

    /// Dénivelé (côte ↗ / descente ↘) si connu.
    private func routeElevationText(_ o: RouteOption) -> String? {
        guard let up = o.climb, let down = o.descent, up >= 5 || down >= 5 else { return nil }
        return String(format: NSLocalizedString("↗ %.0f m de côte · ↘ %.0f m de descente", comment: ""), up, down)
    }

    /// Seuil au-delà duquel une côte / descente est jugée « grosse » (mètres).
    private let bigElevation = 150.0

    /// Avertissement si l'itinéraire présente une grosse côte / descente.
    private func routeWarning(_ o: RouteOption) -> String? {
        let bigUp = (o.climb ?? 0) >= bigElevation
        let bigDown = (o.descent ?? 0) >= bigElevation
        switch (bigUp, bigDown) {
        case (true, true):
            return String(format: NSLocalizedString("⚠️ Forte côte (%.0f m) et forte descente (%.0f m)", comment: ""),
                          o.climb ?? 0, o.descent ?? 0)
        case (true, false):
            return String(format: NSLocalizedString("⚠️ Forte côte (%.0f m)", comment: ""), o.climb ?? 0)
        case (false, true):
            return String(format: NSLocalizedString("⚠️ Forte descente (%.0f m)", comment: ""), o.descent ?? 0)
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
            return String(format: NSLocalizedString("⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €", comment: ""),
                          c.fuel, c.toll, c.fuel + c.toll)
        }
        return String(format: NSLocalizedString("⛽ ~%.2f € · sans péage", comment: ""), c.fuel)
    }

    /// Coût total compact (« ~19,70 € ») pour les bulles sur la carte.
    private func routeCostShort(_ o: RouteOption) -> String? {
        guard let c = routeCosts(o) else { return nil }
        return String(format: "~%.2f €", c.fuel + c.toll)
    }

    /// Infos détaillées : ligne principale + dénivelé sur une ligne à part.
    private func routeDetails(_ o: RouteOption, compact: Bool = false) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            if let legs = o.transit {
                transitChaine(legs)
            }
            // En transports, une seule ligne quoi qu'il arrive : coupée en
            // deux, l'heure « 06:23 → 07:17 » perdait son zéro au pli et se
            // lisait 7:17. Mieux vaut une ligne qui se serre un peu.
            Text(routeCoreText(o))
                .lineLimit(compact || o.transit != nil ? 1 : 2)
                .minimumScaleFactor(compact || o.transit != nil ? 0.6 : 0.85)
            // Ce que cette variante-là rencontre : un feu sur son tracé, la
            // pluie à mi-chemin. Propre à chacune — c'est parfois précisément
            // ce qui les départage.
            ForEach(risquesParOption[o.id] ?? [], id: \.self) { risque in
                Text(risque)
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(risque.hasPrefix("🔥") ? .red : .orange)
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
            }
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
            // Choix du mode (recalcule les itinéraires). Des pictogrammes
            // seuls : à quatre onglets, les libellés ne tiennent plus dans un
            // segment — « Transports » sortait de l'écran — et l'icône se lit
            // plus vite qu'un mot. Le nom reste dit au lecteur d'écran.
            HStack(spacing: 8) {
                Picker("Mode", selection: $travelMode) {
                    ForEach(TravelMode.allCases) { m in
                        Image(systemName: m.icon)
                            .accessibilityLabel(Text(LocalizedStringKey(m.label)))
                            .tag(m)
                    }
                }
                .pickerStyle(.segmented)
                .onChange(of: travelMode) { _, _ in
                    if let dest = pendingDestination {
                        Task { await presentRouteOptions(to: dest) }
                    }
                }
                // « Partir à l'heure » : l'app cale l'heure de départ sur le
                // trajet choisi et pose le rappel.
                if selected != nil {
                    Button {
                        showRappelDepart = true
                    } label: {
                        Image(systemName: "bell.badge")
                            .font(.subheadline.weight(.semibold))
                    }
                    .buttonStyle(.bordered)
                    .tint(.indigo)
                    .accessibilityLabel(Text("Partir à l'heure"))
                }
            }

            // Les étapes du trajet, dans l'ordre, chacune retirable — et le
            // bouton qui arme la pose de la suivante sur la carte. Pas en
            // transports : MOTIS ne chaîne pas les arrêts.
            if travelMode != .transit, !etapes.isEmpty || pendingDestination != nil {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(Array(etapeNoms.enumerated()), id: \.offset) { i, nom in
                            let arret = etapeArrets.indices.contains(i) ? etapeArrets[i] : 0
                            let retard = enRetardSurCreneau(i)
                            HStack(spacing: 4) {
                                // Toucher le nom ouvre le temps d'arrêt : ce
                                // qu'on règle le plus souvent sur une étape.
                                Menu {
                                    Picker("Temps d'arrêt", selection: lienArret(i)) {
                                        Text("Sans arrêt").tag(0)
                                        ForEach([5, 15, 30], id: \.self) { m in
                                            Text(verbatim: "\(m) min").tag(m)
                                        }
                                        Text(verbatim: "1 h").tag(60)
                                    }
                                    // Le créneau de livraison — l'outil Pro
                                    // du tenu-par-l'heure : « avant 10 h »,
                                    // et le chip s'alarme si l'heure estimée
                                    // de passage le dépasse.
                                    if plus.accesPro {
                                        Picker("Avant", selection: lienCreneau(i)) {
                                            Text("Sans créneau").tag(0)
                                            ForEach(
                                                Array(stride(from: 6 * 60, through: 20 * 60, by: 60)),
                                                id: \.self
                                            ) { m in
                                                Text(verbatim: String(format: "%02d:00", m / 60))
                                                    .tag(m)
                                            }
                                        }
                                    }
                                } label: {
                                    Text(
                                        verbatim: (retard ? "⚠️ " : "")
                                            + (arret > 0
                                                ? "\(i + 1) · \(nom) · \(arret >= 60 ? "1 h" : "\(arret)′")"
                                                : "\(i + 1) · \(nom)")
                                            + creneauSuffixe(i)
                                    )
                                    .font(.caption2.weight(.semibold))
                                    .lineLimit(1)
                                    .foregroundStyle(.primary)
                                }
                                Button { retireEtape(i) } label: {
                                    Image(systemName: "xmark")
                                        .font(.system(size: 9, weight: .bold))
                                        .foregroundStyle(.secondary)
                                }
                                .buttonStyle(.plain)
                            }
                            .padding(.horizontal, 8)
                            .padding(.vertical, 4)
                            .background(
                                (retard ? Color.orange.opacity(0.3) : Color.indigo.opacity(0.15)),
                                in: Capsule())
                        }
                        Button {
                            ajouteEtape.toggle()
                        } label: {
                            Label(
                                ajouteEtape ? "Touche la carte" : "Étape",
                                systemImage: ajouteEtape ? "hand.tap.fill" : "plus")
                                .font(.caption.weight(.semibold))
                        }
                        .buttonStyle(.bordered)
                        .tint(ajouteEtape ? .orange : .indigo)
                        .controlSize(.mini)
                        // La station la moins chère le long du trajet — en
                        // voiture seulement : un vélo ne fait pas le plein.
                        if travelMode == .car, !routeOptions.isEmpty {
                            Button {
                                if plus.accesPlus {
                                    showStationsCarburant = true
                                } else {
                                    showPaywall = true
                                }
                            } label: {
                                Label("Carburant", systemImage: "fuelpump.fill")
                                    .font(.caption.weight(.semibold))
                            }
                            .buttonStyle(.bordered)
                            .tint(.green)
                            .controlSize(.mini)
                        }
                        // La tournée collée d'un bloc — l'outil Pro du
                        // livreur : une adresse par ligne, chaque ligne
                        // devient un arrêt.
                        Button {
                            if plus.accesPro {
                                showCollerAdresses = true
                            } else {
                                showPaywall = true
                            }
                        } label: {
                            Label("Coller des adresses", systemImage: "list.clipboard")
                                .font(.caption.weight(.semibold))
                        }
                        .buttonStyle(.bordered)
                        .tint(.indigo)
                        .controlSize(.mini)
                        // À partir de deux étapes, l'ordre devient un choix —
                        // et la géométrie sait le faire mieux que la mémoire.
                        if etapes.count >= 2 {
                            Button {
                                // L'optimisation de tournées est un outil
                                // Pro — celui de ceux qui roulent pour
                                // travailler : sans la formule, le bouton
                                // mène au paywall — les étapes elles-mêmes
                                // restent posables, dans l'ordre où on veut.
                                if plus.accesPro {
                                    ordonneAuPlusCourt()
                                } else {
                                    showPaywall = true
                                }
                            } label: {
                                Label("Au plus court", systemImage: "arrow.triangle.swap")
                                    .font(.caption.weight(.semibold))
                            }
                            .buttonStyle(.bordered)
                            .tint(.teal)
                            .controlSize(.mini)
                        }
                    }
                }
                .frame(height: 30)
            }

            // Ce qui attend à l'arrivée vaut pour toutes les variantes :
            // dit une fois, ici, plutôt que répété sur chaque carte.
            ForEach(risquesArrivee, id: \.self) { r in
                Text(r)
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.orange)
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
            }

            // En transports, l'heure est un paramètre du calcul, pas un
            // détail : le menu choisit le sens (partir / arriver avant), la
            // roue choisit l'heure, et tout changement recalcule.
            if travelMode == .transit {
                HStack(spacing: 10) {
                    Menu {
                        ForEach(TransitQuand.allCases) { q in
                            Button {
                                transitQuand = q
                                if let dest = pendingDestination {
                                    Task { await presentRouteOptions(to: dest) }
                                }
                            } label: {
                                if q == transitQuand {
                                    Label(LocalizedStringKey(q.libelle), systemImage: "checkmark")
                                } else {
                                    Text(LocalizedStringKey(q.libelle))
                                }
                            }
                        }
                    } label: {
                        HStack(spacing: 4) {
                            Image(systemName: "clock")
                            Text(LocalizedStringKey(transitQuand.libelle))
                            Image(systemName: "chevron.up.chevron.down").font(.caption2)
                        }
                        .font(.caption.weight(.semibold))
                    }
                    if transitQuand != .maintenant {
                        DatePicker(
                            "", selection: $transitDate,
                            displayedComponents: [.date, .hourAndMinute]
                        )
                        .labelsHidden()
                        .onChange(of: transitDate) { _, _ in
                            if let dest = pendingDestination {
                                Task { await presentRouteOptions(to: dest) }
                            }
                        }
                    }
                    Spacer(minLength: 0)
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
                            Text(LocalizedStringKey(routeTags(o, fastestID: fastestID, shortestID: shortestID).first?.text ?? "Itinéraire"))
                                .font(.subheadline.weight(.semibold)).lineLimit(1)
                            routeDetails(o, compact: true)
                        }
                    }
                    Spacer(minLength: 6)
                    // Replié, la liste s'ouvre en remontant depuis le bas de
                    // l'écran : la flèche pointe vers le haut, le sens du
                    // geste. Dépliée, elle pointe vers le bas, où tout se
                    // rabat. L'inverse — la convention des listes qui
                    // s'ouvrent vers le bas — mentait ici sur les deux sens.
                    Image(systemName: routesExpanded ? "chevron.down" : "chevron.up")
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
                                                Text(LocalizedStringKey(tag.text))
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
                // Assez pour cinq itinéraires d'un coup d'œil. Au-delà on
                // masquerait la carte, où les tracés se comparent vraiment.
                .frame(maxHeight: 360)
            }

            HStack(spacing: 10) {
                Button {
                    if let o = selected { startNavigation(option: o) }
                } label: {
                    // Hauteur bornée et texte qui se serre : sans elles, une
                    // grande taille de texte étirait ce bouton en colonne
                    // pleine hauteur (constaté sur appareil).
                    Label("Démarrer", systemImage: "location.north.line.fill")
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                        .frame(maxWidth: .infinity)
                        .frame(height: 34)
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

                // Le trajet part en lien : une page web chez le courtier, qui
                // tend le lien profond vers l'app à qui l'a — et Plans ou
                // Google Maps à qui ne l'a pas.
                Button { partageTrajet() } label: {
                    Image(systemName: "square.and.arrow.up")
                        .font(.subheadline.weight(.semibold))
                        .frame(width: 40, height: 34)
                }
                .buttonStyle(.bordered)
                .tint(.indigo)
                .disabled(pendingDestination == nil)
                .accessibilityLabel(Text("Partager l'itinéraire"))

                Button {
                    previewTask?.cancel()
                    previewing = false
                    routeOptions = []
                    selectedRouteID = nil
                    routesExpanded = false
                    pendingDestination = nil
                    etapes = []
                    etapeNoms = []
                    etapeArrets = []
                    etapesAtteintes = []
                    ajouteEtape = false
                    traceGPX = nil
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
        // Comparer des itinéraires, c'est regarder des tracés colorés sur la
        // carte et une liste en bas. Tout le reste — météo, vitesse à l'arrêt,
        // cartes thématiques, 3D, signalement — masquait précisément ce qu'on
        // cherche à voir, et n'a aucun usage à cet instant. Même raison qu'au
        // volant, autre moment. Seul « recentrer » reste : on veut souvent
        // reprendre le cadrage pendant qu'on choisit.
        let compare = !routeOptions.isEmpty || !multiRoutes.isEmpty
        return HStack(alignment: .bottom) {
            if !compare {
                VStack(alignment: .leading, spacing: 8) {
                    weatherPill
                    airPill
                    speedPill
                }
            }
            Spacer()
            VStack(spacing: 14) {
                if compare {
                    circleButton(system: "location.fill", tint: .blue) { recenter() }
                } else {
                    mapPanel
                    reportButton
                }
            }
        }
    }

    /// Les commandes de la carte rassemblées en un seul panneau arrondi :
    /// mêmes gestes qu'avant, sans l'éparpillement de six ronds flottants.
    /// Les cartes thématiques (feux, air, hors ligne) se replient dans un
    /// menu — elles se consultent à l'arrêt, pas entre deux coups d'œil à la
    /// route, et pendant le guidage le menu disparaît entièrement.
    private var mapPanel: some View {
        VStack(spacing: 0) {
            if !nav.active {
                cartesMenu
                Divider().frame(width: 26)
            }
            panelButton(system: "view.3d", tint: is3D ? .white : .blue,
                        actif: is3D, libelle: "Vue 3D") { toggle3D() }
            Divider().frame(width: 26)
            panelButton(system: "location.fill", tint: .blue,
                        libelle: "Recentrer sur ma position") { recenter() }
        }
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 24))
        .shadow(color: .black.opacity(0.15), radius: 6, y: 3)
    }

    /// Une commande du panneau : glyphe seul, cible pleine largeur. L'état
    /// actif (3D enclenchée) se signale par un disque bleu sous le glyphe.
    private func panelButton(system: String, tint: Color, actif: Bool = false,
                             libelle: LocalizedStringKey,
                             action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: system)
                .font(.title3)
                .foregroundStyle(tint)
                .frame(width: 52, height: 50)
                .background {
                    if actif { Circle().fill(Color.blue).padding(5) }
                }
                .contentShape(Rectangle())
        }
        .accessibilityLabel(Text(libelle))
    }

    /// Le menu des cartes thématiques : feux de forêt, qualité de l'air,
    /// fonds hors ligne — trois écrans plein format derrière une seule
    /// entrée.
    private var cartesMenu: some View {
        Menu {
            Button {
                fires.refresh(key: firmsKey)
                showFireMap = true
            } label: {
                Label("Carte des feux de forêt", systemImage: "flame.fill")
            }
            Button { showAirMap = true } label: {
                Label("Qualité de l'air", systemImage: "aqi.medium")
            }
            Button { showOfflineMap = true } label: {
                Label("Carte hors ligne", systemImage: "square.and.arrow.down.on.square")
            }
        } label: {
            Image(systemName: "square.3.layers.3d.top.filled")
                .font(.title3)
                .foregroundStyle(.blue)
                .frame(width: 52, height: 50)
                .contentShape(Rectangle())
        }
        .accessibilityLabel(Text("Cartes"))
    }

    /// Météo en temps réel (température + conditions), tout en haut des pastilles.
    @ViewBuilder private var weatherPill: some View {
        if let temp = weather.temperature, let code = weather.code {
            let w = WeatherService.describe(code)
            HStack(spacing: 6) {
                Text(w.emoji).font(.subheadline)
                Text("\(temp)°").font(.subheadline.weight(.bold))
                Text(LocalizedStringKey(w.label)).font(.caption2).foregroundStyle(.secondary)
                if let age = ageTexte(weather.mesureA) {
                    Text(age).font(.caption2).foregroundStyle(.tertiary)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(.regularMaterial, in: Capsule())
            .overlay(Capsule().stroke(.blue.opacity(0.35), lineWidth: 1.5))
            .shadow(color: .black.opacity(0.15), radius: 5, y: 2)
        }
    }

    /// La qualité de l'air d'ici, en pastille sous la météo : le rond de la
    /// couleur du niveau, le chiffre, le palier. Toucher la pastille ouvre la
    /// carte dédiée — c'est la même donnée, en grand.
    @ViewBuilder private var airPill: some View {
        if let p = airAutour.point, let v = p.valeur {
            Button {
                showAirMap = true
            } label: {
                HStack(spacing: 6) {
                    Circle()
                        .fill(Color(AirCouleurs.couleur(p.niveau)))
                        .frame(width: 10, height: 10)
                    Text(verbatim: "\(Int(v))")
                        .font(.subheadline.weight(.bold))
                        .monospacedDigit()
                        .foregroundStyle(.primary)
                    Text(AirCouleurs.libelles[min(p.niveau, AirCouleurs.libelles.count - 1)])
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    if let age = ageTexte(airAutour.mesureA) {
                        Text(age).font(.caption2).foregroundStyle(.tertiary)
                    }
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
                .background(.regularMaterial, in: Capsule())
                .overlay(Capsule().stroke(.teal.opacity(0.35), lineWidth: 1.5))
                .shadow(color: .black.opacity(0.15), radius: 5, y: 2)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("Qualité de l'air"))
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
                .background(Color.blue, in: Circle())
                .shadow(color: .blue.opacity(0.5), radius: 8, y: 4)
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
                            Image(systemName: m.icon)
                        .accessibilityLabel(Text(LocalizedStringKey(m.label)))
                        .tag(m)
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
                    // Le vélo a son profil : Valhalla distingue le vélo de
                    // route, celui de ville et le VTT, et sait fuir les côtes.
                    // Ces réglages ne portent que sur le calcul local — MapKit
                    // n'a pas de profil vélo, c'est même pour ça qu'on lui a
                    // ajouté Valhalla.
                    if travelMode == .bike {
                        Picker("Type de vélo", selection: $veloType) {
                            Text("Route").tag("Road")
                            Text("Ville").tag("Hybrid")
                            Text("VTT").tag("Mountain")
                        }
                        .pickerStyle(.segmented)
                        Toggle(isOn: $veloEviteCotes) {
                            Label("Éviter les côtes", systemImage: "mountain.2")
                        }
                    }
                }
                // Notifications d'alertes à proximité.
                Section("Alertes") {
                    Toggle(isOn: $notifyNearbyAlerts) {
                        Label("Alertes à proximité", systemImage: "bell.badge")
                    }
                    Text("Reçois une notification quand un signalement (police, accident, danger…) se trouve à moins de 600 m, même écran verrouillé.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                // Incendies (NASA FIRMS) : clé optionnelle saisie sur l'appareil
                // (sinon le serveur fournit les feux si configuré).
                Section("Incendies 🔥 (NASA FIRMS)") {
                    TextField("Clé FIRMS (MAP_KEY)", text: $firmsKey)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .font(.system(.body, design: .monospaced))
                    Link(
                        destination: URL(
                            string: "https://firms.modaps.eosdis.nasa.gov/api/map_key/")!
                    ) {
                        Label("Obtenir une clé", systemImage: "link")
                    }
                    .font(.footnote)
                    Text(
                        "Colle ta clé FIRMS pour afficher les vrais foyers (carte des feux, menu) sans attendre le serveur. \(fires.fires.count) foyer(s) actuellement."
                    )
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                }
                Section("Hors ligne 🗺️") {
                    Button {
                        showPlaces = false
                        showOfflineMap = true
                    } label: {
                        Label("Préparer une zone hors ligne", systemImage: "arrow.down.circle")
                    }
                    // Un seul écran, un seul geste. Les deux téléchargements y
                    // portent sur la même zone : les images pour voir la carte,
                    // le graphe routier pour y calculer une route. Séparés, on
                    // en prenait un et on découvrait l'autre en panne de
                    // réseau.
                    Text(
                        "Les fonds de carte et le calcul d'itinéraire se téléchargent ensemble, sur la zone que tu regardes. Aucune clé n'est nécessaire : tout vient d'OpenStreetMap, libre et sans inscription."
                    )
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    if router.ready {
                        Toggle("Naviguer hors ligne", isOn: $horsLigne)
                        Text("\(router.installees.count) région(s) · \(router.poidsTotal)")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }

                    DisclosureGroup("Serveur de cartes personnel") {
                        TextField("https://exemple.fr/style.json", text: $styleURL)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .keyboardType(.URL)
                            .font(.system(.footnote, design: .monospaced))
                        if !styleURL.isEmpty {
                            Button(role: .destructive) {
                                styleURL = ""
                                tileURL = ""
                            } label: {
                                Label("Revenir au fond par défaut", systemImage: "arrow.uturn.backward")
                            }
                        }
                        Text(
                            "Laisse vide si tu n'héberges rien. Une adresse qui ne répond pas est ignorée : la carte reste affichée avec le fond par défaut."
                        )
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                    }
                }
                // Une trace GPX venue d'ailleurs — Komoot, Strava, un ami —
                // devient un itinéraire suivable. Tout est local : le fichier,
                // le parseur, le guidage. C'est le trajet qui marche le mieux
                // en mode avion, parce qu'il n'a jamais eu besoin de réseau.
                Section("Trace GPX 🥾") {
                    Button {
                        if plus.accesPlus {
                            importeGPX = true
                        } else {
                            showPaywallRecherche = true
                        }
                    } label: {
                        Label("Importer une trace GPX", systemImage: "square.and.arrow.down")
                    }
                    .fileImporter(
                        isPresented: $importeGPX,
                        allowedContentTypes: [
                            UTType(filenameExtension: "gpx") ?? .xml, .xml,
                        ]
                    ) { resultat in
                        chargeGPX(resultat)
                    }
                    Text(
                        "Une randonnée, un parcours vélo exporté d'une autre app : la trace s'affiche sur la carte et se laisse suivre pas à pas, même sans réseau."
                    )
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                }
                // Trafic temps réel TomTom : clé optionnelle saisie sur
                // l'appareil (sinon le serveur fournit le trafic si configuré).
                Section("Trafic 🚦 (TomTom)") {
                    TextField("Clé API TomTom", text: $tomtomKey)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .font(.system(.body, design: .monospaced))
                    Link(destination: URL(string: "https://developer.tomtom.com/user/me/apps")!) {
                        Label("Obtenir une clé (2 500 req./jour)", systemImage: "link")
                    }
                    .font(.footnote)
                    if tomtomKey.trimmingCharacters(in: .whitespaces).isEmpty {
                        Text(
                            "Colle ta clé TomTom pour activer les vrais bouchons en temps réel : la navigation contournera automatiquement les embouteillages sur ta route."
                        )
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                    } else {
                        Text("\(traffic.jams.count) bouchon(s) détecté(s) autour de toi (temps réel).")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
                // Raccourcis Domicile / Travail — poussés au compte à chaque
                // changement : ils suivent l'utilisateur, pas l'appareil.
                Section("Favoris") {
                    favoriteRow(icon: "house.fill", title: "Domicile", place: homePlace) {
                        homePlace = $0
                        Session.home = $0
                        pousseFavoris()
                    }
                    favoriteRow(icon: "briefcase.fill", title: "Travail", place: workPlace) {
                        workPlace = $0
                        Session.work = $0
                        pousseFavoris()
                    }
                }
                // Sans réseau, la recherche d'adresse ne rendra jamais rien :
                // MKLocalSearch interroge les serveurs d'Apple. Le dire, et
                // donner le geste qui marche — sinon on cherche en boucle dans
                // un champ qui ne répondra pas.
                if horsLigne, placeSearch.results.isEmpty {
                    Section("Sans réseau") {
                        Label(
                            "La recherche d'adresse a besoin du réseau. Appuie longuement sur la carte pour choisir un point : l'itinéraire se calcule alors sur l'appareil.",
                            systemImage: "hand.tap"
                        )
                        .font(.footnote)
                        .foregroundStyle(.secondary)
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
        // Fond opaque assumé : le verre d'iOS 26+ laisse la carte saigner à
        // travers la feuille — texte illisible, icônes fantômes (constaté).
        .presentationBackground(Color(.systemBackground))
        // Le paywall de cette feuille : une feuille peut en présenter une
        // autre, celle de la carte ne peut pas passer par-dessus celle-ci.
        .sheet(isPresented: $showPaywallRecherche) { PaywallView() }
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
                // L'abonnement d'abord : où en est-on — essai qui court,
                // Plus, Pro, offert — et tout pour choisir ou gérer.
                Section("Ma formule") {
                    if plus.estOffert {
                        Label("Tout offert 🎁", systemImage: "checkmark.seal.fill")
                            .foregroundStyle(.green)
                    } else if plus.niveau != .aucun {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(
                                    plus.niveau == .pro
                                        ? "MonCap Pro actif" : "MonCap Plus actif")
                                    .font(.subheadline.weight(.semibold))
                                if let fin = plus.renouvellement {
                                    Text(
                                        String(
                                            format: NSLocalizedString(
                                                "Renouvellement le %@.", comment: ""),
                                            dateCourte(fin)))
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        } icon: {
                            Image(systemName: "checkmark.seal.fill")
                                .foregroundStyle(plus.niveau == .pro ? .teal : .green)
                        }
                        Button {
                            gereAbonnement = true
                        } label: {
                            Label("Gérer l'abonnement", systemImage: "gearshape")
                        }
                    } else {
                        if plus.essaiEnCours {
                            Label {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("Essai en cours — tout est ouvert")
                                        .font(.subheadline.weight(.semibold))
                                    if let jours = plus.joursEssaiRestants {
                                        Text(
                                            String(
                                                format: NSLocalizedString(
                                                    "Il reste %d jour(s), puis une formule.",
                                                    comment: ""), jours))
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                            } icon: {
                                Image(systemName: "star.fill").foregroundStyle(.indigo)
                            }
                        }
                        Button {
                            apresMenu = { showPaywall = true }
                            showPlaces = false
                        } label: {
                            Label {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("Choisir ma formule")
                                        .font(.subheadline.weight(.semibold))
                                    Text(
                                        String(
                                            format: NSLocalizedString(
                                                "Plus %@ ou Pro %@ par mois.", comment: ""),
                                            plus.prixPlus, plus.prixPro))
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            } icon: {
                                Image(systemName: "star.fill").foregroundStyle(.indigo)
                            }
                        }
                    }
                }
                Section {
                    Button {
                        apresMenu = { showArrive = true }
                        showPlaces = false
                    } label: {
                        Label("Prévenir que je suis arrivé", systemImage: "checkmark.message.fill")
                    }
                    if liveShare.active {
                        Button {
                            if let u = liveShare.shareURL {
                                apresMenu = { Sharer.present([u]) }
                            }
                            showPlaces = false
                        } label: {
                            Label("Renvoyer le lien de suivi", systemImage: "link")
                        }
                        Button(role: .destructive) {
                            liveShare.stop()
                            // Coupe le suivi en arrière-plan sauf si la navigation
                            // est en cours (elle a le sien).
                            location.setBackgroundTracking(nav.active)
                        } label: {
                            Label("Arrêter le partage en direct", systemImage: "stop.circle.fill")
                        }
                    } else {
                        Button {
                            startLiveShare()
                        } label: {
                            Label(
                                "Partager ma position en direct",
                                systemImage: "dot.radiowaves.left.and.right")
                        }
                        .disabled(location.coordinate == nil)
                    }
                    Button {
                        fires.refresh(key: firmsKey)
                        apresMenu = { showFireMap = true }
                        showPlaces = false
                    } label: {
                        Label("Carte des feux de forêt", systemImage: "flame.fill")
                    }
                }
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
                        apresMenu = { showTrips = true }
                        showPlaces = false
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
                    // L'entretien suit les kilomètres du journal : la
                    // vidange se rappelle toute seule.
                    Button {
                        apresMenu = { showEntretien = true }
                        showPlaces = false
                    } label: {
                        Label("Entretien", systemImage: "wrench.and.screwdriver")
                    }
                    // Le mode moto : même graphe, autre fonction de coût.
                    Toggle(isOn: $modeMoto) {
                        Label("Moto", systemImage: "motorcycle")
                    }
                    .onChange(of: modeMoto) {
                        if modeMoto { modeUtilitaire = false }
                    }
                    if modeMoto {
                        Toggle(isOn: $motoPlaisir) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Routes plaisir")
                                Text("Fuit l'autoroute, cherche les routes qui tournent — pour la balade, pas pour arriver tôt.")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    // Le gabarit utilitaire — l'outil Pro du fourgon : les
                    // ponts bas et les rues interdites au gabarit déclaré
                    // sont évités (costing truck de Valhalla).
                    Toggle(isOn: lienUtilitaire) {
                        Label("Utilitaire", systemImage: "box.truck")
                    }
                    if modeUtilitaire {
                        Stepper(value: $utilHauteur, in: 1.8...4.5, step: 0.05) {
                            HStack {
                                Text("Hauteur")
                                Spacer()
                                Text(verbatim: String(format: "%.2f m", utilHauteur))
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Stepper(value: $utilLargeur, in: 1.8...2.6, step: 0.05) {
                            HStack {
                                Text("Largeur")
                                Spacer()
                                Text(verbatim: String(format: "%.2f m", utilLargeur))
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Stepper(value: $utilPoids, in: 1.5...19, step: 0.5) {
                            HStack {
                                Text("Poids")
                                Spacer()
                                Text(verbatim: String(format: "%.1f t", utilPoids))
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Text("Dis les dimensions vraies, carte grise en main : c'est elles qui écartent les ponts bas et les rues interdites.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
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
                // Le parrainage, sans machinerie : on partage l'app, l'essai
                // de quatorze jours fait le reste. Les codes promo offerts
                // se distribuent depuis App Store Connect, et se saisissent
                // dans le paywall (« J'ai un code »).
                Section {
                    ShareLink(
                        item: URL(string: "https://moncap-c41a5aaf07e8.herokuapp.com/")!,
                        message: Text("Essaie MonCap GPS : quatorze jours tout ouvert pour te faire une idée.")
                    ) {
                        Label("Faire découvrir MonCap", systemImage: "gift")
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
                    // La portabilité RGPD : ses données, en un fichier.
                    Button {
                        exporteDonnees()
                    } label: {
                        Label("Exporter mes données", systemImage: "square.and.arrow.up.on.square")
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
                creditsSection
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
        // Même règle que la feuille de recherche : opaque, pas de verre.
        .presentationBackground(Color(.systemBackground))
        // La feuille système de gestion d'abonnement (résilier, changer) :
        // celle d'Apple, par-dessus le menu.
        .manageSubscriptionsSheet(isPresented: $gereAbonnement)
    }

    /// Une date courte dans la langue de l'app (« 3 sept. 2026 »).
    private func dateCourte(_ d: Date) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: Bundle.main.preferredLocalizations.first ?? "fr")
        f.dateStyle = .medium
        f.timeStyle = .none
        return f.string(from: d)
    }

    // MARK: - Crédits

    /// Section « À propos » : auteur de l'app, version installée et sources des
    /// données affichées (les licences imposent de créditer OpenStreetMap et
    /// les fournisseurs publics).
    private var creditsSection: some View {
        Section("À propos") {
            VStack(alignment: .leading, spacing: 4) {
                Text("MonCap GPS").font(.headline)
                Text("par Maxime Nathan Lestage")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Text(appVersion).font(.caption).foregroundStyle(.secondary)
            }
            .padding(.vertical, 2)
            VStack(alignment: .leading, spacing: 3) {
                Text("Données").font(.caption.weight(.semibold))
                Text(
                    """
                    Cartes : Apple Plans · Feux : NASA FIRMS (VIIRS) · \
                    Trafic : TomTom · Météo : Open-Meteo · \
                    Vitesses : OpenStreetMap · Carburants : data.economie.gouv.fr · \
                    Cartes hors ligne : serveur de tuiles configuré
                    """
                )
                .font(.caption2)
                .foregroundStyle(.secondary)
            }
            .padding(.vertical, 2)
        }
    }

    /// « Version 1.0 (42) » d'après le bundle (le numéro de build vient de la CI).
    private var appVersion: String {
        let info = Bundle.main.infoDictionary
        let v = info?["CFBundleShortVersionString"] as? String ?? "—"
        let b = info?["CFBundleVersion"] as? String ?? "—"
        return "Version \(v) (\(b))"
    }

    // MARK: - Feuille « Signaler »

    private var reportsSheet: some View {
        // Défilable et refermable : la grille dépassait la hauteur du demi-écran
        // et se retrouvait rognée, sans moyen de revenir à la carte.
        ScrollView {
            VStack(spacing: 16) {
                HStack {
                    Text("Signaler").font(.headline)
                    Spacer()
                    Button("Fermer") { showReports = false }
                        .font(.subheadline.weight(.semibold))
                }
                .padding(.horizontal)
                .padding(.top, 8)
                LazyVGrid(
                    columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 16
                ) {
                    ForEach(alertTypes, id: \.category) { t in
                        Button {
                            report(category: t.category, label: t.label)
                            showReports = false
                        } label: {
                            VStack(spacing: 8) {
                                Text(t.emoji).font(.largeTitle)
                                Text(LocalizedStringKey(t.label))
                                    .font(.subheadline.weight(.semibold))
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 18)
                            .background(
                                t.color.opacity(0.15), in: RoundedRectangle(cornerRadius: 18))
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal)
            }
            .padding(.bottom, 16)
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .presentationBackground(Color(.systemBackground))
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
                // Les outils pro du journal : l'export comptable et la
                // facture. Derrière Plus — c'est précisément pour eux qu'il
                // existe.
                ToolbarItem(placement: .topBarLeading) {
                    Menu {
                        Button {
                            if plus.accesPro { exporteJournal() } else { showPaywallTrips = true }
                        } label: {
                            Label("Exporter en CSV", systemImage: "doc.text")
                        }
                        .disabled(trips.isEmpty && plus.accesPro)
                        Button {
                            if plus.accesPro { showFacturation = true } else { showPaywallTrips = true }
                        } label: {
                            Label("Facturation", systemImage: "eurosign.circle")
                        }
                        Button {
                            if plus.accesPro { showAttestation = true } else { showPaywallTrips = true }
                        } label: {
                            Label("Attestation fiscale", systemImage: "percent")
                        }
                    } label: {
                        Image(systemName: "briefcase")
                    }
                }
                // « Mon année » : le journal en gros chiffres, partageable.
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        showRecapAnnuel = true
                    } label: {
                        Image(systemName: "sparkles")
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { showTrips = false }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationBackground(Color(.systemBackground))
        .sheet(isPresented: $showRecapAnnuel) { RecapAnnuelView(trajets: trips) }
        .sheet(isPresented: $showPaywallTrips) { PaywallView() }
        .sheet(isPresented: $showFacturation) { FacturationView(trajets: trips) }
        .sheet(isPresented: $showAttestation) { AttestationFiscaleView(trajets: trips) }
    }

    /// Le journal kilométrique en CSV : une ligne par trajet, du plus ancien
    /// au plus récent, prêt pour un tableur ou la comptabilité. Séparateur
    /// point-virgule : c'est celui qu'Excel attend dans les régions où la
    /// virgule est décimale.
    private func exporteJournal() {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd HH:mm"
        var csv = NSLocalizedString("Date;Trajet;Distance (km);Durée (min)", comment: "") + "\n"
        for t in trips.sorted(by: { $0.created_at < $1.created_at }) {
            let libelle = t.label.replacingOccurrences(of: ";", with: ",")
            csv += "\(f.string(from: t.date));\(libelle);"
            csv += String(format: "%.1f;%.0f\n", t.distance_km, t.duration_min)
        }
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("journal-moncap.csv")
        guard (try? csv.data(using: .utf8)?.write(to: url)) != nil else { return }
        Sharer.present([url])
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
            // Le compte dit la date de création (l'ancre de l'essai de
            // quatorze jours) et si tout est offert (liste d'invités) :
            // l'état ne bouge que quand le serveur a vraiment répondu.
            if let info = accountInfo {
                plus.majCompte(creeLe: info.created_at, offert: info.plus_offert ?? false)
            }
            // Les favoris suivent le compte : ce qui manque ici s'adopte,
            // ce que l'appareil a en plus se pousse — l'appareil reste
            // maître en cas de désaccord, c'est lui qu'on a sous les yeux.
            if let distant = try? await api.prefs() {
                if homePlace == nil, let h = distant.home {
                    Session.home = h
                    homePlace = h
                }
                if workPlace == nil, let w = distant.work {
                    Session.work = w
                    workPlace = w
                }
                if (homePlace != nil && distant.home == nil)
                    || (workPlace != nil && distant.work == nil)
                {
                    pousseFavoris()
                }
            }
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
            camera = MapCam(region: MKCoordinateRegion(center: center, span: span))
        }
    }

    /// Démarre la navigation depuis ma position vers une destination.
    private func startNavigation(to dest: Position) async {
        guard let from = location.coordinate else { return }
        multiRoutes = []
        let to = CLLocationCoordinate2D(latitude: dest.lat, longitude: dest.lon)
        if horsLigne, let local = router.route(from: from, to: to,
                                               costing: travelMode.costing) {
            nav.start(steps: local.steps, coordinates: local.coordinates,
                      distanceKm: local.km, etaMinutes: local.minutes, destination: to)
            speedLimit.preload(along: local.coordinates)
            showPlaces = false
            withAnimation { camera = followCamera(from) }
            return
        }
        guard let route = await drivingRoute(from: from, to: to) else {
            // Le réseau n'a rien rendu : un paquet installé peut encore
            // répondre, interrupteur ou non.
            if let local = router.route(from: from, to: to,
                                        costing: travelMode.costing) {
                nav.start(steps: local.steps, coordinates: local.coordinates,
                          distanceKm: local.km, etaMinutes: local.minutes, destination: to)
                speedLimit.preload(along: local.coordinates)
                showPlaces = false
                withAnimation { camera = followCamera(from) }
            } else {
                itineraireImpossible = raisonSansItineraire()
            }
            return
        }
        nav.start(route: route, destination: to)
        showPlaces = false
        // Vue « conduite » : la carte suit et tourne dans le sens de la marche.
        withAnimation { camera = followCamera(from) }
    }

    /// Pourquoi aucun itinéraire n'a pu être proposé.
    ///
    /// Le message ne **devine** plus. Il disait « aucune région ne couvre ce
    /// trajet » dans tous les cas — y compris quand la région couvrait
    /// parfaitement le trajet mais que son paquet refusait de s'ouvrir, ce qui
    /// envoyait chercher un défaut de couverture là où il fallait réinstaller.
    /// Le routeur rapporte désormais ce que le moteur a réellement dit, et
    /// c'est cela qui s'affiche.
    private func raisonSansItineraire() -> String {
        // Plus de branche « ce mode a besoin du réseau » : les trois modes se
        // calculent désormais sur l'appareil, avec les mêmes tuiles. Les
        // causes d'échec sont donc les mêmes pour tous.
        if !router.ready {
            return NSLocalizedString(
                "Aucune région d'itinéraires n'est installée. Télécharge celle qui te concerne depuis l'écran hors ligne.",
                comment: "")
        }

        switch router.dernierEchec {
        case .aucuneRegion, .none:
            return NSLocalizedString(
                "Aucune région installée ne couvre ce trajet. Il doit tenir entier dans un même paquet : c'est à cela que servent les zones, qui se chevauchent.",
                comment: "")

        case .paquetIllisible(let region):
            return String(
                format: NSLocalizedString(
                    "Le paquet « %@ » ne s'ouvre pas. Supprime-le et réinstalle-le depuis l'écran hors ligne.",
                    comment: ""), region)

        case .refus(let region, let raison):
            if OfflineRoute.estHorsCouverture(raison) {
                return String(
                    format: NSLocalizedString(
                        "« %@ » ne couvre pas ce trajet. Il doit tenir entier dans un même paquet : c'est à cela que servent les zones, qui se chevauchent.",
                        comment: ""), region)
            }
            // Une erreur qu'on n'a pas prévue se montre telle quelle : mieux
            // vaut une phrase technique exacte qu'une explication inventée.
            return String(
                format: NSLocalizedString(
                    "« %@ » a refusé le calcul : %@", comment: ""), region, raison)

        case .sansItineraire(let region):
            return String(
                format: NSLocalizedString(
                    "« %@ » n'a trouvé aucune route entre ces deux points, sans dire pourquoi.",
                    comment: ""), region)
        }
    }

    /// Calcule plusieurs itinéraires vers une destination et les propose :
    /// le plus simple (moins de virages) est mis en avant en vert.
    private func presentRouteOptions(to dest: CLLocationCoordinate2D) async {
        guard let from = location.coordinate else { return }
        multiRoutes = []
        routeCoords = []
        routeInfo = nil
        // Calculer un itinéraire, c'est quitter le suivi de trace : la règle
        // du recalcul redevient la normale.
        traceGPX = nil

        // Les transports ont leur propre chemin : le calcul vient du serveur,
        // pas de MapKit ni de Valhalla, et leurs cartes montrent des lignes et
        // des horaires plutôt que des kilomètres.
        if travelMode == .transit {
            await presentTransitOptions(from: from, to: dest)
            return
        }

        // Avec des étapes, un seul trajet : celui qu'on a voulu, dans l'ordre
        // voulu. Un éventail de quinze variantes sur trois arrêts n'aiderait
        // personne à choisir.
        if !etapes.isEmpty {
            await presentRouteAvecEtapes(from: from, to: dest)
            return
        }

        var builds: [RouteBuild]
        // Hors ligne assumé : rien ne part sur le réseau. Valhalla sait
        // proposer des variantes, on lui en demande cinq — le calcul est
        // local, il coûte du temps de processeur et pas de la donnée.
        //
        // La liste vide laisse volontairement passer au réseau : aucune région
        // installée ne couvre alors le trajet, et rendre la main sans rien
        // proposer serait pire que de déroger au mode choisi.
        //
        // Tous les modes, pas la seule voiture : brider le calcul local à
        // `auto` envoyait le vélo et la marche sur le réseau en plein mode
        // avion, pour un calcul que le paquet installé savait faire.
        let locaux = horsLigne
            ? router.routes(from: from, to: dest, max: OfflineRouter.maxItineraires,
                            costing: travelMode.costing)
            : []
        if !locaux.isEmpty {
            builds = locaux.map {
                RouteBuild(coords: $0.coordinates, steps: $0.steps,
                           km: $0.km, minutes: $0.minutes, hasTolls: $0.hasTolls)
            }
        } else if travelMode == .bike {
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
            // Diversifie via des points de passage décalés de part et d'autre
            // du trajet direct, pour viser une quinzaine de choix (calcul
            // parallèle borné, anti-throttling). Tous les modes y ont droit —
            // les alternatives directes d'Apple s'arrêtent à deux ou trois, et
            // c'est à pied ou à vélo qu'un détour par les quais change tout.
            // La marche en demande moins : ses tracés convergent vite, et six
            // détours n'y donnent pas plus de choix que trois.
            do {
                let fractions: [Double] =
                    travelMode == .walk ? [0.22, 0.46, 0.7] : [0.1, 0.22, 0.34, 0.46, 0.58, 0.7]
                var vias: [CLLocationCoordinate2D] = []
                for f in fractions {
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
        // Renfort local : dès que le réseau rend moins que le plafond de
        // quinze, Valhalla complète — que l'interrupteur « Naviguer hors
        // ligne » soit mis ou non : celui-ci veut dire « privilégie le calcul
        // local », pas « c'est le seul moyen d'y avoir droit ». En ligne comme
        // hors ligne, l'écran promet le même éventail. À pied et à vélo,
        // Apple s'arrête souvent à deux tracés ; Valhalla en ajoute jusqu'à
        // quinze — et pour le vélo, avec un vrai profil, ce que MapKit n'a
        // pas. Sans région installée couvrant le trajet, l'appel rend vite
        // une liste vide et ne coûte rien. La déduplication d'en dessous
        // retire ce qui ferait doublon.
        if builds.count < OfflineRouter.maxItineraires {
            // Seulement le manque : quand le réseau a déjà fourni treize
            // tracés, deux détours locaux suffisent — pas douze.
            builds += router.routes(from: from, to: dest,
                                    max: OfflineRouter.maxItineraires - builds.count,
                                    costing: travelMode.costing)
                .map {
                    RouteBuild(coords: $0.coordinates, steps: $0.steps,
                               km: $0.km, minutes: $0.minutes, hasTolls: $0.hasTolls)
                }
        }
        guard !builds.isEmpty else {
            itineraireImpossible = raisonSansItineraire()
            return
        }

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
        // Le plus simple d'abord, puis par durée ; même plafond hors ligne
        // qu'en ligne, la constante étant partagée.
        options.sort { ($0.isSimplest ? 0 : 1, $0.minutes) < ($1.isSimplest ? 0 : 1, $1.minutes) }
        options = Array(options.prefix(OfflineRouter.maxItineraires))

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
        // Et avec ce qu'il va rencontrer : feux sur le tracé, air et pluie à
        // l'arrivée (en tâche de fond aussi).
        Task { await chargeRisques(options: options, dest: dest) }
    }

    /// Le trajet à étapes : départ → étapes dans l'ordre → destination.
    ///
    /// Valhalla enchaîne nativement (des arrêts « break », les tronçons
    /// recollés par le décodeur existant) ; MapKit est enchaîné tronçon par
    /// tronçon. Chaque étape s'annonce par son nom dans le guidage, et se
    /// marque atteinte à moins de cent cinquante mètres — le recalcul d'écart
    /// ne repasse pas par elle.
    private func presentRouteAvecEtapes(
        from: CLLocationCoordinate2D, to dest: CLLocationCoordinate2D
    ) async {
        etapesAtteintes = []
        var build: RouteBuild?

        if horsLigne,
            let locale = router.route(
                from: from, to: dest, costing: travelMode.costing, vias: etapes)
        {
            build = RouteBuild(
                coords: locale.coordinates, steps: locale.steps,
                km: locale.km, minutes: locale.minutes, hasTolls: locale.hasTolls)
        }
        if build == nil {
            build = await chaineEtapes(
                points: [from] + etapes + [dest], noms: etapeNoms,
                transportType: travelMode.transportType)
        }
        // Le réseau n'a rien rendu : le paquet installé peut encore répondre,
        // interrupteur ou non — même règle que sans étapes.
        if build == nil,
            let locale = router.route(
                from: from, to: dest, costing: travelMode.costing, vias: etapes)
        {
            build = RouteBuild(
                coords: locale.coordinates, steps: locale.steps,
                km: locale.km, minutes: locale.minutes, hasTolls: locale.hasTolls)
        }
        guard let build, build.coords.count >= 2 else {
            itineraireImpossible = raisonSansItineraire()
            return
        }

        // La durée promise inclut les arrêts prévus : partir « pour 1 h 40 »
        // quand on a dit rester un quart d'heure chez Paul, c'est 1 h 55.
        var option = RouteOption(
            coordinates: build.coords, steps: build.steps,
            minutes: build.minutes + Double(etapeArrets.reduce(0, +)),
            km: build.km,
            turns: build.steps.count, hasTolls: build.hasTolls)
        option.isSimplest = true
        option.color = .green

        pendingDestination = dest
        routeOptions = [option]
        selectedRouteID = option.id
        routesExpanded = false
        fitCoordinates(option.coordinates + [from])
        let options = [option]
        Task { await loadElevations(for: options) }
        Task { await chargeRisques(options: options, dest: dest) }
    }

    /// Itinéraires en transports en commun, calculés par le serveur.
    ///
    /// Chaque itinéraire devient une `RouteOption` ordinaire — tracé, étapes,
    /// durée — enrichie de ses tronçons : tout l'aval (carte, fiche, Démarrer)
    /// fonctionne sans une ligne de plus. Les « virages » y comptent les
    /// correspondances, ce que l'affichage sait dire.
    private func presentTransitOptions(
        from: CLLocationCoordinate2D, to dest: CLLocationCoordinate2D
    ) async {
        let depart = transitQuand == .maintenant ? nil : transitDate
        guard
            let its = await TransitService.fetch(
                from: from, to: dest,
                depart: depart, arriveeAvant: transitQuand == .arriverA)
        else {
            itineraireImpossible = NSLocalizedString(
                "Les transports en commun ont besoin du réseau.", comment: "")
            return
        }
        guard !its.isEmpty else {
            itineraireImpossible = NSLocalizedString(
                "Aucun trajet en transports trouvé pour cette destination.", comment: "")
            return
        }

        var options: [RouteOption] = its.compactMap { it in
            var coords: [CLLocationCoordinate2D] = []
            var etapes: [NavStep] = []
            for l in it.legs {
                let trace = TransitService.decode(l.trace)
                let debut = trace.first ?? coords.last ?? from
                let texte: String
                if l.estVehicule {
                    // La girouette d'abord : c'est elle qu'on cherche des yeux
                    // sur le quai. À défaut, le terminus du tronçon.
                    let cap = l.direction ?? l.a
                    if let ligne = l.ligne {
                        texte = String(
                            format: NSLocalizedString(
                                "%@ %@ direction %@ — descendre à %@", comment: ""),
                            l.nomDuMode, ligne, cap, l.a)
                    } else {
                        texte = String(
                            format: NSLocalizedString(
                                "%@ direction %@ — descendre à %@", comment: ""),
                            l.nomDuMode, cap, l.a)
                    }
                } else {
                    // Le serveur borne le trajet par START/END, des jalons et
                    // non des noms : on ne les affiche pas tels quels.
                    let but = (l.a == "END" || l.a.isEmpty)
                        ? NSLocalizedString("la destination", comment: "") : l.a
                    texte = String(
                        format: NSLocalizedString("Marcher jusqu'à %@", comment: ""), but)
                }
                etapes.append(NavStep(text: texte, coord: debut))
                coords += trace
            }
            guard coords.count >= 2 else { return nil }

            var km = 0.0
            for i in 1..<coords.count {
                km += CLLocation(latitude: coords[i - 1].latitude, longitude: coords[i - 1].longitude)
                    .distance(from: CLLocation(latitude: coords[i].latitude, longitude: coords[i].longitude))
                    / 1000
            }
            var o = RouteOption(
                coordinates: coords, steps: etapes,
                minutes: Double(it.duree_s) / 60, km: km, turns: it.correspondances)
            o.transit = it.legs
            if let d = TransitService.heure(it.depart), let a = TransitService.heure(it.arrivee) {
                o.horaire = "\(d) → \(a)"
            }
            return o
        }
        guard !options.isEmpty else {
            itineraireImpossible = NSLocalizedString(
                "Aucun trajet en transports trouvé pour cette destination.", comment: "")
            return
        }

        // « Le plus simple » = le moins de correspondances, puis le plus
        // rapide — en tram, la simplicité se compte en changements.
        if let best = options.indices.min(by: {
            (options[$0].turns, options[$0].minutes) < (options[$1].turns, options[$1].minutes)
        }) {
            options[best].isSimplest = true
        }
        options.sort { ($0.isSimplest ? 0 : 1, $0.minutes) < ($1.isSimplest ? 0 : 1, $1.minutes) }

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
        // Pas de dénivelé : dans un tram, la côte est l'affaire du tram. Les
        // risques, si : un feu ne regarde pas le mode de transport.
        Task { await chargeRisques(options: options, dest: dest) }
    }

    /// Pose une étape là où le doigt l'a désignée, et recalcule.
    private func ajouterEtape(_ coord: CLLocationCoordinate2D) {
        ajouteEtape = false
        etapes.append(coord)
        etapeArrets.append(0)
        etapeNoms.append(
            String(format: NSLocalizedString("Étape %d", comment: ""), etapes.count))
        if let dest = pendingDestination {
            Task { await presentRouteOptions(to: dest) }
        }
        // Nomme l'étape quand le réseau le permet ; l'ordinal reste sinon.
        let index = etapes.count - 1
        Task {
            if let lieu = try? await CLGeocoder().reverseGeocodeLocation(
                CLLocation(latitude: coord.latitude, longitude: coord.longitude)
            ).first, let nom = lieu.name, index < etapeNoms.count {
                etapeNoms[index] = nom
            }
        }
    }

    private func retireEtape(_ index: Int) {
        guard etapes.indices.contains(index) else { return }
        etapes.remove(at: index)
        etapeNoms.remove(at: index)
        if etapeArrets.indices.contains(index) { etapeArrets.remove(at: index) }
        etapesAtteintes = []
        recalculeApresEtapes()
    }

    /// Le temps d'arrêt d'une étape, relié au menu de sa pastille. Le tracé ne
    /// bouge pas quand on le change : seule la durée affichée est corrigée,
    /// sans relancer le calcul.
    private func lienArret(_ index: Int) -> Binding<Int> {
        Binding(
            get: { etapeArrets.indices.contains(index) ? etapeArrets[index] : 0 },
            set: { nouveau in
                guard etapeArrets.indices.contains(index) else { return }
                let delta = Double(nouveau - etapeArrets[index])
                etapeArrets[index] = nouveau
                if routeOptions.count == 1, delta != 0 {
                    routeOptions[0].minutes += delta
                }
            })
    }

    /// La clé d'un créneau : la coordonnée de l'étape, quantifiée — elle
    /// survit au réordonnancement et au retrait, là où un index dériverait.
    private func cleEtape(_ c: CLLocationCoordinate2D) -> String {
        "\(Int(c.latitude * 1e5))|\(Int(c.longitude * 1e5))"
    }

    private func lienCreneau(_ index: Int) -> Binding<Int> {
        Binding(
            get: {
                guard etapes.indices.contains(index) else { return 0 }
                return etapeCreneaux[cleEtape(etapes[index])] ?? 0
            },
            set: { nouveau in
                guard etapes.indices.contains(index) else { return }
                let cle = cleEtape(etapes[index])
                if nouveau == 0 {
                    etapeCreneaux[cle] = nil
                } else {
                    etapeCreneaux[cle] = nouveau
                }
            })
    }

    /// « · avant 10:00 » quand l'étape porte un créneau, rien sinon.
    private func creneauSuffixe(_ index: Int) -> String {
        guard etapes.indices.contains(index),
            let limite = etapeCreneaux[cleEtape(etapes[index])], limite > 0
        else { return "" }
        return String(format: " · ≤ %02d:%02d", limite / 60, limite % 60)
    }

    /// Minutes de route estimées jusqu'à l'étape : la proportion du tracé
    /// parcourue à son point le plus proche, plus les arrêts d'avant — une
    /// approximation franche, largement assez pour comparer à un créneau.
    private func minutesJusquAEtape(_ index: Int) -> Double? {
        guard etapes.indices.contains(index),
            let route = routeOptions.first(where: { $0.id == selectedRouteID })
                ?? routeOptions.first,
            route.coordinates.count >= 2
        else { return nil }
        let coords = route.coordinates
        let pas = max(1, coords.count / 200)
        var indices: [Int] = [0]
        var cumul: [Double] = [0]
        var total = 0.0
        var precedent = coords[0]
        for j in stride(from: pas, to: coords.count, by: pas) {
            total += metersBetween(precedent, coords[j])
            precedent = coords[j]
            indices.append(j)
            cumul.append(total)
        }
        guard total > 0 else { return nil }
        let cible = etapes[index]
        var meilleur = 0
        var dMin = Double.greatestFiniteMagnitude
        for (k, j) in indices.enumerated() {
            let d = metersBetween(coords[j], cible)
            if d < dMin {
                dMin = d
                meilleur = k
            }
        }
        let arretsAvant = (0..<index).reduce(0) {
            $0 + (etapeArrets.indices.contains($1) ? etapeArrets[$1] : 0)
        }
        return route.minutes * (cumul[meilleur] / total) + Double(arretsAvant)
    }

    /// Vrai quand l'heure estimée de passage dépasse le créneau du jour.
    private func enRetardSurCreneau(_ index: Int) -> Bool {
        guard etapes.indices.contains(index),
            let limite = etapeCreneaux[cleEtape(etapes[index])], limite > 0,
            let minutes = minutesJusquAEtape(index)
        else { return false }
        let passage = Date().addingTimeInterval(minutes * 60)
        let c = Calendar.current.dateComponents([.hour, .minute], from: passage)
        return (c.hour ?? 0) * 60 + (c.minute ?? 0) > limite
    }

    /// Réordonne les étapes pour minimiser le chemin : force brute jusqu'à
    /// sept étapes (au pire 5 040 ordres, des additions), plus proche voisin
    /// au-delà. La distance est à vol d'oiseau — c'est un tri, pas un calcul
    /// d'itinéraire — donc tout fonctionne hors ligne.
    private func ordonneAuPlusCourt() {
        guard etapes.count >= 2, let from = location.coordinate,
            let dest = pendingDestination
        else { return }
        let ordre = meilleurOrdre(depuis: from, vers: dest)
        guard ordre != Array(etapes.indices) else { return }
        etapes = ordre.map { etapes[$0] }
        etapeNoms = ordre.map { etapeNoms.indices.contains($0) ? etapeNoms[$0] : "" }
        etapeArrets = ordre.map { etapeArrets.indices.contains($0) ? etapeArrets[$0] : 0 }
        etapesAtteintes = []
        Task { await presentRouteOptions(to: dest) }
    }

    private func meilleurOrdre(
        depuis from: CLLocationCoordinate2D, vers dest: CLLocationCoordinate2D
    ) -> [Int] {
        let n = etapes.count
        func cout(_ ordre: [Int]) -> Double {
            var total = 0.0
            var courant = from
            for i in ordre {
                total += metersBetween(courant, etapes[i])
                courant = etapes[i]
            }
            return total + metersBetween(courant, dest)
        }
        if n <= 7 {
            var meilleur = Array(0..<n)
            var record = cout(meilleur)
            func explore(_ pris: [Int], _ reste: [Int]) {
                if reste.isEmpty {
                    let c = cout(pris)
                    if c < record {
                        record = c
                        meilleur = pris
                    }
                    return
                }
                for (k, i) in reste.enumerated() {
                    var suivant = reste
                    suivant.remove(at: k)
                    explore(pris + [i], suivant)
                }
            }
            explore([], Array(0..<n))
            return meilleur
        }
        // Au-delà de sept : plus proche voisin pour l'ébauche, puis 2-opt —
        // on retourne chaque tronçon tant que ça raccourcit, ce qui décroise
        // les allers-retours que le glouton laisse derrière lui. Sur des
        // distances à vol d'oiseau, c'est quasi optimal, et instantané
        // jusqu'à plusieurs dizaines d'arrêts.
        var reste = Set(0..<n)
        var ordre: [Int] = []
        var courant = from
        while let prochain = reste.min(by: {
            metersBetween(courant, etapes[$0]) < metersBetween(courant, etapes[$1])
        }) {
            ordre.append(prochain)
            reste.remove(prochain)
            courant = etapes[prochain]
        }
        var record = cout(ordre)
        var ameliore = true
        var gardeFou = 0
        while ameliore, gardeFou < 60 {
            ameliore = false
            gardeFou += 1
            for i in 0..<(n - 1) {
                for j in (i + 1)..<n {
                    var essai = ordre
                    essai[i...j].reverse()
                    let c = cout(essai)
                    // Un demi-mètre de marge : sans elle, deux ordres au coût
                    // égal à l'arrondi près s'échangeraient sans fin.
                    if c < record - 0.5 {
                        ordre = essai
                        record = c
                        ameliore = true
                    }
                }
            }
        }
        return ordre
    }

    /// Relance le calcul après un changement d'étapes, si une destination
    /// est posée.
    private func recalculeApresEtapes() {
        if let dest = pendingDestination {
            Task { await presentRouteOptions(to: dest) }
        }
    }

    /// Les étapes encore devant nous pendant le guidage.
    private func etapesRestantes() -> [CLLocationCoordinate2D] {
        etapes.enumerated()
            .filter { !etapesAtteintes.contains($0.offset) }
            .map(\.element)
    }

    /// Croise les itinéraires avec ce que l'app sait déjà : les feux sur le
    /// tracé, l'air et la pluie à l'arrivée. Trois requêtes légères, et le
    /// silence en deçà des seuils — une alerte qui crie pour rien apprend à
    /// être ignorée.
    private func chargeRisques(
        options: [RouteOption], dest: CLLocationCoordinate2D
    ) async {
        risquesParOption = [:]
        risquesArrivee = []

        // Une seule requête de feux pour toutes les variantes : elles longent
        // le même pays.
        let echantillons = options.flatMap { sampleCoords($0.coordinates, max: 40) }
        let feux = await RouteRisques.feux(couvrant: echantillons)
        if !feux.isEmpty {
            for o in options {
                if let km = RouteRisques.kmDuPremierFeu(trace: o.coordinates, feux: feux) {
                    risquesParOption[o.id, default: []].append(
                        String(
                            format: NSLocalizedString(
                                "🔥 Traverse une zone de feux vers le km %d", comment: ""),
                            km))
                }
            }
        }

        // La pluie à mi-chemin, sur les trois variantes de tête : chaque point
        // sondé coûte une requête, et au-delà des étiquettes « simple, rapide,
        // court », les tracés se recouvrent trop pour que la météo diffère.
        let formatHeure = DateFormatter()
        formatHeure.dateFormat = "HH:mm"
        for o in options.prefix(3) {
            for pluie in await RouteRisques.pluieEnRoute(
                trace: o.coordinates, dureeMinutes: o.minutes)
            {
                let heure = formatHeure.string(from: pluie.heure)
                let texte: String
                if let lieu = pluie.lieu {
                    texte = String(
                        format: NSLocalizedString("🌧️ Pluie vers %@ (%@ · %d %%)", comment: ""),
                        lieu, heure, pluie.proba)
                } else {
                    texte = String(
                        format: NSLocalizedString("🌧️ Pluie en route (%@ · %d %%)", comment: ""),
                        heure, pluie.proba)
                }
                risquesParOption[o.id, default: []].append(texte)
            }
        }

        if let niveau = await RouteRisques.niveauAirArrivee(dest), niveau >= 3 {
            let paliers = [
                "Bon", "Correct", "Dégradé", "Mauvais", "Très mauvais",
                "Extrêmement mauvais",
            ]
            risquesArrivee.append(
                String(
                    format: NSLocalizedString("🫁 Air %@ à l'arrivée", comment: ""),
                    NSLocalizedString(paliers[min(niveau, 5)], comment: "")))
        }

        let minutes = options.first?.minutes ?? 0
        if let proba = await RouteRisques.pluieArrivee(dest, dans: minutes), proba >= 50 {
            risquesArrivee.append(
                String(
                    format: NSLocalizedString(
                        "🌧️ Pluie probable à l'arrivée (%d %%)", comment: ""),
                    proba))
        }

        // Les zones à faibles émissions, par variante — un contournement
        // peut précisément les éviter. Voiture seulement : un vélo entre
        // partout. On nomme la zone, sans prétendre connaître sa règle du
        // moment : elle change trop souvent pour être embarquée.
        if travelMode == .car {
            for o in options.prefix(3) {
                for zone in await RouteRisques.zonesFaiblesEmissions(
                    trace: o.coordinates)
                {
                    risquesParOption[o.id, default: []].append(
                        String(
                            format: NSLocalizedString(
                                "🚦 ZFE %@ — vérifie ta vignette Crit'Air",
                                comment: ""),
                            zone))
                }
            }
        }
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
        if let c = location.coordinate {
            withAnimation { camera = followCamera(c) }
        }
    }

    /// Recalcule l'itinéraire depuis la position actuelle (sortie de route),
    /// dans le mode de déplacement courant.
    private func recomputeRoute() async {
        guard let from = location.coordinate, let to = nav.destination else {
            nav.rerouteFailed()
            return
        }
        // Une trace importée est la route : s'en écarter n'appelle pas un
        // nouvel itinéraire. Le guidage réaffirme la trace telle quelle et
        // continue de viser son prochain virage.
        if let t = traceGPX {
            nav.applyReroute(
                steps: t.steps, coords: t.coordinates, km: t.km, minutes: t.minutes)
            return
        }
        // Les étapes pas encore atteintes restent au programme : rater une
        // sortie n'annule pas le passage chez Paul — ni le temps qu'on
        // comptait y rester, encore devant nous lui aussi.
        let restantes = etapesRestantes()
        let arrets = Double(
            etapes.indices
                .filter { !etapesAtteintes.contains($0) }
                .reduce(0) { $0 + (etapeArrets.indices.contains($1) ? etapeArrets[$1] : 0) })
        // Hors ligne assumé : on ne tente même pas le réseau. Attendre son
        // échec ferait patienter le bandeau « Recalcul… » plusieurs secondes,
        // au pire moment — celui où l'on vient de rater une sortie.
        if horsLigne, let local = router.route(from: from, to: to,
                                               costing: travelMode.costing,
                                               vias: restantes) {
            nav.applyReroute(
                steps: local.steps, coords: local.coordinates,
                km: local.km, minutes: local.minutes + arrets)
            speedLimit.preload(along: local.coordinates)
            return
        }
        if !restantes.isEmpty {
            // Le chaînage réseau, puis le repli local, comme au calcul.
            if let build = await chaineEtapes(
                points: [from] + restantes + [to], noms: etapeNoms,
                transportType: travelMode.transportType)
            {
                nav.applyReroute(
                    steps: build.steps, coords: build.coords,
                    km: build.km, minutes: build.minutes + arrets)
                speedLimit.preload(along: build.coords)
            } else if let local = router.route(
                from: from, to: to, costing: travelMode.costing, vias: restantes)
            {
                nav.applyReroute(
                    steps: local.steps, coords: local.coordinates,
                    km: local.km, minutes: local.minutes + arrets)
                speedLimit.preload(along: local.coordinates)
            } else {
                nav.rerouteFailed()
            }
            return
        }
        guard let route = await drivingRoute(from: from, to: to,
                                             transportType: travelMode.transportType)
        else {
            // Apple exige une connexion. Si les tuiles de la région sont
            // installées, le calcul se fait sur l'appareil — c'est tout
            // l'intérêt : rater une sortie hors couverture ne condamne plus.
            if let local = OfflineRouter.shared.route(from: from, to: to,
                                                      costing: travelMode.costing)
            {
                nav.applyReroute(
                    steps: local.steps, coords: local.coordinates,
                    km: local.km, minutes: local.minutes)
                speedLimit.preload(along: local.coordinates)
                return
            }
            // Sans tuiles : on garde l'itinéraire en cours plutôt que de
            // laisser le bandeau figé sur « Recalcul… ».
            nav.rerouteFailed()
            return
        }
        nav.applyReroute(route: route)
        // Les limites de vitesse suivent le nouveau tracé.
        speedLimit.preload(along: route.polyline.coordinates)
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

    /// Tous les itinéraires (dont alternatives) entre deux points, pour choisir
    /// le plus rapide selon le trafic ou contourner un bouchon.
    private func drivingRoutes(
        from: CLLocationCoordinate2D, to: CLLocationCoordinate2D,
        transportType: MKDirectionsTransportType = .automobile
    ) async -> [MKRoute] {
        let request = MKDirections.Request()
        request.source = MKMapItem(placemark: MKPlacemark(coordinate: from))
        request.destination = MKMapItem(placemark: MKPlacemark(coordinate: to))
        request.transportType = transportType
        request.requestsAlternateRoutes = true
        routePreferences(request)
        return (try? await MKDirections(request: request).calculate().routes) ?? []
    }

    private func recenter() {
        followsRoute = true
        guard let c = location.coordinate else { return }
        if nav.active {
            withAnimation { camera = followCamera(c) }
            return
        }
        withAnimation { camera = userCamera(c) }
    }

    /// Vitesse en dessous de laquelle on considère être à l'arrêt.
    private static let stoppedKmh = 5.0

    /// Caméra de suivi adaptée à l'allure — une seule règle pour la 2D et la 3D.
    ///
    /// En roulant, la caméra descend et se rapproche : on voit la rue et la
    /// prochaine manœuvre, et plus on va vite plus elle recule pour regarder
    /// loin devant. À l'arrêt, elle prend de la hauteur et dézoome pour situer
    /// les environs.
    private func followCamera(_ c: CLLocationCoordinate2D) -> MapCam {
        let speed = max(0, location.speedKmh)
        if speed < Self.stoppedKmh {
            // À l'arrêt on prend du recul pour situer les environs, et le cap
            // GPS n'étant plus fiable, on garde le dernier connu.
            return MapCam(
                center: c, zoom: 14.5,
                heading: max(0, lastCamHeading), pitch: is3D ? 35 : 0)
        }
        // En roulant on se rapproche de la rue, et plus on va vite plus on
        // recule pour voir loin devant.
        let zoom = max(14, 16.5 - speed / 40)
        return MapCam(
            center: c, zoom: zoom,
            heading: location.course, pitch: is3D ? 60 : 0)
    }

    /// Caméra centrée sur un point, inclinée si le mode 3D est actif.
    private func userCamera(_ c: CLLocationCoordinate2D, zoom: Double = 15) -> MapCam {
        MapCam(center: c, zoom: zoom, heading: 0, pitch: is3D ? 60 : 0)
    }

    /// Active/désactive la vue 3D et l'applique immédiatement.
    private func toggle3D() {
        is3D.toggle()
        followsRoute = true
        guard let c = location.coordinate else { return }
        if nav.active {
            withAnimation { camera = followCamera(c) }
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
                    camera = MapCam(
                        center: c, zoom: 15.5,
                        heading: heading, pitch: is3D ? 60 : 45)
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
            camera = MapCam(region: MKCoordinateRegion(center: center, span: span))
        }
    }

    private func exportGPX() async {
        guard let url = try? await api.exportGPX() else {
            shareError = "Impossible de générer le fichier GPX pour le moment."
            return
        }
        // Le partage part dans onDismiss du menu, quand la feuille est
        // vraiment fermée — même mécanisme que le reste du menu.
        apresMenu = { Sharer.present([url]) }
        showPlaces = false
    }

    /// Le fichier choisi dans le sélecteur devient une trace suivable —
    /// ou une explication, si le fichier n'en est pas une.
    private func chargeGPX(_ resultat: Result<URL, Error>) {
        guard case .success(let url) = resultat, let trace = GPXImport.lit(url) else {
            shareError = NSLocalizedString(
                "Ce fichier GPX n'a pas pu être lu.", comment: "")
            return
        }
        showSearch = false
        presenteTrace(trace)
    }

    /// Affiche une trace importée comme unique itinéraire. Les consignes
    /// sortent de la géométrie (les mêmes que pour BRouter), la durée d'une
    /// vitesse moyenne selon le mode — une trace n'apporte ni l'une ni
    /// l'autre. Le tout est promu « trace en cours » : le recalcul d'écart la
    /// réaffirme au lieu de la remplacer.
    private func presenteTrace(_ trace: GPXImport.Trace) {
        let coords = trace.points
        guard coords.count >= 2, let arrivee = coords.last else { return }
        var km = 0.0
        for i in 1..<coords.count {
            km += metersBetween(coords[i - 1], coords[i])
        }
        km /= 1_000
        let vitesse: Double =
            travelMode == .car ? 50 : travelMode == .bike ? 16 : 4.5
        let steps = turnSteps(from: coords)
        var option = RouteOption(
            coordinates: coords, steps: steps,
            minutes: km / vitesse * 60, km: km,
            turns: steps.count)
        option.isSimplest = true
        option.color = .green
        traceGPX = option
        etapes = []
        etapeNoms = []
        etapeArrets = []
        etapesAtteintes = []
        pendingDestination = arrivee
        routeOptions = [option]
        selectedRouteID = option.id
        routesExpanded = false
        fitCoordinates(coords)
        Task { await chargeRisques(options: [option], dest: arrivee) }
    }

    /// Compose le lien de partage du trajet en cours et ouvre la feuille de
    /// partage. Le lien est une page web du courtier : tout voyage dans ses
    /// paramètres (destination, étapes, mode, nom géocodé) — aucun stockage,
    /// rien n'expire. La page tend le lien profond `moncap://` à qui a l'app.
    private func partageTrajet() {
        guard let dest = pendingDestination else { return }
        let etapesActuelles = etapes
        let mode = travelMode.rawValue
        Task {
            var items = [
                URLQueryItem(
                    name: "a",
                    value: String(format: "%.5f,%.5f", dest.latitude, dest.longitude)),
                URLQueryItem(name: "mode", value: mode),
            ]
            if !etapesActuelles.isEmpty {
                let v = etapesActuelles.prefix(5)
                    .map { String(format: "%.5f,%.5f", $0.latitude, $0.longitude) }
                    .joined(separator: "|")
                items.append(URLQueryItem(name: "v", value: v))
            }
            // Le nom de l'endroit, si le géocodage veut bien le dire — le
            // lien reste bon sans lui.
            if let lieu = try? await CLGeocoder()
                .reverseGeocodeLocation(
                    CLLocation(latitude: dest.latitude, longitude: dest.longitude)
                ).first, let nom = lieu.name ?? lieu.locality
            {
                items.append(URLQueryItem(name: "nom", value: String(nom.prefix(60))))
            }
            var comps = URLComponents(
                url: APIClient().baseURL.appendingPathComponent("t"),
                resolvingAgainstBaseURL: false)!
            comps.queryItems = items
            guard let url = comps.url else { return }
            Sharer.present([url])
        }
    }

    /// Traite une demande de guidage venue de Siri : calcule vers le point
    /// demandé, et démarre le guidage sur la meilleure option quand la
    /// demande le dit (« Ramène-moi à la maison » part sans autre geste).
    private func traiteGuidageDemande() {
        guard let demande = GuidageDemande.releve() else { return }
        let dest = CLLocationCoordinate2D(latitude: demande.lat, longitude: demande.lon)
        showSearch = false
        showPlaces = false
        Task {
            await presentRouteOptions(to: dest)
            if demande.demarre, let o = routeOptions.first {
                startNavigation(option: o)
            }
        }
    }

    /// Ouvre un trajet reçu par lien profond `moncap://trajet?…` : le mode,
    /// les étapes et la destination reprennent leur place, et le calcul part.
    private func ouvreLien(_ url: URL) {
        guard url.scheme == "moncap",
            let comps = URLComponents(url: url, resolvingAgainstBaseURL: false)
        else { return }
        // moncap://favori?nom=domicile|travail — le widget d'accueil : il ne
        // porte aucune adresse, c'est ici qu'on résout celle du moment.
        if comps.host == "favori" {
            let nom = comps.queryItems?.first { $0.name == "nom" }?.value
            guard let lieu = nom == "travail" ? workPlace : homePlace else { return }
            let dest = CLLocationCoordinate2D(latitude: lieu.lat, longitude: lieu.lon)
            etapes = []
            etapeNoms = []
            etapeArrets = []
            etapesAtteintes = []
            traceGPX = nil
            showSearch = false
            showPlaces = false
            pendingDestination = dest
            Task { await presentRouteOptions(to: dest) }
            return
        }
        guard comps.host == "trajet" else { return }
        let items = comps.queryItems ?? []
        func valeur(_ nom: String) -> String? {
            items.first { $0.name == nom }?.value
        }
        func coord(_ brut: String?) -> CLLocationCoordinate2D? {
            guard let parts = brut?.components(separatedBy: ","), parts.count == 2,
                let lat = Double(parts[0]), let lon = Double(parts[1]),
                abs(lat) <= 90, abs(lon) <= 180
            else { return nil }
            return CLLocationCoordinate2D(latitude: lat, longitude: lon)
        }
        guard let dest = coord(valeur("a")) else { return }
        if let mode = valeur("mode").flatMap(TravelMode.init(rawValue:)) {
            travelMode = mode
        }
        etapes = []
        etapeNoms = []
        etapeArrets = []
        etapesAtteintes = []
        traceGPX = nil
        if let brut = valeur("v") {
            for (i, morceau) in brut.components(separatedBy: "|").prefix(5).enumerated() {
                guard let c = coord(morceau) else { continue }
                etapes.append(c)
                etapeArrets.append(0)
                etapeNoms.append(
                    String(format: NSLocalizedString("Étape %d", comment: ""), i + 1))
            }
        }
        showSearch = false
        showPlaces = false
        pendingDestination = dest
        Task { await presentRouteOptions(to: dest) }
    }
}
