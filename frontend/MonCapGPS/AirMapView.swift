import CoreLocation
import MapLibre
import SwiftUI

/// Qualité de l'air, servie par notre backend (`GET /air`).
///
/// La mesure vient d'Open-Meteo, gratuite et **sans clé**, interrogée par le
/// serveur pour toute une grille en une requête : une carte se lit par taches,
/// pas par relevés isolés.
///
/// Deux indices coexistent, et ce n'est pas un doublon. L'indice **européen**
/// n'est calculé que sur le domaine européen de CAMS ; l'**américain** l'est
/// partout. Une carte mondiale doit donc savoir passer de l'un à l'autre — et
/// le dire, car leurs échelles n'ont rien de comparable : 100 est déjà mauvais
/// en Europe, encore acceptable aux États-Unis.
struct AirPoint: Identifiable, Decodable {
    var id: String { "\(lat),\(lon)" }
    let lat: Double
    let lon: Double
    /// Indice européen, absent hors d'Europe.
    let eu: Double?
    /// Indice américain, calculé partout.
    let us: Double?
    /// Particules fines, µg/m³ — la seule grandeur physique du lot.
    let pm25: Double?

    var coordinate: CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: lat, longitude: lon)
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
                // Une grille vide n'est pas une erreur : au-dessus d'un océan,
                // Open-Meteo n'a simplement rien à dire.
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

/// Écran dédié « Qualité de l'air » : des pastilles colorées sur le fond de
/// carte habituel, et une légende qui nomme l'échelle employée.
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

/// La carte elle-même : une source GeoJSON et une couche de pastilles.
///
/// Une couche de style plutôt que des annotations : cent points en annotations
/// coûtent cent vues UIKit, et la couleur se pilote ici par une expression, du
/// côté du moteur de rendu.
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
        private static let couche = "air-pastilles"

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

            let formes: [MLNPointFeature] = points.map { p in
                let f = MLNPointFeature()
                f.coordinate = p.coordinate
                f.attributes = ["niveau": p.niveau]
                return f
            }
            let src = MLNShapeSource(
                identifier: Self.source,
                shape: MLNShapeCollectionFeature(shapes: formes), options: nil)
            style.addSource(src)

            let couche = MLNCircleStyleLayer(identifier: Self.couche, source: src)
            // Couleur pilotée par l'attribut, côté moteur de rendu.
            couche.circleColor = NSExpression(
                format: "TERNARY(niveau = 0, %@, TERNARY(niveau = 1, %@, "
                    + "TERNARY(niveau = 2, %@, TERNARY(niveau = 3, %@, "
                    + "TERNARY(niveau = 4, %@, %@)))))",
                AirCouleurs.niveaux[0], AirCouleurs.niveaux[1], AirCouleurs.niveaux[2],
                AirCouleurs.niveaux[3], AirCouleurs.niveaux[4], AirCouleurs.niveaux[5])
            couche.circleRadius = NSExpression(forConstantValue: 14)
            couche.circleOpacity = NSExpression(forConstantValue: 0.62)
            couche.circleStrokeWidth = NSExpression(forConstantValue: 1)
            couche.circleStrokeColor = NSExpression(
                forConstantValue: UIColor.white.withAlphaComponent(0.7))
            style.addLayer(couche)
        }

        @objc func handleTap(_ g: UITapGestureRecognizer) {
            guard let view = g.view as? MLNMapView else { return }
            let touche = view.convert(g.location(in: view), toCoordinateFrom: view)
            // Le point le plus proche du doigt, s'il est assez près.
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
