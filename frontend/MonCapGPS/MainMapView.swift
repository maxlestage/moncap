import CoreLocation
import MapKit
import MapLibre
import SwiftUI
import UIKit

/// Carte principale, rendue par MapLibre.
///
/// L'app n'utilise plus Apple Plans : le même moteur sert la carte de conduite
/// et les cartes hors ligne, avec le même fond OpenStreetMap. C'est ce qui rend
/// possible de rouler sans réseau — Apple Plans ne permet pas d'enregistrer
/// ses tuiles.
///
/// Le reste de MapKit continue de servir : recherche de lieux, géocodage et
/// calcul d'itinéraires n'ont pas besoin d'une vue carte.

// MARK: - Caméra

/// Position de caméra, en remplacement de `MapCameraPosition` d'Apple.
///
/// Chaque instance porte une identité propre : réassigner une caméra
/// identique doit tout de même recadrer la vue (recentrage après un
/// déplacement à la main, par exemple).
struct MapCam: Equatable {
    let id = UUID()
    var center: CLLocationCoordinate2D
    /// Niveau de zoom MapLibre.
    ///
    /// Exprimé ainsi, et non en mètres : convertir une distance en altitude
    /// dépend de la taille de la vue et du moteur, et donnait un cadrage
    /// imprévisible. Un niveau de zoom, lui, veut dire la même chose partout.
    var zoom: Double = 15
    var heading: Double = 0
    var pitch: Double = 0

    static func == (a: MapCam, b: MapCam) -> Bool { a.id == b.id }
}

extension MapCam {
    /// Caméra couvrant une région, en remplacement de `.region(...)`.
    ///
    /// Déclarée dans une extension à dessein : un initialiseur écrit dans le
    /// corps de la structure supprimerait l'initialiseur membre à membre.
    init(region: MKCoordinateRegion) {
        // Chaque niveau de zoom couvre deux fois moins de degrés que le
        // précédent. Au zoom 0 le tour du monde fait 512 **points** chez
        // MapLibre : un écran d'environ 390 points en couvre donc 360 × 390 /
        // 512 ≈ 274 degrés. Compter en pixels donnait un niveau de trop, et
        // les itinéraires est-ouest sortaient de l'écran.
        let span = max(region.span.longitudeDelta, region.span.latitudeDelta * 1.3, 0.0005)
        self.init(center: region.center, zoom: max(2, min(17, log2(274 / span))),
                  heading: 0, pitch: 0)
    }
}

// MARK: - Contenu

/// Apparence d'une épingle. `Hashable` : sert de clé au cache d'images, deux
/// épingles de même allure partageant le même dessin.
enum PinLook: Hashable {
    /// Avatar de véhicule, ou pictogramme piéton/vélo.
    case avatar(String, CGFloat)
    /// Pastille blanche avec un emoji (lieux d'une catégorie).
    case emoji(String, CGFloat)
    /// Signalement : emoji et, s'il y a lieu, son nombre de confirmations.
    case report(String, Int)
    /// Bulle d'itinéraire : durée, coût éventuel, couleur, mise en avant.
    case bubble(String, String?, String, Bool)
    /// Repère simple, avec un symbole système.
    case marker(String, String)
    /// Foyer d'incendie : disque rouge cerné de son halo, flamme au centre.
    case flame
}

/// Épingle posée sur la carte.
struct MapPin: Identifiable {
    let id: String
    let coordinate: CLLocationCoordinate2D
    let title: String
    let look: PinLook
}

/// Tracé posé sur la carte.
struct MapLine: Identifiable {
    let id: String
    let coordinates: [CLLocationCoordinate2D]
    /// Couleur en hexadécimal (`#RRGGBB`), pour rester comparable d'une
    /// évaluation de vue à l'autre.
    let color: String
    let width: Double
    var opacity: Double = 1
    var dashed: Bool = false

    /// Résumé de l'apparence : reconstruire les couches n'a de sens que si
    /// quelque chose a changé.
    var signature: String {
        // Les extrémités et le milieu entrent dans le compte : un itinéraire
        // recalculé garde souvent le même nombre de points, et sans eux le
        // tracé affiché restait celui d'avant.
        let a = coordinates.first
        let b = coordinates[coordinates.count / 2 % max(1, coordinates.count)]
        let c = coordinates.last
        func k(_ p: CLLocationCoordinate2D?) -> String {
            guard let p else { return "-" }
            return "\(Int(p.latitude * 1e5)),\(Int(p.longitude * 1e5))"
        }
        return "\(id)|\(coordinates.count)|\(k(a))|\(k(b))|\(k(c))"
            + "|\(color)|\(width)|\(opacity)|\(dashed)"
    }
}

// MARK: - Vue

struct MainMapView: UIViewRepresentable {
    let styleURL: URL
    /// Caméra à appliquer. `nil` = laisser la carte où elle est.
    let cam: MapCam?
    let pins: [MapPin]
    let lines: [MapLine]
    /// Relief des bâtiments : réservé à la vue 3D, coûteux à dessiner.
    let buildings3D: Bool
    let onPinTap: (String) -> Void
    /// Un lieu du fond de carte (restaurant, hôtel…) a été touché.
    let onPlaceTap: (CLLocationCoordinate2D, String) -> Void
    /// La carte a été touchée ailleurs.
    let onMapTap: (CLLocationCoordinate2D) -> Void
    let onRegionChange: (MKCoordinateRegion) -> Void
    /// Geste manuel : sert à débrayer le suivi automatique.
    let onUserGesture: () -> Void

    func makeUIView(context: Context) -> SizedMapView {
        let view = SizedMapView(frame: .zero, styleURL: styleURL)
        context.coordinator.appliedStyle = styleURL
        // Filet de sécurité : la toute première caméra arrive parfois avant que
        // la carte n'ait sa place à l'écran, et à l'arrêt rien ne la réémet.
        view.onLayout = { [weak view, weak coordinator = context.coordinator] in
            guard let view, let coordinator else { return }
            coordinator.applyPendingCam(to: view)
        }
        view.delegate = context.coordinator
        // Pas de pastille bleue : la position est marquée par l'avatar,
        // posé comme épingle par la vue appelante.
        view.showsUserLocation = false
        view.attributionButtonPosition = .bottomRight
        // La boussole d'Apple avait son équivalent ici ; le logo doit rester
        // visible, la licence des données l'exige.
        view.compassView.isHidden = false
        view.logoView.isHidden = false

        let tap = UITapGestureRecognizer(
            target: context.coordinator, action: #selector(Coordinator.handleTap(_:)))
        tap.delegate = context.coordinator
        // Sans cela, le premier appui d'un double-tap de zoom déclenchait déjà
        // une action — un calcul d'itinéraire, par exemple. MapLibre impose la
        // même contrainte à son propre appui simple.
        for existing in view.gestureRecognizers ?? []
        where (existing as? UITapGestureRecognizer)?.numberOfTapsRequired == 2 {
            tap.require(toFail: existing)
        }
        view.addGestureRecognizer(tap)

        let pan = UIPanGestureRecognizer(
            target: context.coordinator, action: #selector(Coordinator.handleGesture(_:)))
        pan.delegate = context.coordinator
        view.addGestureRecognizer(pan)
        let pinch = UIPinchGestureRecognizer(
            target: context.coordinator, action: #selector(Coordinator.handleGesture(_:)))
        pinch.delegate = context.coordinator
        view.addGestureRecognizer(pinch)

        if let cam {
            context.coordinator.appliedCam = cam
            context.coordinator.apply(cam, to: view)
        }
        return view
    }

    func updateUIView(_ view: SizedMapView, context: Context) {
        let c = context.coordinator
        c.onPinTap = onPinTap
        c.onPlaceTap = onPlaceTap
        c.onMapTap = onMapTap
        c.onRegionChange = onRegionChange
        c.onUserGesture = onUserGesture

        // Changer de style recharge tout : les couches et épingles seront
        // reposées à la fin du chargement.
        //
        // La comparaison porte sur ce qu'on a demandé, jamais sur ce que la
        // vue renvoie : pendant le chargement `styleURL` ne vaut pas encore la
        // valeur posée, et comme cette vue est réévaluée en continu (GPS,
        // vitesse, météo), le style repartait de zéro à chaque fois sans
        // jamais aboutir — la carte restait blanche.
        if c.appliedStyle != styleURL { c.changeStyle(to: styleURL, on: view) }

        if let cam, cam != c.appliedCam {
            c.appliedCam = cam
            c.apply(cam, to: view)
        }
        c.sync(pins: pins, on: view)
        c.sync(lines: lines, on: view)
        c.setBuildings3D(buildings3D, on: view)
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    // MARK: Coordinateur

    final class Coordinator: NSObject, MLNMapViewDelegate, UIGestureRecognizerDelegate {
        var onPinTap: (String) -> Void = { _ in }
        var onPlaceTap: (CLLocationCoordinate2D, String) -> Void = { _, _ in }
        var onMapTap: (CLLocationCoordinate2D) -> Void = { _ in }
        var onRegionChange: (MKCoordinateRegion) -> Void = { _ in }
        var onUserGesture: () -> Void = {}

        var appliedCam: MapCam?
        /// Style demandé à la vue. Fait foi : la vue, elle, ne le renvoie
        /// qu'une fois le chargement terminé.
        var appliedStyle: URL?
        /// La vue a-t-elle déjà été mesurée une fois ?
        private var laidOut = false
        /// Épingles en place, par identifiant.
        private var placed: [String: PinAnnotation] = [:]
        /// Signature des tracés déjà dessinés, pour ne rien refaire pour rien.
        private var lineSignatures: [String] = []
        /// Identifiants des tracés en place, pour savoir quoi retirer.
        private var lineIDs: [String] = []
        private var styleReady = false
        private var pendingPins: [MapPin] = []
        private var pendingLines: [MapLine] = []
        private var buildings3D = false

        /// Couches de lieux du fond OpenMapTiles : ce sont elles qu'on
        /// interroge quand l'utilisateur touche la carte.
        private static let placeLayers: Set<String> = [
            "poi_r20", "poi_r7", "poi_r1", "poi_transit", "airport",
        ]

        /// Bascule vers un autre fond de carte.
        func changeStyle(to url: URL, on view: MLNMapView) {
            appliedStyle = url
            styleReady = false
            // Les épingles appartiennent à la vue, pas au style : les oublier
            // sans les retirer les aurait laissées en place, puis dupliquées
            // au rechargement.
            view.removeAnnotations(Array(placed.values))
            placed = [:]
            lineSignatures = []
            lineIDs = []
            view.styleURL = url
        }

        // MARK: Caméra

        func apply(_ cam: MapCam, to view: MLNMapView) {
            // L'inclinaison d'abord, sans animation : elle ne dépend ni du
            // centre ni du zoom, et `setCenter` la laisse intacte.
            let pitch = CGFloat(max(0, min(60, cam.pitch)))
            if abs(view.camera.pitch - pitch) > 1 {
                let tilted = view.camera
                tilted.pitch = pitch
                view.setCamera(tilted, animated: false)
            }
            // Puis centre, zoom et cap en une fois. Aucune altitude n'entre en
            // jeu : ce chemin ne dépend pas de la taille de la vue.
            view.setCenter(
                cam.center, zoomLevel: max(2, min(19, cam.zoom)),
                direction: cam.heading, animated: true)
        }

        /// Repose la dernière caméra une fois la vue mesurée : la toute
        /// première arrive parfois avant que la carte n'ait sa place à l'écran.
        func applyPendingCam(to view: MLNMapView) {
            guard !laidOut, view.bounds.height > 10 else { return }
            // Marqué dès la première mise en page utile, même sans caméra à
            // poser : sinon une mise en page ultérieure (rotation, clavier)
            // rejouerait une caméra périmée et ramènerait la carte en arrière.
            laidOut = true
            if let cam = appliedCam { apply(cam, to: view) }
        }

        // MARK: Épingles

        func sync(pins: [MapPin], on view: MLNMapView) {
            guard styleReady else {
                pendingPins = pins
                return
            }
            var seen = Set<String>()
            for pin in pins {
                seen.insert(pin.id)
                if let existing = placed[pin.id] {
                    if existing.look != pin.look {
                        view.removeAnnotation(existing)
                        placed[pin.id] = add(pin, to: view)
                    } else if !same(existing.coordinate, pin.coordinate) {
                        // Déplacer plutôt que recréer : évite un clignotement.
                        existing.coordinate = pin.coordinate
                    }
                } else {
                    placed[pin.id] = add(pin, to: view)
                }
            }
            for (id, annotation) in placed where !seen.contains(id) {
                view.removeAnnotation(annotation)
                placed.removeValue(forKey: id)
            }
        }

        private func add(_ pin: MapPin, to view: MLNMapView) -> PinAnnotation {
            let a = PinAnnotation()
            a.coordinate = pin.coordinate
            a.title = pin.title
            a.pinID = pin.id
            a.look = pin.look
            view.addAnnotation(a)
            return a
        }

        private func same(_ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D) -> Bool {
            abs(a.latitude - b.latitude) < 1e-7 && abs(a.longitude - b.longitude) < 1e-7
        }

        // MARK: Tracés

        func sync(lines: [MapLine], on view: MLNMapView) {
            guard styleReady, let style = view.style else {
                pendingLines = lines
                return
            }
            let signatures = lines.map(\.signature)
            guard signatures != lineSignatures else { return }

            // Retirer les anciennes couches avant d'ajouter : l'ordre de la
            // liste fait l'ordre d'empilement (le tracé retenu passe devant).
            for id in lineIDs {
                if let layer = style.layer(withIdentifier: "trace-\(id)") {
                    style.removeLayer(layer)
                }
                if let source = style.source(withIdentifier: "trace-src-\(id)") {
                    style.removeSource(source)
                }
            }
            lineSignatures = signatures
            lineIDs = lines.map(\.id)

            for line in lines where line.coordinates.count >= 2 {
                var coords = line.coordinates
                let feature = MLNPolylineFeature(coordinates: &coords, count: UInt(coords.count))
                let source = MLNShapeSource(
                    identifier: "trace-src-\(line.id)", shape: feature, options: nil)
                style.addSource(source)
                let layer = MLNLineStyleLayer(identifier: "trace-\(line.id)", source: source)
                layer.lineColor = NSExpression(forConstantValue: UIColor(hex: line.color))
                layer.lineWidth = NSExpression(forConstantValue: line.width)
                layer.lineOpacity = NSExpression(forConstantValue: line.opacity)
                layer.lineCap = NSExpression(forConstantValue: "round")
                layer.lineJoin = NSExpression(forConstantValue: "round")
                if line.dashed {
                    layer.lineDashPattern = NSExpression(forConstantValue: [2, 1.5])
                }
                style.addLayer(layer)
            }
        }

        func setBuildings3D(_ on: Bool, on view: MLNMapView) {
            buildings3D = on
            // Rien à masquer : au-delà du zoom 14, `building-3d` est la seule
            // couche de bâti du style (`building` s'arrête là). La cacher en
            // 2D effaçait les bâtiments sur toute la plage de conduite ; à
            // plat, une extrusion se lit simplement comme une empreinte.
        }

        // MARK: Délégué

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            styleReady = true
            style.layer(withIdentifier: "building-3d")?.isVisible = buildings3D
            if !pendingPins.isEmpty {
                sync(pins: pendingPins, on: mapView)
                pendingPins = []
            }
            if !pendingLines.isEmpty {
                sync(lines: pendingLines, on: mapView)
                pendingLines = []
            }
        }

        func mapView(_ mapView: MLNMapView, imageFor annotation: MLNAnnotation) -> MLNAnnotationImage? {
            guard let pin = annotation as? PinAnnotation, let look = pin.look else { return nil }
            let key = String(look.hashValue)
            if let cached = mapView.dequeueReusableAnnotationImage(withIdentifier: key) {
                return cached
            }
            return MLNAnnotationImage(image: PinRenderer.image(for: look), reuseIdentifier: key)
        }

        func mapView(_ mapView: MLNMapView, annotationCanShowCallout annotation: MLNAnnotation) -> Bool {
            // Les bulles d'Apple sont remplacées par nos propres panneaux.
            false
        }

        func mapView(_ mapView: MLNMapView, didSelect annotation: MLNAnnotation) {
            mapView.deselectAnnotation(annotation, animated: false)
            guard let pin = annotation as? PinAnnotation else { return }
            onPinTap(pin.pinID)
        }

        func mapView(_ mapView: MLNMapView, regionDidChangeAnimated animated: Bool) {
            onRegionChange(Coordinator.region(of: mapView.visibleCoordinateBounds))
        }

        /// Zone visible, convertie dans le type déjà employé par l'app.
        static func region(of bounds: MLNCoordinateBounds) -> MKCoordinateRegion {
            let ne = bounds.ne, sw = bounds.sw
            let center = CLLocationCoordinate2D(
                latitude: (ne.latitude + sw.latitude) / 2,
                longitude: (ne.longitude + sw.longitude) / 2)
            let span = MKCoordinateSpan(
                latitudeDelta: abs(ne.latitude - sw.latitude),
                longitudeDelta: abs(ne.longitude - sw.longitude))
            return MKCoordinateRegion(center: center, span: span)
        }

        // MARK: Gestes

        @objc func handleTap(_ g: UITapGestureRecognizer) {
            guard let view = g.view as? MLNMapView else { return }
            let point = g.location(in: view)

            // Une épingle sous le doigt est déjà traitée par MapLibre, qui
            // appelle `didSelect`. Sans ce test, l'appui déclenchait les deux
            // chemins : sélection de l'épingle ET action « lieu du fond ».
            if annotation(near: point, in: view) != nil { return }

            let coord = view.convert(point, toCoordinateFrom: view)

            // Un lieu du fond de carte l'emporte : c'est ce que l'utilisateur
            // visait s'il a touché une icône.
            let features = view.visibleFeatures(
                at: point, styleLayerIdentifiers: Coordinator.placeLayers)
            if let place = features.first(where: { $0.attribute(forKey: "name") != nil }),
                let name = place.attribute(forKey: "name") as? String
            {
                onPlaceTap(place.coordinate, name)
                return
            }
            onMapTap(coord)
        }

        /// Épingle posée sous le point touché, à portée du doigt.
        private func annotation(near point: CGPoint, in view: MLNMapView) -> PinAnnotation? {
            let reach: CGFloat = 30
            for pin in placed.values {
                let p = view.convert(pin.coordinate, toPointTo: view)
                if abs(p.x - point.x) <= reach && abs(p.y - point.y) <= reach { return pin }
            }
            return nil
        }

        @objc func handleGesture(_ g: UIGestureRecognizer) {
            if g.state == .began || g.state == .changed { onUserGesture() }
        }

        func gestureRecognizer(
            _ g: UIGestureRecognizer,
            shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
        ) -> Bool { true }
    }
}

/// Carte MapLibre qui signale sa mise en page.
///
/// `UIViewRepresentable` n'expose aucun moment « la vue a sa taille » ; sans
/// ce signal, une caméra reçue trop tôt resterait sans effet.
final class SizedMapView: MLNMapView {
    var onLayout: (() -> Void)?

    override func layoutSubviews() {
        super.layoutSubviews()
        onLayout?()
    }
}

/// Épingle MapLibre enrichie de ce qu'il faut pour la dessiner et la reconnaître.
final class PinAnnotation: MLNPointAnnotation {
    var pinID = ""
    var look: PinLook?
}

// MARK: - Dessin des épingles

/// Fabrique les images d'épingles.
///
/// Les épingles sont dessinées une fois puis mises en cache par MapLibre : bien
/// moins coûteux que d'héberger une vue SwiftUI par épingle, ce qui comptait dès
/// qu'une catégorie de lieux en affiche des dizaines.
enum PinRenderer {
    static func image(for look: PinLook) -> UIImage {
        switch look {
        case let .avatar(id, size):
            return avatar(id, size: size)
        case let .emoji(emoji, size):
            return disc(emoji: emoji, diameter: size)
        case let .report(emoji, confirms):
            return report(emoji: emoji, confirms: confirms)
        case let .bubble(text, sub, color, selected):
            return bubble(text: text, sub: sub, color: UIColor(hex: color), selected: selected)
        case let .marker(symbol, color):
            return marker(symbol: symbol, color: UIColor(hex: color))
        case .flame:
            return flame()
        }
    }

    /// Marqueur de foyer, repris du site : halo diffus, anneau soutenu, cœur
    /// rouge et flamme dorée. La pulsation d'origine disparaît — une image ne
    /// s'anime pas — mais le halo reste, et l'épingle tient à des dizaines
    /// d'exemplaires sans peser sur la carte.
    private static func flame() -> UIImage {
        let side: CGFloat = 64
        let red = UIColor(red: 0.86, green: 0.13, blue: 0.11, alpha: 1)
        return UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { ctx in
            let mid = side / 2
            func disc(_ diameter: CGFloat, _ color: UIColor) {
                color.setFill()
                ctx.cgContext.fillEllipse(
                    in: CGRect(
                        x: mid - diameter / 2, y: mid - diameter / 2,
                        width: diameter, height: diameter))
            }
            disc(60, red.withAlphaComponent(0.30))
            disc(48, red.withAlphaComponent(0.50))
            disc(36, red)
            let config = UIImage.SymbolConfiguration(pointSize: 18, weight: .bold)
            let gold = UIColor(red: 1, green: 0.85, blue: 0.40, alpha: 1)
            if let glyph = UIImage(systemName: "flame.fill", withConfiguration: config)?
                .withTintColor(gold, renderingMode: .alwaysOriginal)
            {
                glyph.draw(
                    in: CGRect(
                        x: mid - glyph.size.width / 2, y: mid - glyph.size.height / 2,
                        width: glyph.size.width, height: glyph.size.height))
            }
        }
    }

    private static func avatar(_ id: String, size: CGFloat) -> UIImage {
        if id == "walk" || id == "bike" {
            let symbol = id == "walk" ? "figure.walk" : "bicycle"
            let tint: UIColor = id == "walk" ? .systemGreen : .systemBlue
            return disc(symbol: symbol, diameter: size, fill: tint)
        }
        guard let art = UIImage(named: Avatars.asset(id)) else {
            return disc(emoji: "🚐", diameter: size)
        }
        let ratio = art.size.height == 0 ? 1 : art.size.width / art.size.height
        let box = CGSize(width: size * min(1, ratio), height: size / max(1, ratio))
        return UIGraphicsImageRenderer(size: box).image { _ in
            art.draw(in: CGRect(origin: .zero, size: box))
        }
    }

    private static func disc(emoji: String, diameter: CGFloat) -> UIImage {
        let pad: CGFloat = 3
        let side = diameter + pad * 2
        return UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { ctx in
            let rect = CGRect(x: pad, y: pad, width: diameter, height: diameter)
            UIColor.white.setFill()
            ctx.cgContext.fillEllipse(in: rect)
            UIColor.black.withAlphaComponent(0.15).setStroke()
            ctx.cgContext.setLineWidth(0.5)
            ctx.cgContext.strokeEllipse(in: rect)
            let font = UIFont.systemFont(ofSize: diameter * 0.52)
            let size = (emoji as NSString).size(withAttributes: [.font: font])
            (emoji as NSString).draw(
                at: CGPoint(x: rect.midX - size.width / 2, y: rect.midY - size.height / 2),
                withAttributes: [.font: font])
        }
    }

    private static func disc(symbol: String, diameter: CGFloat, fill: UIColor) -> UIImage {
        let pad: CGFloat = 3
        let side = diameter + pad * 2
        let config = UIImage.SymbolConfiguration(pointSize: diameter * 0.5, weight: .bold)
        let glyph = UIImage(systemName: symbol, withConfiguration: config)?
            .withTintColor(.white, renderingMode: .alwaysOriginal)
        return UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { ctx in
            let rect = CGRect(x: pad, y: pad, width: diameter, height: diameter)
            fill.setFill()
            ctx.cgContext.fillEllipse(in: rect)
            if let glyph {
                glyph.draw(
                    in: CGRect(
                        x: rect.midX - glyph.size.width / 2,
                        y: rect.midY - glyph.size.height / 2,
                        width: glyph.size.width, height: glyph.size.height))
            }
        }
    }

    private static func report(emoji: String, confirms: Int) -> UIImage {
        let base = disc(emoji: emoji, diameter: 34)
        guard confirms > 0 else { return base }
        let badge = "\(confirms)" as NSString
        let font = UIFont.systemFont(ofSize: 11, weight: .bold)
        let textSize = badge.size(withAttributes: [.font: font])
        let badgeW = max(16, textSize.width + 8)
        let size = CGSize(width: base.size.width + badgeW / 2, height: base.size.height + 6)
        return UIGraphicsImageRenderer(size: size).image { ctx in
            base.draw(in: CGRect(x: 0, y: 6, width: base.size.width, height: base.size.height))
            let rect = CGRect(x: size.width - badgeW, y: 0, width: badgeW, height: 16)
            UIColor.systemGreen.setFill()
            ctx.cgContext.fillEllipse(in: rect.insetBy(dx: 0, dy: 0))
            badge.draw(
                at: CGPoint(x: rect.midX - textSize.width / 2, y: rect.midY - textSize.height / 2),
                withAttributes: [.font: font, .foregroundColor: UIColor.white])
        }
    }

    private static func bubble(text: String, sub: String?, color: UIColor, selected: Bool)
        -> UIImage
    {
        let font = UIFont.systemFont(ofSize: 12, weight: .bold)
        let subFont = UIFont.systemFont(ofSize: 11, weight: .semibold)
        let line = text as NSString
        let lineSize = line.size(withAttributes: [.font: font])
        var subSize = CGSize.zero
        if let sub { subSize = (sub as NSString).size(withAttributes: [.font: subFont]) }
        let w = max(lineSize.width, subSize.width) + 20
        let h = lineSize.height + (sub == nil ? 0 : subSize.height) + 10
        return UIGraphicsImageRenderer(size: CGSize(width: w, height: h)).image { ctx in
            let rect = CGRect(x: 1, y: 1, width: w - 2, height: h - 2)
            let path = UIBezierPath(roundedRect: rect, cornerRadius: rect.height / 2)
            (selected ? color : UIColor.systemBackground).setFill()
            path.fill()
            color.setStroke()
            path.lineWidth = 2
            path.stroke()
            let textColor: UIColor = selected ? .white : .label
            line.draw(
                at: CGPoint(x: rect.midX - lineSize.width / 2, y: rect.minY + 4),
                withAttributes: [.font: font, .foregroundColor: textColor])
            if let sub {
                (sub as NSString).draw(
                    at: CGPoint(
                        x: rect.midX - subSize.width / 2, y: rect.minY + 4 + lineSize.height),
                    withAttributes: [.font: subFont, .foregroundColor: textColor])
            }
        }
    }

    private static func marker(symbol: String, color: UIColor) -> UIImage {
        disc(symbol: symbol, diameter: 30, fill: color)
    }
}

// MARK: - Couleurs

extension Color {
    /// Couleur SwiftUI en `#RRGGBB`.
    ///
    /// Les épingles et tracés circulent sous forme de chaînes : deux `Color`
    /// construites séparément ne se comparent pas de façon fiable, alors qu'une
    /// chaîne se compare et sert de clé de cache.
    var hex: String {
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        guard UIColor(self).getRed(&r, green: &g, blue: &b, alpha: &a) else { return "#8E8E93" }
        return String(format: "#%02X%02X%02X", Int(r * 255), Int(g * 255), Int(b * 255))
    }
}

extension String {
    /// Le reste de la chaîne si elle commence par `prefix`, sinon `nil`.
    func strippingPrefix(_ prefix: String) -> String? {
        hasPrefix(prefix) ? String(dropFirst(prefix.count)) : nil
    }
}

extension UIColor {
    /// Couleur depuis `#RRGGBB` — les vues passent des chaînes pour rester
    /// comparables d'une évaluation à l'autre.
    convenience init(hex: String) {
        var s = hex.trimmingCharacters(in: .whitespacesAndNewlines)
        if s.hasPrefix("#") { s.removeFirst() }
        var value: UInt64 = 0
        Scanner(string: s).scanHexInt64(&value)
        let r = CGFloat((value & 0xFF0000) >> 16) / 255
        let g = CGFloat((value & 0x00FF00) >> 8) / 255
        let b = CGFloat(value & 0x0000FF) / 255
        self.init(red: r, green: g, blue: b, alpha: 1)
    }
}
