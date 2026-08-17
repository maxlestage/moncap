import CoreLocation
import MapLibre
import SwiftUI

/// Qualité de l'air, servie par notre backend (`GET /air`).
///
/// La mesure vient d'Open-Meteo, gratuite et **sans clé**, interrogée par le
/// serveur pour toute une grille en une requête : une carte se lit par taches,
/// pas par relevés isolés.
///
/// L'écran peint une **surface continue** et non des pastilles. Une pastille
/// pose la mesure en un point et laisse croire qu'ailleurs on ne sait rien,
/// alors que le modèle donne une valeur partout ; et à l'échelle d'une ville
/// elle se réduisait à un rond vert unique au milieu du vide. Chaque mesure
/// couvre donc sa maille, et les mailles pavent l'écran.
///
/// Deux indices arrivent, et l'écran n'en montre qu'un. Contrairement à ce que
/// j'avais supposé, Open-Meteo calcule l'indice **européen partout** — relevé
/// au-dessus du Sahara, où il vaut 33. Il sert donc d'échelle unique, ce qui
/// vaut mieux : une carte qui change de convention selon le continent ne se
/// compare plus à elle-même.
///
/// L'**américain** reste dans la réponse, comme repli s'il venait à manquer, et
/// parce qu'il dit autre chose : au-dessus du Sahara il monte à 1142 quand
/// l'européen plafonne à 33 — les poussières n'y sont pas pondérées pareil. Les
/// deux ne se superposent pas, d'où l'échelle nommée à l'écran.
struct AirPoint: Identifiable, Decodable {
    var id: String { "\(lat),\(lon)" }
    let lat: Double
    let lon: Double
    /// Indice européen. Présent partout, en pratique.
    let eu: Double?
    /// Indice américain, conservé comme repli.
    let us: Double?
    /// Particules fines, µg/m³ — la seule grandeur physique du lot.
    let pm25: Double?
    /// Hauteur et largeur de la maille couverte par la mesure, en degrés.
    ///
    /// Facultatives : un serveur d'avant cette version n'en envoie pas, et
    /// l'app ne doit pas s'en trouver vide. Sans elles, la maille par défaut
    /// vaut celle du modèle CAMS, un cinquième de degré.
    let dlat: Double?
    let dlon: Double?

    var coordinate: CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: lat, longitude: lon)
    }

    /// Le rectangle couvert par la mesure.
    var maille: (dlat: Double, dlon: Double) { (dlat ?? 0.2, dlon ?? 0.2) }

    /// Vrai si le point tombe dans la maille.
    func contient(_ c: CLLocationCoordinate2D) -> Bool {
        let (dla, dlo) = maille
        return abs(c.latitude - lat) <= dla / 2 && abs(c.longitude - lon) <= dlo / 2
    }

    /// Échelle retenue pour ce point : l'européenne si elle existe.
    var echelle: AirEchelle { eu != nil ? .europeenne : .americaine }

    /// Valeur affichée, dans l'échelle retenue.
    var valeur: Double? { eu ?? us }

    /// Niveau de 0 (bon) à 5 (extrêmement mauvais), commun aux deux échelles
    /// pour que la couleur reste lisible d'un continent à l'autre.
    var niveau: Int {
        guard let v = valeur else { return 0 }
        let paliers = echelle.paliers
        return paliers.firstIndex(where: { v <= $0 }) ?? paliers.count
    }
}

/// Les deux conventions, avec leurs seuils propres.
enum AirEchelle {
    case europeenne
    case americaine

    /// Bornes hautes des cinq premiers niveaux.
    var paliers: [Double] {
        switch self {
        // Indice européen (EAQI) : bon, correct, dégradé, mauvais, très mauvais.
        case .europeenne: return [20, 40, 60, 80, 100]
        // Indice américain (AQI) : bon, modéré, mauvais pour les sensibles,
        // mauvais, très mauvais.
        case .americaine: return [50, 100, 150, 200, 300]
        }
    }

    var nom: LocalizedStringKey {
        switch self {
        case .europeenne: return "Indice européen"
        case .americaine: return "Indice américain"
        }
    }
}

/// Les six couleurs des niveaux, du turquoise au pourpre.
///
/// Ce sont celles de l'indice européen officiel : reprendre une convention
/// existante vaut mieux qu'inventer une palette que personne ne sait lire.
enum AirCouleurs {
    static let niveaux: [UIColor] = [
        UIColor(red: 0.31, green: 0.78, blue: 0.76, alpha: 1),  // bon
        UIColor(red: 0.31, green: 0.85, blue: 0.55, alpha: 1),  // correct
        UIColor(red: 0.94, green: 0.87, blue: 0.36, alpha: 1),  // dégradé
        UIColor(red: 0.94, green: 0.51, blue: 0.35, alpha: 1),  // mauvais
        UIColor(red: 0.84, green: 0.24, blue: 0.33, alpha: 1),  // très mauvais
        UIColor(red: 0.51, green: 0.16, blue: 0.36, alpha: 1),  // extrêmement mauvais
    ]

    static func couleur(_ niveau: Int) -> UIColor {
        niveaux[min(max(niveau, 0), niveaux.count - 1)]
    }

    static let libelles: [LocalizedStringKey] = [
        "Bon", "Correct", "Dégradé", "Mauvais", "Très mauvais", "Extrêmement mauvais",
    ]
}

/// Va chercher la grille pour la zone regardée.
@MainActor
final class AirService: ObservableObject {
    @Published private(set) var points: [AirPoint] = []
    @Published private(set) var chargement = false
    @Published private(set) var message: String?

    private var tache: Task<Void, Never>?
    /// Zone déjà reprise une fois. Un simple drapeau ne suffirait pas : la
    /// reprise rend elle aussi une grille vide au-dessus d'un océan, et en
    /// relancerait une autre — une boucle de quatre secondes, indéfiniment.
    private var zoneReprise: String?
    private let base = APIClient().baseURL

    /// Demande la grille d'une zone. Une nouvelle demande annule la précédente :
    /// en faisant glisser la carte, seule la dernière compte.
    func charge(bbox: String) {
        tache?.cancel()
        chargement = true
        let base = self.base
        tache = Task {
            let recus = await Self.fetch(base: base, bbox: bbox)
            if Task.isCancelled { return }
            chargement = false
            if let recus {
                points = recus
                // Vide sur une zone neuve, c'est le plus souvent le serveur
                // qui a rendu la main avant d'avoir sa donnée. Une seule
                // reprise la cueille, et le message d'océan n'apparaît qu'après
                // elle — sinon il accuserait la mer d'une simple attente.
                if recus.isEmpty, zoneReprise != bbox {
                    zoneReprise = bbox
                    message = nil
                    try? await Task.sleep(for: .seconds(4))
                    charge(bbox: bbox)
                    return
                }
                message = recus.isEmpty
                    ? String(localized: "Aucune mesure ici. Rapproche-toi des terres.")
                    : nil
            } else {
                message = String(localized: "Mesures indisponibles — il faut du réseau.")
            }
        }
    }

    nonisolated private static func fetch(base: URL, bbox: String) async -> [AirPoint]? {
        var comps = URLComponents(
            url: base.appendingPathComponent("air"), resolvingAgainstBaseURL: false)!
        comps.queryItems = [URLQueryItem(name: "bbox", value: bbox)]
        guard
            let url = comps.url,
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let points = try? JSONDecoder().decode([AirPoint].self, from: data)
        else { return nil }
        return points
    }
}

/// Écran dédié « Qualité de l'air » : une nappe colorée sur le fond de carte
/// habituel, et une légende qui nomme l'échelle employée.
struct AirMapScreen: View {
    @Environment(\.dismiss) private var dismiss
    /// Position d'ouverture : on regarde d'abord là où on est.
    let center: CLLocationCoordinate2D?

    @AppStorage(TileSource.styleKey) private var styleURLString = ""
    @StateObject private var service = AirService()
    @State private var style = URL(string: TileSource.defaultStyle)!
    @State private var bounds: MLNCoordinateBounds?
    @State private var selection: AirPoint?
    @State private var cam = MapCam(
        center: CLLocationCoordinate2D(latitude: 46.6, longitude: 2.4), zoom: 4.6)

    var body: some View {
        ZStack(alignment: .top) {
            AirMapLibreView(
                styleURL: style,
                points: service.points,
                cam: cam,
                onRegionChange: { bounds = $0 },
                onSelect: { selection = $0 }
            )
            .ignoresSafeArea()

            VStack(spacing: 10) {
                entete
                if let m = service.message {
                    Text(m)
                        .font(.caption)
                        .multilineTextAlignment(.center)
                        .padding(10)
                        .frame(maxWidth: .infinity)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
                        .padding(.horizontal)
                }
            }

            VStack {
                Spacer()
                if let p = selection { fiche(p) }
                legende
            }
        }
        .task(id: styleURLString) {
            style = await TileSource.resolvedStyle(custom: styleURLString)
        }
        // Même découpage que la reconnaissance de région : on ne relance pas
        // une grille à chaque pixel de glissement.
        .task(id: caseRegardee) {
            if let b = bounds { service.charge(bbox: Self.bbox(b)) }
        }
        .onAppear {
            if let center { cam = MapCam(center: center, zoom: 7) }
        }
    }

    /// Case de 0,25° du centre : identité de la zone regardée.
    private var caseRegardee: String? {
        guard let b = bounds else { return nil }
        let la = (b.sw.latitude + b.ne.latitude) / 2
        let lo = (b.sw.longitude + b.ne.longitude) / 2
        return "\((la * 4).rounded())|\((lo * 4).rounded())"
    }

    private static func bbox(_ b: MLNCoordinateBounds) -> String {
        String(
            format: "%.3f,%.3f,%.3f,%.3f",
            b.sw.longitude, b.sw.latitude, b.ne.longitude, b.ne.latitude)
    }

    private var entete: some View {
        HStack(spacing: 10) {
            Button { dismiss() } label: {
                Image(systemName: "xmark")
                    .font(.headline)
                    .foregroundStyle(.primary)
                    .frame(width: 40, height: 40)
                    .background(.regularMaterial, in: Circle())
            }
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Qualité de l'air").font(.headline)
                    Text(service.points.first?.echelle.nom ?? "Indice européen")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if service.chargement { ProgressView().controlSize(.small) }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.regularMaterial, in: Capsule())
            Spacer()
        }
        .padding()
    }

    /// Fiche d'un point touché : la valeur, son niveau, et les particules —
    /// la seule grandeur que l'on peut comparer d'un continent à l'autre.
    private func fiche(_ p: AirPoint) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Circle()
                    .fill(Color(AirCouleurs.couleur(p.niveau)))
                    .frame(width: 14, height: 14)
                Text(AirCouleurs.libelles[min(p.niveau, AirCouleurs.libelles.count - 1)])
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Button { selection = nil } label: {
                    Image(systemName: "xmark").font(.footnote.weight(.bold))
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
            }
            if let v = p.valeur {
                Text(verbatim: "\(p.echelle == .europeenne ? "EAQI" : "US AQI") \(Int(v))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            if let pm = p.pm25 {
                Text(String(format: String(localized: "Particules fines : %.0f µg/m³"), pm))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
        .padding(14)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
        .padding(.horizontal)
    }

    /// Légende : sans elle, six couleurs ne disent rien.
    private var legende: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 0) {
                ForEach(Array(AirCouleurs.niveaux.enumerated()), id: \.offset) { _, c in
                    Rectangle().fill(Color(c)).frame(height: 8)
                }
            }
            .clipShape(Capsule())
            HStack {
                Text("Bon").font(.caption2)
                Spacer()
                Text("Extrêmement mauvais").font(.caption2)
            }
            .foregroundStyle(.secondary)
            Text("Mesures Open-Meteo (CAMS)")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .padding(12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        .padding()
    }
}

/// La carte elle-même : une source GeoJSON et une couche de mailles pleines.
///
/// Une couche de style plutôt que des annotations : quatre cents mailles en
/// annotations coûteraient quatre cents vues UIKit, et la couleur se pilote ici
/// par une expression, du côté du moteur de rendu.
struct AirMapLibreView: UIViewRepresentable {
    let styleURL: URL
    let points: [AirPoint]
    let cam: MapCam?
    let onRegionChange: (MLNCoordinateBounds) -> Void
    let onSelect: (AirPoint) -> Void

    func makeUIView(context: Context) -> SizedMapView {
        let view = SizedMapView(frame: .zero, styleURL: styleURL)
        context.coordinator.appliedStyle = styleURL
        view.delegate = context.coordinator
        view.showsUserLocation = true
        view.attributionButtonPosition = .bottomRight

        let tap = UITapGestureRecognizer(
            target: context.coordinator, action: #selector(Coordinator.handleTap(_:)))
        view.addGestureRecognizer(tap)
        return view
    }

    func updateUIView(_ view: SizedMapView, context: Context) {
        let c = context.coordinator
        c.onSelect = onSelect
        c.onRegionChange = onRegionChange
        c.points = points
        if c.appliedStyle != styleURL {
            c.appliedStyle = styleURL
            c.styleReady = false
            view.styleURL = styleURL
        }
        if let cam, cam != c.appliedCam {
            c.appliedCam = cam
            view.setCenter(cam.center, zoomLevel: cam.zoom, animated: false)
        }
        c.sync(on: view)
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator: NSObject, MLNMapViewDelegate {
        var onSelect: (AirPoint) -> Void = { _ in }
        var onRegionChange: (MLNCoordinateBounds) -> Void = { _ in }
        var points: [AirPoint] = []
        var appliedStyle: URL?
        var appliedCam: MapCam?
        var styleReady = false
        /// Signature de ce qui est dessiné : sans elle, la couche serait
        /// refaite à chaque réévaluation de la vue.
        private var dessine: String = ""

        private static let source = "air-src"
        private static let couche = "air-mailles"

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            styleReady = true
            dessine = ""
            sync(on: mapView)
        }

        func mapView(_ mapView: MLNMapView, regionDidChangeAnimated animated: Bool) {
            onRegionChange(mapView.visibleCoordinateBounds)
        }

        func sync(on view: MLNMapView) {
            guard styleReady, let style = view.style else { return }
            let signature = points.map(\.id).joined(separator: ";")
            if signature == dessine { return }
            dessine = signature

            if let vieille = style.layer(withIdentifier: Self.couche) {
                style.removeLayer(vieille)
            }
            if let vieille = style.source(withIdentifier: Self.source) {
                style.removeSource(vieille)
            }
            guard !points.isEmpty else { return }

            // Une maille par mesure, et les mailles se touchent : mises bout à
            // bout elles pavent l'écran, sans ruban de fond nu entre elles.
            let formes: [MLNPolygonFeature] = points.map { p in
                let (dla, dlo) = p.maille
                let su = p.lat - dla / 2, no = p.lat + dla / 2
                let o = p.lon - dlo / 2, e = p.lon + dlo / 2
                var coins = [
                    CLLocationCoordinate2D(latitude: su, longitude: o),
                    CLLocationCoordinate2D(latitude: su, longitude: e),
                    CLLocationCoordinate2D(latitude: no, longitude: e),
                    CLLocationCoordinate2D(latitude: no, longitude: o),
                ]
                let f = MLNPolygonFeature(coordinates: &coins, count: UInt(coins.count))
                f.attributes = ["niveau": p.niveau]
                return f
            }
            let src = MLNShapeSource(
                identifier: Self.source,
                shape: MLNShapeCollectionFeature(shapes: formes), options: nil)
            style.addSource(src)

            let couche = MLNFillStyleLayer(identifier: Self.couche, source: src)
            // Couleur pilotée par l'attribut, côté moteur de rendu.
            let teinte = NSExpression(
                format: "TERNARY(niveau = 0, %@, TERNARY(niveau = 1, %@, "
                    + "TERNARY(niveau = 2, %@, TERNARY(niveau = 3, %@, "
                    + "TERNARY(niveau = 4, %@, %@)))))",
                AirCouleurs.niveaux[0], AirCouleurs.niveaux[1], AirCouleurs.niveaux[2],
                AirCouleurs.niveaux[3], AirCouleurs.niveaux[4], AirCouleurs.niveaux[5])
            couche.fillColor = teinte
            // Le contour de la même couleur que le fond : sans lui, deux mailles
            // voisines laissent parfois passer un cheveu de fond entre elles,
            // artefact de tramage qui hachurerait toute la surface.
            couche.fillOutlineColor = teinte
            // Assez opaque pour se lire d'un coup d'œil, assez transparent pour
            // qu'on reconnaisse encore le pays sous la couleur.
            couche.fillOpacity = NSExpression(forConstantValue: 0.45)

            // Glissée **sous** les libellés : une nappe posée par-dessus noierait
            // les noms de villes, et une carte de l'air dont on ne lit plus les
            // villes ne dit plus où l'on respire mal.
            if let premierTexte = style.layers.first(where: { $0 is MLNSymbolStyleLayer }) {
                style.insertLayer(couche, below: premierTexte)
            } else {
                style.addLayer(couche)
            }
        }

        @objc func handleTap(_ g: UITapGestureRecognizer) {
            guard let view = g.view as? MLNMapView else { return }
            let touche = view.convert(g.location(in: view), toCoordinateFrom: view)
            // La maille sous le doigt d'abord : elle couvre une surface, il n'y
            // a donc pas à chercher le plus proche voisin.
            if let dedans = points.first(where: { $0.contient(touche) }) {
                onSelect(dedans)
                return
            }
            // Hors de la grille — un bord d'écran —, le plus proche s'il est
            // assez près.
            let plus = points.min {
                Self.ecart($0.coordinate, touche) < Self.ecart($1.coordinate, touche)
            }
            if let plus, Self.ecart(plus.coordinate, touche) < 1.0 { onSelect(plus) }
        }

        /// Écart au carré en degrés — suffisant pour comparer entre eux des
        /// points d'une même grille, et sans trigonométrie.
        private static func ecart(
            _ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D
        ) -> Double {
            let dla = a.latitude - b.latitude
            let dlo = (a.longitude - b.longitude) * cos(a.latitude * .pi / 180)
            return dla * dla + dlo * dlo
        }
    }
}
