import MapKit
import MapLibre
import SwiftUI

/// Un contour de feu approximatif : polygone (enveloppe des points chauds d'un
/// même foyer) + son centre et l'heure de la dernière détection satellite.
struct FirePerimeter: Identifiable {
    let id: Int
    let polygon: [CLLocationCoordinate2D]
    let center: CLLocationCoordinate2D
    /// Détection satellite la plus récente du foyer (pour « Mise à jour il y a X h »).
    let updatedAt: Date?
    /// Première détection du foyer sur la fenêtre de sept jours (pour
    /// « Créé il y a X j »). Nil tant que le serveur ne la connaît pas.
    let startedAt: Date?
}

/// Écran dédié « Carte des feux » à la manière de feuxdeforet.fr : fond
/// satellite, contours de feu en rouge translucide et marqueurs flamme. Séparé
/// de la carte GPS pour ne pas l'alourdir. Les contours sont approximés à partir
/// des points chauds satellite (NASA FIRMS) regroupés par foyer.
/// Fond satellite de la carte des feux.
///
/// L'IGN diffuse l'orthophotographie de la France en 20 cm sous Licence
/// Ouverte, sans clé ni inscription — meilleure résolution que le fond
/// satellite d'Apple, et les feux suivis sont métropolitains de toute façon.
/// L'attribution est affichée à l'écran, la licence l'exige.
enum OrthoSource {
    static let template =
        "https://data.geopf.fr/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0"
        + "&LAYER=ORTHOIMAGERY.ORTHOPHOTOS&STYLE=normal&FORMAT=image/jpeg"
        + "&TILEMATRIXSET=PM&TILEMATRIX={z}&TILEROW={y}&TILECOL={x}"
    static let attribution = "© IGN · © OpenStreetMap"
}

/// Carte des feux rendue par MapLibre : orthophoto par-dessus le fond
/// OpenStreetMap, contours de feu et marqueurs.
struct FireMapLibreView: UIViewRepresentable {
    let styleURL: URL
    let perimeters: [FirePerimeter]
    /// Les détections satellite telles quelles.
    ///
    /// Elles sont dessinées **en plus** des contours, et c'est ce qui rend la
    /// carte fiable à toute échelle.
    ///
    /// Les contours passent par une projection locale en mètres, ancrée sur la
    /// première détection du groupe et n'employant qu'un seul cosinus de
    /// latitude. C'est juste sur un foyer, qui tient dans quelques kilomètres.
    /// Mais le regroupement se fait de proche en proche : à l'échelle du monde,
    /// des milliers de détections se soudent en un groupe continental, et la
    /// projection place alors les contours très loin de leur position réelle —
    /// parfois hors du globe, auquel cas rien ne s'affiche. On lisait « 1467
    /// feux en cours » sur une carte vide : les contours existaient, ils
    /// n'étaient simplement pas là où on regardait.
    ///
    /// Un point par détection ne dépend, lui, d'aucune projection.
    let fires: [Fire]
    /// Zone visible, pour ne demander que ce qu'on regarde.
    var onRegionChange: (MLNCoordinateBounds) -> Void = { _ in }
    /// Identifiant du foyer touché.
    let onSelect: (Int) -> Void

    func makeUIView(context: Context) -> SizedMapView {
        let view = SizedMapView(frame: .zero, styleURL: styleURL)
        context.coordinator.appliedStyle = styleURL
        view.delegate = context.coordinator
        view.showsUserLocation = false
        view.logoView.isHidden = false
        view.attributionButtonPosition = .bottomRight
        // Rien de ce qui dépend de la taille ne doit courir avant la première
        // mise en page : le style peut sortir du cache et les feux être déjà
        // en mémoire, tout arrivait alors sur une vue de taille nulle — cadrage
        // sur le monde entier, épingles perdues — et ne se rejouait jamais.
        view.onLayout = { [weak view, weak coordinator = context.coordinator] in
            guard let view, let coordinator else { return }
            coordinator.layoutReady(view)
        }

        let tap = UITapGestureRecognizer(
            target: context.coordinator, action: #selector(Coordinator.handleTap(_:)))
        view.addGestureRecognizer(tap)
        return view
    }

    func updateUIView(_ view: SizedMapView, context: Context) {
        context.coordinator.onSelect = onSelect
        // Le style est résolu après coup (vérification réseau) : sans ce
        // report, un serveur personnel n'était jamais honoré ici. Comparé à la
        // demande, jamais à ce que la vue renvoie pendant un chargement.
        if context.coordinator.appliedStyle != styleURL {
            context.coordinator.changeStyle(to: styleURL, on: view)
        }
        context.coordinator.fires = fires
        context.coordinator.onRegionChange = onRegionChange
        context.coordinator.sync(perimeters, on: view)
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator: NSObject, MLNMapViewDelegate {
        var onSelect: (Int) -> Void = { _ in }
        var onRegionChange: (MLNCoordinateBounds) -> Void = { _ in }
        private var styleReady = false
        private var laidOut = false
        private var drawn: [String] = []
        /// Le cadrage n'est fait qu'une fois : le refaire à chaque
        /// rafraîchissement ramènerait l'utilisateur de force.
        private var fitted = false
        /// Derniers foyers reçus, rejoués dès que style ET mise en page sont là.
        private var last: [FirePerimeter] = []
        /// Détections brutes, redessinées avec le reste.
        var fires: [Fire] = []
        /// Style demandé à la vue ; fait foi pendant le chargement.
        var appliedStyle: URL?

        /// Bascule vers un autre fond : tout sera reposé au rechargement.
        func changeStyle(to url: URL, on view: MLNMapView) {
            appliedStyle = url
            styleReady = false
            // Tout étant en couches de style, le rechargement fait le ménage :
            // il ne reste rien à retirer à la main.
            drawn = []
            view.styleURL = url
        }

        func mapView(_ mapView: MLNMapView, regionDidChangeAnimated animated: Bool) {
            onRegionChange(mapView.visibleCoordinateBounds)
        }

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            addOrtho(to: style)
            styleReady = true
            redraw(on: mapView)
        }

        /// Première mise en page utile : la vue a enfin une taille.
        func layoutReady(_ view: MLNMapView) {
            guard !laidOut, view.bounds.height > 10 else { return }
            laidOut = true
            redraw(on: view)
        }

        /// Redessine tout depuis `last`, en oubliant ce qui a pu être posé
        /// sur une vue pas encore prête.
        private func redraw(on view: MLNMapView) {
            drawn = []
            sync(last, on: view)
        }

        /// Orthophoto posée au-dessus du fond : les libellés disparaissent
        /// dessous, c'est l'effet recherché pour une vue satellite.
        private func addOrtho(to style: MLNStyle) {
            guard style.source(withIdentifier: "ortho") == nil else { return }
            let source = MLNRasterTileSource(
                identifier: "ortho",
                tileURLTemplates: [OrthoSource.template],
                options: [.tileSize: 256, .minimumZoomLevel: 2, .maximumZoomLevel: 19])
            style.addSource(source)
            let layer = MLNRasterStyleLayer(identifier: "ortho-fond", source: source)
            style.addLayer(layer)
        }

        func sync(_ perimeters: [FirePerimeter], on view: MLNMapView) {
            last = perimeters
            guard styleReady, laidOut, let style = view.style else { return }
            // Les identifiants sont séquentiels (0..n-1) : ils n'encodent que
            // le nombre de foyers. La garde porte sur la géométrie réelle,
            // sinon un foyer qui grossit à effectif constant restait figé.
            let signature = perimeters.map {
                "\(Int($0.center.latitude * 1e5)),\(Int($0.center.longitude * 1e5))"
                    + ",\($0.polygon.count),\(Int($0.updatedAt?.timeIntervalSince1970 ?? 0))"
            }
            // Les détections comptent dans la signature : sans cela, une
            // carte qui en reçoit de nouvelles sans changer de contours ne se
            // redessinerait pas.
            let empreinte = signature + ["pts:\(fires.count)"]
            guard empreinte != drawn else { return }
            drawn = empreinte

            for name in ["feux-points", "feux-coeur", "feux-halo", "feux-liseré", "feux-contour"] {
                if let layer = style.layer(withIdentifier: name) { style.removeLayer(layer) }
            }
            for name in ["feux-src", "feux-pts", "feux-detections"] {
                if let source = style.source(withIdentifier: name) { style.removeSource(source) }
            }
            // Les détections d'abord : elles s'affichent même quand aucun
            // contour n'a pu être calculé, ce qui est le cas dès qu'on dézoome.
            if !fires.isEmpty {
                let pts = fires.map { f -> MLNPointFeature in
                    let pt = MLNPointFeature()
                    pt.coordinate = f.coordinate
                    return pt
                }
                let src = MLNShapeSource(
                    identifier: "feux-detections",
                    shape: MLNShapeCollectionFeature(shapes: pts), options: nil)
                style.addSource(src)

                let points = MLNCircleStyleLayer(identifier: "feux-points", source: src)
                // Le rayon suit le zoom : un point de 2 px à l'échelle du monde
                // laisse voir les foyers sans noyer les continents de rouge ;
                // de près, il s'efface derrière les contours, plus précis.
                points.circleRadius = NSExpression(
                    format: "mgl_interpolate:withCurveType:parameters:stops:"
                        + "($zoomLevel, 'linear', nil, %@)",
                    [2: 2.0, 5: 3.5, 8: 5.0, 11: 6.0])
                points.circleColor = NSExpression(
                    forConstantValue: UIColor(red: 0.95, green: 0.35, blue: 0.15, alpha: 1))
                points.circleOpacity = NSExpression(forConstantValue: 0.9)
                points.circleStrokeWidth = NSExpression(forConstantValue: 0.5)
                points.circleStrokeColor = NSExpression(
                    forConstantValue: UIColor.white.withAlphaComponent(0.6))
                style.addLayer(points)
            }

            guard !perimeters.isEmpty else { return }

            // Un seul jeu de formes pour tous les foyers : deux couches
            // suffisent alors, quel que soit leur nombre.
            var shapes: [MLNShape] = []
            for p in perimeters where p.polygon.count >= 3 {
                var coords = p.polygon
                shapes.append(MLNPolygonFeature(coordinates: &coords, count: UInt(coords.count)))
            }
            let source = MLNShapeSource(
                identifier: "feux-src", shape: MLNShapeCollectionFeature(shapes: shapes),
                options: nil)
            style.addSource(source)

            let fill = MLNFillStyleLayer(identifier: "feux-contour", source: source)
            // Brun brûlé translucide cerné de rouge vif, comme feuxdeforet.fr.
            fill.fillColor = NSExpression(
                forConstantValue: UIColor(red: 0.48, green: 0.20, blue: 0.13, alpha: 1))
            fill.fillOpacity = NSExpression(forConstantValue: 0.55)
            style.addLayer(fill)

            let stroke = MLNLineStyleLayer(identifier: "feux-liseré", source: source)
            stroke.lineColor = NSExpression(
                forConstantValue: UIColor(red: 0.86, green: 0.13, blue: 0.11, alpha: 1))
            stroke.lineWidth = NSExpression(forConstantValue: 2)
            style.addLayer(stroke)

            // Marqueurs en couches de style plutôt qu'en épingles.
            //
            // Les contours mesurent quelques kilomètres : à l'échelle de la
            // France ils sont sous le pixel, seul le marqueur situe le foyer.
            // Une couche de cercles se dessine par le même chemin que les
            // contours et tient sans effort à cent exemplaires.
            var points: [MLNPointFeature] = []
            for p in perimeters {
                let f = MLNPointFeature()
                f.coordinate = p.center
                f.attributes = ["feu": p.id]
                points.append(f)
            }
            let centres = MLNShapeSource(
                identifier: "feux-pts", shape: MLNShapeCollectionFeature(shapes: points),
                options: nil)
            style.addSource(centres)

            let red = UIColor(red: 0.86, green: 0.13, blue: 0.11, alpha: 1)
            let halo = MLNCircleStyleLayer(identifier: "feux-halo", source: centres)
            halo.circleRadius = NSExpression(forConstantValue: 13)
            halo.circleColor = NSExpression(forConstantValue: red)
            halo.circleOpacity = NSExpression(forConstantValue: 0.28)
            style.addLayer(halo)

            let coeur = MLNCircleStyleLayer(identifier: "feux-coeur", source: centres)
            coeur.circleRadius = NSExpression(forConstantValue: 5.5)
            coeur.circleColor = NSExpression(forConstantValue: red)
            coeur.circleStrokeColor = NSExpression(forConstantValue: UIColor.white)
            coeur.circleStrokeWidth = NSExpression(forConstantValue: 1.5)
            style.addLayer(coeur)

            fitIfNeeded(perimeters, on: view)
        }

        /// Cadre sur l'ensemble des foyers, une seule fois.
        private func fitIfNeeded(_ perimeters: [FirePerimeter], on view: MLNMapView) {
            guard !fitted, view.bounds.height > 10 else { return }
            let points = perimeters.flatMap(\.polygon)
            guard let first = points.first else { return }
            var minLat = first.latitude, maxLat = first.latitude
            var minLon = first.longitude, maxLon = first.longitude
            for c in points {
                minLat = min(minLat, c.latitude); maxLat = max(maxLat, c.latitude)
                minLon = min(minLon, c.longitude); maxLon = max(maxLon, c.longitude)
            }
            fitted = true
            // Marge pour que les foyers de bord ne collent pas au bandeau.
            let bounds = MLNCoordinateBounds(
                sw: CLLocationCoordinate2D(latitude: minLat, longitude: minLon),
                ne: CLLocationCoordinate2D(latitude: maxLat, longitude: maxLon))
            view.setVisibleCoordinateBounds(
                bounds,
                edgePadding: UIEdgeInsets(top: 130, left: 40, bottom: 190, right: 40),
                animated: false, completionHandler: nil)
        }

        /// Foyer touché : interrogation des couches de marqueurs sous le doigt.
        @objc func handleTap(_ g: UITapGestureRecognizer) {
            guard let view = g.view as? MLNMapView else { return }
            let point = g.location(in: view)
            let touchés = view.visibleFeatures(
                at: point, styleLayerIdentifiers: ["feux-coeur", "feux-halo"])
            for f in touchés {
                if let n = f.attribute(forKey: "feu") as? NSNumber {
                    onSelect(n.intValue)
                    return
                }
            }
        }
    }
}

/// Écran dédié « Carte des feux » à la manière de feuxdeforet.fr : orthophoto
/// IGN, contours de feu en rouge translucide et marqueurs flamme. Séparé de la
/// carte GPS pour ne pas l'alourdir. Les contours sont approximés à partir des
/// points chauds satellite (NASA FIRMS) regroupés par foyer.
struct FireMapScreen: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var service: FireService
    @AppStorage(TileSource.styleKey) private var styleURLString = ""

    @State private var perimeters: [FirePerimeter] = []
    @State private var style = URL(string: TileSource.defaultStyle)!
    /// Foyer dont la fiche est ouverte (tap sur le marqueur) — une seule à la
    /// fois, en bas de l'écran plutôt qu'en bulle : au dézoom les bulles de
    /// tous les foyers se superposaient en masse noire.
    @State private var selectedID: Int?
    /// Noms des foyers (« Feu de Saumos ») par position arrondie, géocodés à la
    /// demande au tap ; "" = géocodage en cours (évite les requêtes doublons).
    @State private var names: [String: String] = [:]
    /// Zone regardée : le plafond du serveur s'applique à ce qu'on demande.
    /// Sur le monde entier, quatre mille détections doivent couvrir la planète
    /// — un continent en reçoit alors une poignée. Demander la zone visible
    /// donne le même plafond pour cent fois moins de surface.
    @State private var bounds: MLNCoordinateBounds?
    /// Clé FIRMS de l'appareil, s'il en a une.
    @AppStorage("moncap.firmsKey") private var firmsKey = ""

    private var selected: FirePerimeter? {
        selectedID.flatMap { id in perimeters.first { $0.id == id } }
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

    var body: some View {
        ZStack(alignment: .top) {
            FireMapLibreView(
                styleURL: style, perimeters: perimeters, fires: service.fires,
                onRegionChange: { bounds = $0 }
            ) { id in
                withAnimation(.spring(duration: 0.25)) { selectedID = id }
                if let p = perimeters.first(where: { $0.id == id }) { loadName(of: p) }
            }
            .ignoresSafeArea()

            header

            if let p = selected {
                VStack {
                    Spacer()
                    fiche(p)
                }
            }
        }
        .task(id: styleURLString) {
            style = await TileSource.resolvedStyle(custom: styleURLString)
        }
        // Par case de 0,25° — environ vingt-cinq kilomètres — et non à chaque
        // image : un glissement de carte en produit des dizaines par seconde.
        .task(id: caseRegardee) {
            guard let b = bounds else { return }
            service.refresh(key: firmsKey, bbox: Self.bbox(b))
        }
        // Recalcule les contours à chaque rafraîchissement réussi — daté par
        // `lastUpdate`, car le simple compte de points peut rester identique
        // alors que les données ont changé (fenêtre glissante de 7 jours).
        .task(id: service.lastUpdate) {
            let fires = service.fires
            perimeters = await Task.detached { FireGeometry.perimeters(from: fires) }.value
            // Les identifiants de foyers changent au recalcul : referme la
            // fiche plutôt que de la laisser sauter sur un autre foyer.
            selectedID = nil
        }
    }

    /// Fiche du foyer touché, en bas de l'écran.
    private func fiche(_ p: FirePerimeter) -> some View {
        HStack(alignment: .top, spacing: 12) {
            FireUpdateBubble(
                name: name(of: p),
                updatedAt: p.updatedAt ?? service.lastUpdate,
                // Pas de repli sur `lastUpdate` ici : l'heure à laquelle on a
                // interrogé le serveur ne dit rien de la naissance du feu.
                // Inconnue, la ligne se tait.
                startedAt: p.startedAt)
            Button {
                withAnimation(.spring(duration: 0.25)) { selectedID = nil }
            } label: {
                Image(systemName: "xmark")
                    .font(.footnote.weight(.bold))
                    .foregroundStyle(.white.opacity(0.75))
                    .frame(width: 28, height: 28)
                    .background(Color.black.opacity(0.72), in: Circle())
            }
        }
        .padding(.horizontal)
        .padding(.bottom, 28)
    }

    /// Clé de cache d'un foyer : position arrondie (~1 km), stable d'un
    /// recalcul à l'autre contrairement à l'identifiant de cluster.
    private func nameKey(_ c: CLLocationCoordinate2D) -> String {
        "\(Int(c.latitude * 100))|\(Int(c.longitude * 100))"
    }

    /// Nom géocodé du foyer (« Feu de Saumos »), s'il est déjà connu.
    private func name(of p: FirePerimeter) -> String? {
        names[nameKey(p.center)].flatMap { $0.isEmpty ? nil : $0 }
    }

    /// Géocodage inverse du centre du foyer, à la demande (un seul appel par
    /// foyer, au premier tap) : nomme le feu d'après sa commune, comme le site.
    private func loadName(of p: FirePerimeter) {
        let key = nameKey(p.center)
        guard names[key] == nil else { return }
        names[key] = ""  // requête en cours
        let center = p.center
        Task {
            let loc = CLLocation(latitude: center.latitude, longitude: center.longitude)
            let mark = try? await CLGeocoder().reverseGeocodeLocation(loc).first
            if let city = mark?.locality ?? mark?.subLocality ?? mark?.subAdministrativeArea {
                names[key] = "Feu de \(city)"
            } else {
                // Échec (réseau, quota) : oublier la sentinelle « en cours »,
                // sinon plus aucune tentative n'était possible pour ce foyer.
                names.removeValue(forKey: key)
            }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.headline)
                    .foregroundStyle(.primary)
                    .frame(width: 40, height: 40)
                    .background(.regularMaterial, in: Circle())
            }
            VStack(alignment: .leading, spacing: 1) {
                // Le compteur porte sur les **détections**, pas sur les
                // contours. Un grand feu produit des dizaines de pixels VIIRS :
                // annoncer « 1467 feux » pour 1467 détections surestimait
                // largement. Et les contours ne se forment qu'à l'échelle d'un
                // foyer, si bien que le compteur tombait à zéro dès qu'on
                // dézoomait, alors que la carte montrait toujours des points.
                // Deux littéraux entiers, et non un pluriel interpolé : « point
                // \(s) chaud\(s) » donnerait la clé « %lld point%@ chaud%@ », que
                // le catalogue ne peut pas traduire correctement.
                Text(
                    service.fires.isEmpty
                        ? "Carte des feux en cours"
                        : service.fires.count > 1
                            ? "\(service.fires.count) points chauds"
                            : "\(service.fires.count) point chaud"
                )
                .font(.headline)
                Text(
                    service.fires.isEmpty
                        ? "Aucun foyer détecté"
                        : "détections satellite · \(OrthoSource.attribution)"
                )
                .font(.caption2)
                .foregroundStyle(.secondary)
            }
            // Une ligne par texte, réduite au besoin : en grande taille de
            // police le bandeau enflait jusqu'à couvrir la moitié de l'écran.
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.regularMaterial, in: Capsule())
            Spacer()
        }
        .padding()
    }
}

/// Infobulle sombre sous le marqueur, identique à celle de feuxdeforet.fr :
/// nom du feu (géocodé), « Contour satellite approximatif », puis « Mise à
/// jour il y a X h » en italique.
///
/// Plus d'âge du foyer : la carte ne montre que les feux actifs des
/// dernières vingt-quatre heures, et une date de création n'aurait plus de
/// fenêtre d'observation d'où la tirer.
struct FireUpdateBubble: View {
    let name: String?
    let updatedAt: Date?
    /// Première détection du foyer. Nil quand elle est inconnue — la ligne
    /// disparaît alors, plutôt que d'annoncer un âge faux.
    let startedAt: Date?

    var body: some View {
        VStack(spacing: 2) {
            if let name {
                Text(name)
                    .font(.footnote.weight(.bold))
                    .foregroundStyle(.white)
            }
            Text("Contour satellite approximatif")
                .font(name == nil ? .footnote.weight(.semibold) : .caption)
                .foregroundStyle(name == nil ? .white : .white.opacity(0.8))
            if let depuis = depuisText {
                Text(depuis)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white.opacity(0.85))
            }
            if let ago = agoText {
                Text(ago)
                    .font(.caption.italic())
                    .foregroundStyle(.white.opacity(0.65))
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(Color.black.opacity(0.72), in: RoundedRectangle(cornerRadius: 12))
        .fixedSize()
    }

    /// « Créé il y a 3 j », omis si la première détection est inconnue.
    ///
    /// Sept jours est le fond de la fenêtre : au-delà, on ne sait pas si le
    /// feu est né le septième jour ou le trentième. On le dit — « depuis plus
    /// d'une semaine » — au lieu d'annoncer un âge qu'on ne connaît pas.
    private var depuisText: String? {
        guard let startedAt else { return nil }
        let heures = Date().timeIntervalSince(startedAt) / 3600
        guard heures >= 0 else { return nil }
        if heures < 24 {
            let h = Int(heures)
            return h < 1
                ? String(localized: "Créé il y a moins d'1 h")
                : String(format: String(localized: "Créé il y a %lld h"), h)
        }
        let jours = Int(heures / 24)
        return jours >= 7
            ? String(localized: "Actif depuis plus d'une semaine")
            : String(format: String(localized: "Créé il y a %lld j"), jours)
    }

    /// « Mise à jour il y a 3 h », omis si la date est inconnue.
    private var agoText: String? {
        guard let updatedAt else { return nil }
        let hours = Int(Date().timeIntervalSince(updatedAt) / 3600)
        return hours < 1
            ? String(localized: "Mise à jour il y a moins d'1 h")
            : String(format: String(localized: "Mise à jour il y a %lld h"), hours)
    }
}

/// Géométrie des contours de feu.
///
/// Le contour d'un foyer est l'**union des empreintes de ses détections**
/// satellite : chaque point chaud VIIRS (pixel de 375 m) est élargi en un
/// disque, et l'on trace la frontière de leur réunion. On obtient une forme
/// organique continue qui épouse la zone brûlée — comme les contours de
/// feuxdeforet.fr — au lieu des enveloppes anguleuses (triangles, cercles
/// isolés) que donnait le calcul d'enveloppe convexe/concave sur des points
/// épars.
///
/// Mise en œuvre : champ de distance sur une grille locale, extraction de
/// l'iso-ligne par *marching squares* orienté (l'intérieur reste à gauche, donc
/// les segments se chaînent sans ambiguïté en anneaux fermés), puis lissage de
/// Chaikin.
enum FireGeometry {
    /// Rayon de l'empreinte attribuée à une détection. Un pixel VIIRS fait
    /// 375 m de côté ; on élargit un peu pour couvrir le pourtour réel du feu
    /// et souder les détections voisines d'un même foyer.
    private static let footprintMeters = 800.0
    /// Résolution de la grille de calcul.
    private static let cellMeters = 150.0
    /// Distance de regroupement en foyers distincts.
    private static let clusterMeters = 12_000.0
    /// Sous ce seuil, un contour est du bruit et n'est pas affiché.
    private static let minAreaM2 = 300_000.0
    /// Plafond de cellules par foyer : au-delà, la grille est élargie plutôt
    /// que de faire exploser le calcul sur un foyer très étendu.
    private static let maxCells = 250_000
    /// Un foyer sans détection depuis ce délai est considéré éteint (l'API
    /// renvoie 7 jours d'historique pour dater les débuts de feux, mais seuls
    /// les foyers encore actifs sont affichés — « feux en cours »).
    private static let activeWindow: TimeInterval = 36 * 3600

    private static let metersPerDegree = 111_320.0

    /// Construit les contours des foyers **encore actifs**. La forme est bâtie
    /// sur *tout* l'historique du foyer (la zone brûlée s'accumule), tandis que
    /// l'activité récente décide seulement de l'affichage.
    static func perimeters(from fires: [Fire]) -> [FirePerimeter] {
        guard !fires.isEmpty else { return [] }
        var out: [FirePerimeter] = []
        for group in cluster(fires, thresholdMeters: clusterMeters) {
            let latest = group.compactMap { $0.detectedAt }.max()
            if let latest, Date().timeIntervalSince(latest) > activeWindow { continue }
            // Un groupe qui s'étend sur plus de cent cinquante kilomètres
            // vient d'un enchaînement de proche en proche, pas d'un foyer : la
            // grille se dégraderait jusqu'à des cellules plus grandes que
            // l'empreinte d'un feu, et ne rendrait aucun contour. Les points
            // de détection, eux, restent affichés.
            guard etendueRaisonnable(group) else { continue }
            let shapes = rings(of: group)
            guard !shapes.isEmpty else { continue }
            let centers = shapes.map { centroid($0) }
            // Un foyer peut se scinder en plusieurs poches : chaque détection
            // est rattachée à la poche la plus proche, pour dater chaque
            // contour avec ses propres détections.
            var buckets = [[Fire]](repeating: [], count: shapes.count)
            for f in group {
                var best = 0
                var bestDistance = Double.greatestFiniteMagnitude
                for (i, c) in centers.enumerated() {
                    let d = squaredMeters(f.coordinate, c)
                    if d < bestDistance {
                        bestDistance = d
                        best = i
                    }
                }
                buckets[best].append(f)
            }
            for (i, ring) in shapes.enumerated() {
                let own = buckets[i].isEmpty ? group : buckets[i]
                out.append(
                    FirePerimeter(
                        id: out.count, polygon: ring, center: centers[i],
                        updatedAt: own.compactMap { $0.detectedAt }.max(),
                        // Le début, lui, se prend sur **tout** le groupe et
                        // non sur la seule poche : un foyer qui s'est scindé
                        // n'est pas né au moment où il s'est scindé.
                        startedAt: group.compactMap { $0.firstSeen }.min()))
            }
        }
        return out
    }

    /// Le groupe tient-il dans une boîte de cent cinquante kilomètres ?
    private static func etendueRaisonnable(_ group: [Fire]) -> Bool {
        guard let first = group.first?.coordinate else { return false }
        let cosLat = cos(first.latitude * .pi / 180)
        var minX = 0.0, maxX = 0.0, minY = 0.0, maxY = 0.0
        for f in group {
            let x = (f.coordinate.longitude - first.longitude) * metersPerDegree * cosLat
            let y = (f.coordinate.latitude - first.latitude) * metersPerDegree
            minX = min(minX, x); maxX = max(maxX, x)
            minY = min(minY, y); maxY = max(maxY, y)
        }
        return (maxX - minX) <= 150_000 && (maxY - minY) <= 150_000
    }

    private static func squaredMeters(
        _ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D
    ) -> Double {
        let cosLat = cos(a.latitude * .pi / 180)
        let dx = (b.longitude - a.longitude) * metersPerDegree * cosLat
        let dy = (b.latitude - a.latitude) * metersPerDegree
        return dx * dx + dy * dy
    }

    /// Regroupe les feux : un point rejoint un groupe s'il est à moins de
    /// `thresholdMeters` de l'un de ses membres. Distance équirectangulaire
    /// (planaire) : à ces échelles l'écart avec la distance géodésique est
    /// négligeable, et c'est bien plus rapide que `CLLocation.distance`.
    private static func cluster(_ fires: [Fire], thresholdMeters: Double) -> [[Fire]] {
        let thresholdSq = thresholdMeters * thresholdMeters
        var groups: [[Fire]] = []
        for f in fires {
            if let idx = groups.firstIndex(where: { group in
                group.contains { squaredMeters(f.coordinate, $0.coordinate) < thresholdSq }
            }) {
                groups[idx].append(f)
            } else {
                groups.append([f])
            }
        }
        return groups
    }

    private static func centroid(_ pts: [CLLocationCoordinate2D]) -> CLLocationCoordinate2D {
        let n = Double(pts.count)
        return CLLocationCoordinate2D(
            latitude: pts.reduce(0) { $0 + $1.latitude } / n,
            longitude: pts.reduce(0) { $0 + $1.longitude } / n)
    }

    // MARK: - Contour = frontière de l'union des empreintes

    /// Anneaux fermés délimitant l'union des empreintes d'un foyer.
    private static func rings(of group: [Fire]) -> [[CLLocationCoordinate2D]] {
        guard let first = group.first?.coordinate else { return [] }
        let cosLat = cos(first.latitude * .pi / 180)
        // Projection locale en mètres, ancrée sur la première détection.
        let pts: [(x: Double, y: Double)] = group.map {
            (
                x: ($0.coordinate.longitude - first.longitude) * metersPerDegree * cosLat,
                y: ($0.coordinate.latitude - first.latitude) * metersPerDegree
            )
        }
        // Marge >= empreinte : le champ est négatif au bord, donc les anneaux
        // sont toujours fermés (sinon le contour sortirait de la grille).
        var cell = cellMeters
        let margin = footprintMeters + 3 * cell
        let xs = pts.map { $0.x }
        let ys = pts.map { $0.y }
        let minX = xs.min()! - margin
        let maxX = xs.max()! + margin
        let minY = ys.min()! - margin
        let maxY = ys.max()! + margin
        // Foyer très étendu : on dégrade la résolution plutôt que d'abandonner.
        while Int((maxX - minX) / cell + 2) * Int((maxY - minY) / cell + 2) > maxCells {
            cell *= 2
        }
        let nx = Int((maxX - minX) / cell) + 2
        let ny = Int((maxY - minY) / cell) + 2
        guard nx > 2, ny > 2 else { return [] }

        // Champ = empreinte − distance à la détection la plus proche.
        // L'iso-ligne 0 est donc exactement la frontière de l'union des disques.
        var field = [Double](repeating: -Double.greatestFiniteMagnitude, count: nx * ny)
        let reach = footprintMeters + cell
        for p in pts {
            let i0 = max(0, Int((p.x - reach - minX) / cell))
            let i1 = min(nx - 1, Int((p.x + reach - minX) / cell) + 1)
            let j0 = max(0, Int((p.y - reach - minY) / cell))
            let j1 = min(ny - 1, Int((p.y + reach - minY) / cell) + 1)
            guard i0 <= i1, j0 <= j1 else { continue }
            for j in j0...j1 {
                let dy = (minY + Double(j) * cell) - p.y
                for i in i0...i1 {
                    let dx = (minX + Double(i) * cell) - p.x
                    let v = footprintMeters - (dx * dx + dy * dy).squareRoot()
                    let k = j * nx + i
                    if v > field[k] { field[k] = v }
                }
            }
        }

        return isoRings(field, nx: nx, ny: ny)
            .filter { area($0) * cell * cell >= minAreaM2 }
            .map { ring in
                smooth(ring, iterations: 2).map { g in
                    CLLocationCoordinate2D(
                        latitude: first.latitude + (minY + g.y * cell) / metersPerDegree,
                        longitude: first.longitude
                            + (minX + g.x * cell) / (metersPerDegree * cosLat))
                }
            }
    }

    /// Aire (en cellules²) d'un anneau, par la formule du lacet.
    private static func area(_ ring: [(x: Double, y: Double)]) -> Double {
        var a = 0.0
        for i in 0..<ring.count {
            let p = ring[i]
            let q = ring[(i + 1) % ring.count]
            a += p.x * q.y - q.x * p.y
        }
        return abs(a) / 2
    }

    /// Clé d'appariement des extrémités de segments (grille au 1/100 000).
    private struct Key: Hashable {
        let x: Int
        let y: Int
        init(_ p: (x: Double, y: Double)) {
            x = Int((p.x * 100_000).rounded())
            y = Int((p.y * 100_000).rounded())
        }
    }

    /// *Marching squares* orienté : extrait l'iso-ligne `field == 0` sous forme
    /// d'anneaux fermés, en coordonnées de grille. Chaque segment est dirigé de
    /// façon à garder l'intérieur à sa gauche : chaque extrémité a donc une
    /// seule arête sortante, et le chaînage est déterministe.
    private static func isoRings(_ field: [Double], nx: Int, ny: Int)
        -> [[(x: Double, y: Double)]]
    {
        // Arêtes de la cellule : 0 = bas, 1 = droite, 2 = haut, 3 = gauche.
        // Table (depuis, vers) par configuration de coins
        // (bit 1 = bas-gauche, 2 = bas-droite, 4 = haut-droite, 8 = haut-gauche).
        let table: [[(Int, Int)]] = [
            [], [(0, 3)], [(1, 0)], [(1, 3)],
            [(2, 1)], [(0, 3), (2, 1)], [(2, 0)], [(2, 3)],
            [(3, 2)], [(0, 2)], [(1, 0), (3, 2)], [(1, 2)],
            [(3, 1)], [(0, 1)], [(3, 0)], [],
        ]
        var next: [Key: Key] = [:]
        var point: [Key: (x: Double, y: Double)] = [:]

        func cut(
            _ a: (x: Double, y: Double), _ va: Double,
            _ b: (x: Double, y: Double), _ vb: Double
        ) -> (x: Double, y: Double) {
            let d = vb - va
            let t = abs(d) < 1e-12 ? 0.5 : min(1, max(0, (0 - va) / d))
            return (x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t)
        }

        for j in 0..<(ny - 1) {
            for i in 0..<(nx - 1) {
                let vBL = field[j * nx + i]
                let vBR = field[j * nx + i + 1]
                let vTR = field[(j + 1) * nx + i + 1]
                let vTL = field[(j + 1) * nx + i]
                var code = 0
                if vBL > 0 { code |= 1 }
                if vBR > 0 { code |= 2 }
                if vTR > 0 { code |= 4 }
                if vTL > 0 { code |= 8 }
                let segs = table[code]
                if segs.isEmpty { continue }
                let bl = (x: Double(i), y: Double(j))
                let br = (x: Double(i + 1), y: Double(j))
                let tr = (x: Double(i + 1), y: Double(j + 1))
                let tl = (x: Double(i), y: Double(j + 1))
                let edge: [(x: Double, y: Double)] = [
                    cut(bl, vBL, br, vBR),  // bas
                    cut(br, vBR, tr, vTR),  // droite
                    cut(tl, vTL, tr, vTR),  // haut
                    cut(bl, vBL, tl, vTL),  // gauche
                ]
                for (from, to) in segs {
                    let a = edge[from]
                    let b = edge[to]
                    let ka = Key(a)
                    let kb = Key(b)
                    if ka == kb { continue }
                    point[ka] = a
                    point[kb] = b
                    next[ka] = kb
                }
            }
        }

        var rings: [[(x: Double, y: Double)]] = []
        while let start = next.keys.first {
            var ring: [(x: Double, y: Double)] = []
            var cur = start
            while let step = next.removeValue(forKey: cur) {
                if let p = point[cur] { ring.append(p) }
                cur = step
                if cur == start { break }
            }
            if ring.count >= 4 { rings.append(ring) }
        }
        return rings
    }

    /// Lissage par coupe-coins de Chaikin : arrondit un polygone fermé (les
    /// marches d'escalier de la grille deviennent une courbe organique).
    private static func smooth(_ ring: [(x: Double, y: Double)], iterations: Int)
        -> [(x: Double, y: Double)]
    {
        guard ring.count >= 3 else { return ring }
        var pts = ring
        for _ in 0..<iterations {
            var out: [(x: Double, y: Double)] = []
            out.reserveCapacity(pts.count * 2)
            for i in 0..<pts.count {
                let a = pts[i]
                let b = pts[(i + 1) % pts.count]
                out.append((x: a.x * 0.75 + b.x * 0.25, y: a.y * 0.75 + b.y * 0.25))
                out.append((x: a.x * 0.25 + b.x * 0.75, y: a.y * 0.25 + b.y * 0.75))
            }
            pts = out
        }
        return pts
    }
}
