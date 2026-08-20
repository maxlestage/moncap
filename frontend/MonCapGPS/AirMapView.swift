import CoreLocation
import MapLibre
import SwiftUI

/// Qualité de l'air, servie par notre backend (`GET /air`).
///
/// La mesure vient d'Open-Meteo, gratuite et **sans clé**, interrogée par le
/// serveur pour toute une grille en une requête : une carte se lit par taches,
/// pas par relevés isolés.
///
/// L'écran peint une **nappe continue** et non des pastilles. Une pastille pose
/// la mesure en un point et laisse croire qu'ailleurs on ne sait rien, alors
/// que le modèle donne une valeur partout ; et à l'échelle d'une ville elle se
/// réduisait à un rond vert unique au milieu du vide. La grille est donc versée
/// dans une image géoréférencée, dégradée d'une mesure à sa voisine, qui couvre
/// tout ce qu'on regarde.
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
    /// Place dans le treillis : colonne, ligne, et ses dimensions. Facultatives
    /// pour la même raison ; à défaut, le treillis est déduit des coordonnées.
    let ix: Int?
    let iy: Int?
    let nx: Int?
    let ny: Int?

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

    /// Le même niveau, mais **continu** : 1,5 pour une valeur au milieu du
    /// deuxième palier.
    ///
    /// C'est ce nombre-là que la nappe dégrade d'une mesure à sa voisine, et
    /// non la valeur brute : les deux échelles n'ont ni les mêmes bornes ni le
    /// même pas, et interpoler entre un indice européen et un américain
    /// donnerait une couleur qui ne veut rien dire. Ramenés au niveau, ils se
    /// mêlent sans se confondre.
    var niveauContinu: Double? {
        guard let v = valeur else { return nil }
        let paliers = echelle.paliers
        if v <= paliers[0] { return max(v, 0) / paliers[0] }
        for i in 1..<paliers.count where v <= paliers[i] {
            return Double(i) + (v - paliers[i - 1]) / (paliers[i] - paliers[i - 1])
        }
        return Double(paliers.count)
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

/// Fabrique la nappe : une image géoréférencée, dégradée d'une mesure à sa
/// voisine.
///
/// Des rectangles pleins couvriraient bien l'écran, mais en damier — or l'air
/// ne change pas de qualité au franchissement d'une frontière invisible. Une
/// image posée sur la zone laisse le moteur de rendu adoucir les bords, et
/// coûte un objet au lieu de quatre cents. C'est ainsi que procèdent les cartes
/// de l'air qu'on connaît.
///
/// Interpoler n'ajoute aucune information — la mesure reste au cinquième de
/// degré — mais le champ mesuré, lui, est continu : le dégradé le représente
/// plus fidèlement que des marches d'escalier.
enum AirNappe {
    /// Le treillis des mesures, reconstruit depuis la réponse.
    struct Grille {
        let nx: Int
        let ny: Int
        /// Coin sud-ouest de la nappe, bords des mailles compris.
        let o: Double
        let su: Double
        let dlat: Double
        let dlon: Double
        /// Niveaux continus, du sud au nord puis d'ouest en est. `nil` là où la
        /// mesure manque.
        let niveaux: [Double?]

        var e: Double { o + dlon * Double(nx) }
        var no: Double { su + dlat * Double(ny) }
    }

    /// Grandissement de l'image par rapport au treillis.
    ///
    /// Seize : l'intérieur d'une maille reste d'une couleur franche, et seule
    /// la frontière entre deux niveaux s'adoucit, sur quelques pixels. Plus
    /// grossier, le moteur ferait un dégradé de bout en bout et l'on ne
    /// lirait plus les paliers ; plus fin, on paierait des pixels pour rien.
    static let grandissement = 16

    /// Limite du Mercator sphérique : au-delà, la projection part à l'infini.
    static let latitudeMax = 85.0

    /// Ordonnée Mercator d'une latitude, en radians.
    static func mercator(_ lat: Double) -> Double {
        let l = min(max(lat, -latitudeMax), latitudeMax) * .pi / 180
        return log(tan(.pi / 4 + l / 2))
    }

    /// La latitude qui correspond à une ordonnée Mercator, en degrés.
    static func latitude(_ y: Double) -> Double {
        (2 * atan(exp(y)) - .pi / 2) * 180 / .pi
    }

    /// Reconstruit le treillis. `nil` si la réponse n'en permet pas un.
    static func grille(_ points: [AirPoint]) -> Grille? {
        guard let premier = points.first else { return nil }
        let (dlat, dlon) = premier.maille
        guard dlat > 0, dlon > 0 else { return nil }

        // Le cas normal : le serveur dit la place de chaque mesure.
        if let nx = premier.nx, let ny = premier.ny, nx > 0, ny > 0,
            points.allSatisfy({ $0.ix != nil && $0.iy != nil })
        {
            var niveaux = [Double?](repeating: nil, count: nx * ny)
            for p in points {
                guard let ix = p.ix, let iy = p.iy, ix < nx, iy < ny else { continue }
                niveaux[iy * nx + ix] = p.niveauContinu
            }
            return Grille(
                nx: nx, ny: ny,
                o: premier.lon - dlon * (Double(premier.ix ?? 0) + 0.5),
                su: premier.lat - dlat * (Double(premier.iy ?? 0) + 0.5),
                dlat: dlat, dlon: dlon, niveaux: niveaux)
        }

        // Repli, face à un serveur d'avant cette version : le treillis est
        // déduit des coordonnées distinctes. Une colonne entièrement dépourvue
        // de mesure décalerait le reste — c'est pourquoi ce n'est qu'un repli.
        let lons = Array(Set(points.map(\.lon))).sorted()
        let lats = Array(Set(points.map(\.lat))).sorted()
        guard let o = lons.first, let su = lats.first else { return nil }
        let colonne = Dictionary(uniqueKeysWithValues: lons.enumerated().map { ($1, $0) })
        let ligne = Dictionary(uniqueKeysWithValues: lats.enumerated().map { ($1, $0) })
        var niveaux = [Double?](repeating: nil, count: lons.count * lats.count)
        for p in points {
            guard let ix = colonne[p.lon], let iy = ligne[p.lat] else { continue }
            niveaux[iy * lons.count + ix] = p.niveauContinu
        }
        return Grille(
            nx: lons.count, ny: lats.count,
            o: o - dlon / 2, su: su - dlat / 2,
            dlat: dlat, dlon: dlon, niveaux: niveaux)
    }

    /// Peint le treillis : un pixel par point du dégradé, transparent là où
    /// aucune mesure ne porte.
    static func image(_ g: Grille) -> UIImage? {
        let largeur = g.nx * grandissement
        let hauteur = g.ny * grandissement
        guard largeur > 0, hauteur > 0 else { return nil }

        let palette: [(Double, Double, Double)] = AirCouleurs.niveaux.map { c in
            var r: CGFloat = 0, v: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
            c.getRed(&r, green: &v, blue: &b, alpha: &a)
            return (Double(r) * 255, Double(v) * 255, Double(b) * 255)
        }

        /// La couleur d'un niveau continu, prise **entre** deux paliers.
        ///
        /// Colorer par palier entier gâchait la mesure : relevée sur l'Europe,
        /// elle va de 16 à 78, mais le seul palier « 20 à 40 » avalait la
        /// moitié du continent et la nappe s'étalait d'un vert unique. Le
        /// dégradé rend cette variation, et c'est aussi ce qu'annonce la
        /// légende — un ruban continu, non six pastilles. Le palier nommé, lui,
        /// reste donné au doigt, avec son chiffre.
        func teinte(_ n: Double) -> (UInt8, UInt8, UInt8) {
            let borne = min(max(n, 0), Double(palette.count - 1))
            let bas = min(Int(borne), palette.count - 2)
            let f = borne - Double(bas)
            let (r0, v0, b0) = palette[bas]
            let (r1, v1, b1) = palette[bas + 1]
            return (
                UInt8((r0 + (r1 - r0) * f).rounded()),
                UInt8((v0 + (v1 - v0) * f).rounded()),
                UInt8((b0 + (b1 - b0) * f).rounded())
            )
        }

        var pixels = [UInt8](repeating: 0, count: largeur * hauteur * 4)
        let pas = Double(grandissement)

        // L'image est tendue sur le quadrilatère en **Mercator**, pas en
        // latitudes régulières : c'est ainsi que le moteur la plaque. Peinte
        // en latitudes régulières, elle glisserait vers le sud à mesure qu'on
        // monte — sur une zone de 40 à 54°, son milieu tomberait à 47,0° au
        // lieu de 47,5°, cinquante et un kilomètres d'écart, deux fois la
        // maille du modèle ; et l'écart croît avec la hauteur de la zone.
        // Chaque ligne de l'image est donc ramenée à sa vraie latitude avant
        // d'aller chercher ses mesures.
        let yHaut = Self.mercator(g.no)
        let yBas = Self.mercator(g.su)

        for py in 0..<hauteur {
            // L'image se lit du nord au sud, le treillis du sud au nord.
            let y = yHaut + (Double(py) + 0.5) / Double(hauteur) * (yBas - yHaut)
            let lat = Self.latitude(y)
            let gy = (lat - g.su) / g.dlat - 0.5
            let j0 = Int(floor(gy))
            let fy = gy - floor(gy)

            for px in 0..<largeur {
                let gx = (Double(px) + 0.5) / pas - 0.5
                let i0 = Int(floor(gx))
                let fx = gx - floor(gx)

                // Moyenne pondérée des quatre mesures voisines. Celles qui
                // manquent sont simplement retirées du calcul, et leur poids
                // avec : un trou isolé se comble au lieu de trouer la nappe.
                var somme = 0.0
                var poids = 0.0
                for dj in 0...1 {
                    let j = min(max(j0 + dj, 0), g.ny - 1)
                    let py2 = dj == 0 ? 1 - fy : fy
                    for di in 0...1 {
                        let i = min(max(i0 + di, 0), g.nx - 1)
                        let px2 = di == 0 ? 1 - fx : fx
                        guard let n = g.niveaux[j * g.nx + i] else { continue }
                        somme += n * py2 * px2
                        poids += py2 * px2
                    }
                }
                guard poids > 0.0001 else { continue }

                let (r, v, b) = teinte(somme / poids)
                let o = (py * largeur + px) * 4
                pixels[o] = r
                pixels[o + 1] = v
                pixels[o + 2] = b
                pixels[o + 3] = 255
            }
        }

        guard let fournisseur = CGDataProvider(data: Data(pixels) as CFData),
            let cg = CGImage(
                width: largeur, height: hauteur,
                bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: largeur * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: fournisseur, decode: nil, shouldInterpolate: true,
                intent: .defaultIntent)
        else { return nil }
        return UIImage(cgImage: cg)
    }
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

    /// Ce que le serveur a répondu. Les trois cas ne se disent pas pareil : la
    /// mer, la panne d'amont et l'absence de réseau ont chacune leur phrase.
    private enum Reponse {
        case mesures([AirPoint])
        /// Le serveur a répondu, mais n'a pas la donnée — amont en panne ou
        /// qui nous refuse.
        case indisponible
        /// On n'a pas pu lui parler.
        case reseau
    }

    /// Demande la grille d'une zone. Une nouvelle demande annule la précédente :
    /// en faisant glisser la carte, seule la dernière compte.
    func charge(bbox: String) {
        tache?.cancel()
        chargement = true
        let base = self.base
        tache = Task {
            let reponse = await Self.fetch(base: base, bbox: bbox)
            if Task.isCancelled { return }
            chargement = false

            switch reponse {
            case .mesures(let recus) where !recus.isEmpty:
                points = recus
                message = nil

            case .mesures:
                // Vide sur une zone neuve, c'est le plus souvent le serveur
                // qui a rendu la main avant d'avoir sa donnée. Une seule
                // reprise la cueille, et le message d'océan n'attend qu'elle —
                // sinon il accuserait la mer d'une simple attente.
                if zoneReprise != bbox {
                    zoneReprise = bbox
                    message = nil
                    try? await Task.sleep(for: .seconds(4))
                    charge(bbox: bbox)
                    return
                }
                message = manquant(String(localized: "Aucune mesure ici. Rapproche-toi des terres."))

            case .indisponible:
                message = manquant(String(localized: "Mesures momentanément indisponibles."))

            case .reseau:
                message = manquant(String(localized: "Mesures indisponibles — il faut du réseau."))
            }
        }
    }

    /// Le message n'a lieu d'être que si l'écran n'a rien à montrer.
    ///
    /// La nappe déjà peinte n'est **pas** effacée par une réponse vide : elle
    /// reste à sa place, qui est géographique, et l'écran cesse de clignoter
    /// entre une carte peinte et une carte nue à chaque déplacement.
    private func manquant(_ texte: String) -> String? {
        points.isEmpty ? texte : nil
    }

    nonisolated private static func fetch(base: URL, bbox: String) async -> Reponse {
        var comps = URLComponents(
            url: base.appendingPathComponent("air"), resolvingAgainstBaseURL: false)!
        comps.queryItems = [URLQueryItem(name: "bbox", value: bbox)]
        guard let url = comps.url,
            let (data, resp) = try? await URLSession.shared.data(from: url)
        else { return .reseau }
        guard (resp as? HTTPURLResponse)?.statusCode == 200,
            let points = try? JSONDecoder().decode([AirPoint].self, from: data)
        else { return .indisponible }
        return .mesures(points)
    }
}

/// La mesure d'air autour d'une position, pour la pastille de l'écran
/// principal.
///
/// Léger et paresseux, sur le modèle de `WeatherService` : rafraîchi à la
/// demi-heure — la donnée d'Open-Meteo est horaire — ou dès qu'on a changé de
/// case de 0,25°, environ vingt-cinq kilomètres. Une boîte d'un tiers de degré
/// autour de soi coûte quatre appels Open-Meteo, le plancher de la grille.
@MainActor
final class AirAutour: ObservableObject {
    /// La maille qui contient la position, ou la plus proche servie.
    @Published private(set) var point: AirPoint?

    private var caseServie: String?
    private var date = Date.distantPast
    private let base = APIClient().baseURL

    /// À appeler à chaque position, comme la météo : le tri se fait ici.
    func update(_ c: CLLocationCoordinate2D) {
        let case_ = "\((c.latitude * 4).rounded())|\((c.longitude * 4).rounded())"
        guard caseServie != case_ || Date().timeIntervalSince(date) > 1800 else { return }
        caseServie = case_
        date = Date()

        let bbox = String(
            format: "%.3f,%.3f,%.3f,%.3f",
            c.longitude - 0.15, c.latitude - 0.15,
            c.longitude + 0.15, c.latitude + 0.15)
        var comps = URLComponents(
            url: base.appendingPathComponent("air"), resolvingAgainstBaseURL: false)!
        comps.queryItems = [URLQueryItem(name: "bbox", value: bbox)]
        guard let url = comps.url else { return }

        Task {
            guard
                let (data, resp) = try? await URLSession.shared.data(from: url),
                (resp as? HTTPURLResponse)?.statusCode == 200,
                let points = try? JSONDecoder().decode([AirPoint].self, from: data),
                !points.isEmpty
            else {
                // Échec : on oublie la case pour que la prochaine position
                // retente, et la pastille garde sa dernière mesure plutôt que
                // de clignoter.
                caseServie = nil
                return
            }
            point = points.first { $0.contient(c) } ?? points.first
        }
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
        ZStack {
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

                // La légende à gauche, les commandes à droite : le milieu de
                // l'écran reste à la carte, qui est ce qu'on est venu voir.
                HStack(alignment: .top) {
                    legende
                    Spacer(minLength: 12)
                    commandes
                }
                .padding(.horizontal)

                if let m = service.message {
                    Text(m)
                        .font(.caption)
                        .multilineTextAlignment(.center)
                        .padding(10)
                        .frame(maxWidth: .infinity)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
                        .padding(.horizontal)
                }

                Spacer()

                // Une valeur est lisible sans rien toucher : celle d'ici. Une
                // carte qui n'annonce un chiffre qu'après un appui oblige à
                // deviner qu'on peut appuyer.
                if let p = selection ?? chezMoi {
                    fiche(p, chezMoi: selection == nil)
                }
                source
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
            // Une vue régionale, et non le quartier : la qualité de l'air se
            // lit sur des centaines de kilomètres, et de trop près la nappe
            // n'est plus qu'un aplat uni.
            if let center { cam = MapCam(center: center, zoom: 6) }
        }
    }

    /// La mesure qui porte sur là où l'on se trouve.
    private var chezMoi: AirPoint? {
        guard let c = center else { return nil }
        return service.points.first { $0.contient(c) }
    }

    /// Identité de la zone regardée : le centre, découpé à la mesure de ce
    /// qu'on voit.
    ///
    /// Un pas fixe d'un quart de degré convenait de près et devenait absurde
    /// de loin : vue du globe, la carte couvre cent soixante-dix degrés de
    /// haut, et le moindre frémissement du doigt changeait de case, donc
    /// relançait une grille de trois cent quatre-vingt-quatre mesures. Le pas
    /// suit maintenant la hauteur vue — un huitième — si bien qu'il faut
    /// déplacer la carte d'un huitième de son propre écran pour redemander,
    /// à toutes les échelles.
    private var caseRegardee: String? {
        guard let b = bounds else { return nil }
        let hauteur = abs(b.ne.latitude - b.sw.latitude)
        let pas = max(hauteur / 8, 0.25)
        let la = (b.sw.latitude + b.ne.latitude) / 2
        let lo = (b.sw.longitude + b.ne.longitude) / 2
        return "\((la / pas).rounded())|\((lo / pas).rounded())|\(Int(pas * 100))"
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
            // Une ligne, pas deux : l'échelle employée est nommée par la
            // légende, juste dessous. La répéter ici grossissait le bandeau
            // pour rien, et il mangeait la carte.
            HStack(spacing: 8) {
                Text("Qualité de l'air").font(.subheadline.weight(.semibold))
                if service.chargement { ProgressView().controlSize(.small) }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .background(.regularMaterial, in: Capsule())
            Spacer(minLength: 0)
        }
        .padding(.horizontal)
        .padding(.top, 6)
    }

    /// Les commandes de droite : revenir sur soi.
    /// Les commandes de droite : voir le monde entier.
    ///
    /// Le retour sur soi n'a plus besoin de bouton — la fiche « Autour de
    /// toi » le porte, c'est elle qu'on touche naturellement pour revenir à
    /// soi. La place libérée va au geste inverse : embrasser la planète d'un
    /// coup. Le serveur sert la grille mondiale comme n'importe quelle zone —
    /// mesuré : 384 mailles, l'indice de 8 à 113 selon les continents.
    private var commandes: some View {
        Button {
            selection = nil
            // Cadrée sur les terres habitées : à ce zoom, l'Antarctique ne
            // mesure rien et mangerait un tiers de l'écran.
            cam = MapCam(
                center: CLLocationCoordinate2D(latitude: 22, longitude: 10), zoom: 0.6)
        } label: {
            Image(systemName: "globe.europe.africa.fill")
                .font(.headline)
                .foregroundStyle(.tint)
                .frame(width: 40, height: 40)
                .background(.regularMaterial, in: Circle())
        }
        .accessibilityLabel(Text("Voir le monde entier"))
    }

    /// Fiche d'une mesure : le chiffre dans sa couleur, son palier, et les
    /// particules — la seule grandeur que l'on peut comparer d'un continent à
    /// l'autre.
    ///
    /// Le chiffre est **dans** la pastille plutôt qu'à côté : la couleur seule
    /// ne se lit qu'à la légende, le chiffre seul ne se situe pas sur une
    /// échelle ; ensemble ils se passent l'un de l'autre.
    private func fiche(_ p: AirPoint, chezMoi: Bool) -> some View {
        HStack(spacing: 13) {
            ZStack {
                Circle().fill(Color(AirCouleurs.couleur(p.niveau)))
                if let v = p.valeur {
                    Text(verbatim: "\(Int(v))")
                        .font(.title3.weight(.bold))
                        .foregroundStyle(.white)
                        .monospacedDigit()
                        .minimumScaleFactor(0.6)
                        .padding(4)
                }
            }
            .frame(width: 52, height: 52)

            VStack(alignment: .leading, spacing: 2) {
                Text(chezMoi ? "Autour de toi" : "Point touché")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Text(AirCouleurs.libelles[min(p.niveau, AirCouleurs.libelles.count - 1)])
                    .font(.headline)
                if let pm = p.pm25 {
                    Text(String(format: String(localized: "Particules fines : %.0f µg/m³"), pm))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
            }

            Spacer(minLength: 0)

            if chezMoi {
                // La fiche est un bouton : la toucher ramène la carte sur soi.
                // C'est le pendant du globe d'en haut — lui éloigne, elle
                // rapproche — et le pictogramme le laisse deviner.
                Image(systemName: "location.fill")
                    .font(.footnote.weight(.bold))
                    .foregroundStyle(.tint)
                    .frame(width: 28, height: 28)
            } else {
                Button { selection = nil } label: {
                    Image(systemName: "xmark")
                        .font(.footnote.weight(.bold))
                        .foregroundStyle(.secondary)
                        .frame(width: 28, height: 28)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(13)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
        .padding(.horizontal)
        .contentShape(Rectangle())
        .onTapGesture {
            guard chezMoi, let c = center else { return }
            selection = nil
            cam = MapCam(center: c, zoom: 6)
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(chezMoi ? .isButton : [])
        .accessibilityHint(chezMoi ? Text("Recentrer sur ma position") : Text(""))
    }

    /// Légende : sans elle, six couleurs ne disent rien.
    ///
    /// Les paliers sont **nommés**, et non résumés par « bon » à un bout et
    /// « extrêmement mauvais » à l'autre : entre les deux, un ruban dégradé
    /// laissait deviner. Du pire en haut au meilleur en bas, l'ordre qu'ont
    /// les cartes de l'air — l'œil descend vers ce qu'on espère.
    private var legende: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(service.points.first?.echelle.nom ?? "Indice européen")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.bottom, 2)

            ForEach((0..<AirCouleurs.libelles.count).reversed(), id: \.self) { i in
                HStack(spacing: 7) {
                    Capsule()
                        .fill(Color(AirCouleurs.niveaux[i]))
                        .frame(width: 4, height: 13)
                    Text(AirCouleurs.libelles[i])
                        .font(.caption2)
                        .lineLimit(1)
                }
            }
        }
        .padding(10)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .accessibilityElement(children: .contain)
    }

    /// D'où vient la mesure. Une carte qui ne dit pas sa source demande à être
    /// crue sur parole.
    private var source: some View {
        Text("Mesures Open-Meteo (CAMS)")
            .font(.caption2)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(.regularMaterial, in: Capsule())
            .padding(.bottom, 6)
    }
}

/// La carte elle-même : une image géoréférencée posée sur la zone regardée.
///
/// Une source d'image plutôt que quatre cents polygones : un seul objet à
/// construire, et c'est le moteur de rendu qui adoucit les frontières entre
/// niveaux, au pixel près, sans que rien ne remonte jusqu'ici.
struct AirMapLibreView: UIViewRepresentable {
    let styleURL: URL
    let points: [AirPoint]
    let cam: MapCam?
    let onRegionChange: (MLNCoordinateBounds) -> Void
    let onSelect: (AirPoint) -> Void

    func makeUIView(context: Context) -> SizedMapView {
        let view = SizedMapView(frame: .zero, styleURL: styleURL)
        context.coordinator.appliedStyle = styleURL
        // Filet de sécurité, le même que sur la carte principale : la toute
        // première caméra arrive avant que la carte n'ait sa place à l'écran,
        // et à l'arrêt rien ne la réémet. Faute de ce fil, l'écran s'ouvrait
        // sur le globe entier au lieu de la région où l'on se trouve — et de
        // si loin, le moindre glissement change de zone, ce qui relançait une
        // demande à chaque fois.
        view.onLayout = { [weak view, weak coordinator = context.coordinator] in
            guard let view, let coordinator else { return }
            coordinator.poseCameraEnAttente(sur: view)
        }
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
            c.camEnAttente = cam
            c.poseCameraEnAttente(sur: view)
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
        /// Caméra reçue mais pas encore posée, faute de taille à l'écran.
        var camEnAttente: MapCam?
        var styleReady = false
        /// Signature de ce qui est dessiné : sans elle, la couche serait
        /// refaite à chaque réévaluation de la vue.
        private var dessine: String = ""

        private static let source = "air-src"
        private static let couche = "air-nappe"

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            styleReady = true
            dessine = ""
            sync(on: mapView)
        }

        func mapView(_ mapView: MLNMapView, regionDidChangeAnimated animated: Bool) {
            onRegionChange(mapView.visibleCoordinateBounds)
        }

        /// Pose la caméra en attente, si la carte a de quoi la recevoir.
        ///
        /// Une carte de largeur nulle avale l'ordre sans rien faire : il faut
        /// donc garder la caméra sous le coude et la reposer au premier
        /// agencement qui lui donne une taille.
        func poseCameraEnAttente(sur view: MLNMapView) {
            guard let cam = camEnAttente else { return }
            guard view.bounds.width > 1, view.bounds.height > 1 else { return }
            camEnAttente = nil
            view.setCenter(cam.center, zoomLevel: cam.zoom, animated: false)
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
            guard !points.isEmpty,
                let g = AirNappe.grille(points),
                let image = AirNappe.image(g)
            else { return }

            let quad = MLNCoordinateQuadMake(
                CLLocationCoordinate2D(latitude: g.no, longitude: g.o),
                CLLocationCoordinate2D(latitude: g.su, longitude: g.o),
                CLLocationCoordinate2D(latitude: g.su, longitude: g.e),
                CLLocationCoordinate2D(latitude: g.no, longitude: g.e))
            let src = MLNImageSource(
                identifier: Self.source, coordinateQuad: quad, image: image)
            style.addSource(src)

            let couche = MLNRasterStyleLayer(identifier: Self.couche, source: src)
            // Assez opaque pour se lire d'un coup d'œil, assez transparent pour
            // qu'on reconnaisse encore le pays sous la couleur.
            couche.rasterOpacity = NSExpression(forConstantValue: 0.55)
            // Filtrage linéaire : l'image est déjà seize fois plus fine que le
            // treillis, si bien que seule la frontière entre deux niveaux
            // s'adoucit. C'est ce qu'annonce la légende, un ruban dégradé et
            // non six pastilles.
            // La valeur littérale plutôt que la catégorie `NSValue` : l'en-tête
            // documente les deux, et celle-ci ne dépend pas du nom que
            // l'importateur Swift donne à un constructeur d'énumération ObjC.
            couche.rasterResamplingMode = NSExpression(forConstantValue: "linear")

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
