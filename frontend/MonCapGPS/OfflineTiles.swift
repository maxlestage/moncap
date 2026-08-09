import Foundation
import MapKit

/// Serveur de tuiles choisi par l'utilisateur (réglages), sur le modèle des
/// clés FIRMS et TomTom déjà présentes.
///
/// Apple Plans ne permet ni de télécharger ni de conserver ses tuiles : les
/// cartes hors ligne reposent donc sur une seconde source, image (« slippy
/// map »), que chacun choisit selon la licence à laquelle il a droit.
/// Vide = fonction désactivée.
enum TileSource {
    static let defaultsKey = "moncap.tileURL"
    /// Style vectoriel MapLibre. Renseigné, il prime sur les tuiles images.
    static let styleKey = "moncap.styleURL"

    /// Style par défaut : OpenFreeMap sert des tuiles vectorielles OpenStreetMap
    /// libres, sans clé ni inscription, polices et icônes comprises.
    ///
    /// C'est ce qui rend la carte utilisable sans rien installer. Un serveur
    /// personnel (archive PMTiles) reste possible : il suffit de le renseigner
    /// dans les réglages, il prend alors le pas sur cette valeur.
    static let defaultStyle = "https://tiles.openfreemap.org/styles/liberty"

    /// Style effectivement utilisé : celui de l'utilisateur, sinon le nôtre.
    static var styleURL: URL {
        let custom = (UserDefaults.standard.string(forKey: styleKey) ?? "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if custom.hasPrefix("http"), let url = URL(string: custom) { return url }
        return URL(string: defaultStyle)!
    }

    /// Fond à utiliser : celui de l'utilisateur s'il répond vraiment, sinon le
    /// nôtre.
    ///
    /// Une adresse enregistrée qui ne sert aucune carte donnait un écran vide.
    /// La vérifier avant de la retenir évite d'y rester bloqué, et se fait ici
    /// une seule fois pour tous les écrans.
    static func resolvedStyle(custom: String) async -> URL {
        let trimmed = custom.trimmingCharacters(in: .whitespacesAndNewlines)
        let fallback = URL(string: defaultStyle)!
        guard trimmed.hasPrefix("http"), let url = URL(string: trimmed) else { return fallback }
        return await StyleProbe.check(url) == .ok ? url : fallback
    }

    /// Modèle d'adresse, par exemple `https://exemple.fr/{z}/{x}/{y}.png`.
    static var template: String {
        (UserDefaults.standard.string(forKey: defaultsKey) ?? "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static var isConfigured: Bool {
        let t = template
        return t.contains("{z}") && t.contains("{x}") && t.contains("{y}")
    }
}

/// État d'un style vectoriel, vérifié avant de rendre la carte.
///
/// Sans cette vérification, un serveur sans cartes configurées donne une carte
/// **grise et muette** : MapLibre échoue en silence. On préfère le dire.
enum StyleStatus: Equatable {
    case checking
    case ok
    /// Le serveur répond, mais aucune carte n'y est installée.
    case notConfigured
    /// Serveur injoignable (pas de réseau, ou en panne).
    case unreachable

    /// Message à afficher, ou `nil` si tout va bien.
    ///
    /// `hasStored` : des zones sont déjà enregistrées. Sans réseau elles
    /// restent consultables — inutile d'alarmer l'utilisateur.
    func message(hasStored: Bool) -> String? {
        switch self {
        case .checking, .ok:
            return nil
        case .notConfigured:
            // Le fond personnel injoignable est écarté en amont : rester ici
            // signifie que même le fond par défaut ne sert rien.
            return "Fond de carte indisponible pour le moment."
        case .unreachable:
            return hasStored
                ? nil
                : "Serveur de cartes injoignable. Les zones déjà enregistrées restent consultables."
        }
    }

    /// Télécharger n'a de sens que si le serveur a bien des cartes.
    var canDownload: Bool { self == .ok }
}

/// Vérifie qu'une feuille de style est réellement servie et exploitable.
enum StyleProbe {
    /// Classe une réponse HTTP. Séparé du réseau pour rester vérifiable.
    static func classify(status: Int, body: Data) -> StyleStatus {
        // 404 : notre backend répond ainsi tant que ni TILES_PMTILES ni
        // TILES_UPSTREAM ne sont renseignés.
        guard status == 200 else {
            return status == 404 ? .notConfigured : .unreachable
        }
        // Un style utilisable a des sources : sans elles, la carte serait vide.
        guard
            let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any],
            let sources = json["sources"] as? [String: Any], !sources.isEmpty
        else { return .notConfigured }
        return .ok
    }

    static func check(_ url: URL) async -> StyleStatus {
        var req = URLRequest(url: url)
        req.timeoutInterval = 10
        do {
            let (data, response) = try await URLSession.shared.data(for: req)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            return classify(status: status, body: data)
        } catch {
            return .unreachable
        }
    }
}

/// Une tuile de carte, repérée par son niveau de zoom et sa position.
struct TilePath: Hashable {
    let z: Int
    let x: Int
    let y: Int
}

/// Conversions entre coordonnées géographiques et tuiles (Web Mercator).
enum TileMath {
    /// Tuile contenant un point, à un zoom donné.
    static func tile(lat: Double, lon: Double, z: Int) -> (x: Int, y: Int) {
        let n = Double(1 << z)
        let x = (lon + 180) / 360 * n
        let latRad = lat * .pi / 180
        let y = (1 - log(tan(latRad) + 1 / cos(latRad)) / .pi) / 2 * n
        let maxIndex = (1 << z) - 1
        return (
            x: Swift.min(maxIndex, Swift.max(0, Int(x.rounded(.down)))),
            y: Swift.min(maxIndex, Swift.max(0, Int(y.rounded(.down))))
        )
    }

    /// Toutes les tuiles couvrant une région, pour une plage de zooms.
    static func tiles(in region: MKCoordinateRegion, zooms: ClosedRange<Int>) -> [TilePath] {
        let north = Swift.min(85, region.center.latitude + region.span.latitudeDelta / 2)
        let south = Swift.max(-85, region.center.latitude - region.span.latitudeDelta / 2)
        let west = region.center.longitude - region.span.longitudeDelta / 2
        let east = region.center.longitude + region.span.longitudeDelta / 2
        var out: [TilePath] = []
        for z in zooms {
            // En Web Mercator l'axe des tuiles descend : le nord donne le petit y.
            let topLeft = tile(lat: north, lon: west, z: z)
            let bottomRight = tile(lat: south, lon: east, z: z)
            guard topLeft.x <= bottomRight.x, topLeft.y <= bottomRight.y else { continue }
            for x in topLeft.x...bottomRight.x {
                for y in topLeft.y...bottomRight.y {
                    out.append(TilePath(z: z, x: x, y: y))
                }
            }
        }
        return out
    }
}

/// Tuiles enregistrées sur l'appareil.
enum TileCache {
    /// Poids moyen constaté d'une tuile, pour estimer un téléchargement avant
    /// de le lancer.
    static let averageTileBytes = 18_000

    static var directory: URL? {
        guard
            let base = FileManager.default
                .urls(for: .applicationSupportDirectory, in: .userDomainMask).first
        else { return nil }
        let dir = base.appendingPathComponent("tuiles", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    static func fileURL(_ path: TilePath) -> URL? {
        directory?
            .appendingPathComponent("\(path.z)-\(path.x)-\(path.y).tile")
    }

    static func data(for path: TilePath) -> Data? {
        guard let url = fileURL(path) else { return nil }
        return try? Data(contentsOf: url)
    }

    static func store(_ data: Data, for path: TilePath) {
        guard let url = fileURL(path) else { return }
        try? data.write(to: url, options: .atomic)
    }

    static func has(_ path: TilePath) -> Bool {
        guard let url = fileURL(path) else { return false }
        return FileManager.default.fileExists(atPath: url.path)
    }

    /// Espace occupé, en octets.
    static func sizeBytes() -> Int {
        guard let directory,
            let files = try? FileManager.default.contentsOfDirectory(
                at: directory, includingPropertiesForKeys: [.fileSizeKey])
        else { return 0 }
        return files.reduce(0) { total, url in
            let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
            return total + size
        }
    }

    static func clear() {
        guard let directory else { return }
        try? FileManager.default.removeItem(at: directory)
    }
}

/// Couche de tuiles servant **d'abord le disque**, puis le réseau.
///
/// Hors ligne, une tuile absente renvoie une image vide plutôt qu'une erreur :
/// la carte reste utilisable là où elle a été téléchargée, sans message
/// d'échec sur les bords.
final class CachedTileOverlay: MKTileOverlay {
    /// Faux quand aucun réseau n'est disponible : on ne tente alors même pas
    /// la requête, qui échouerait après un délai inutile.
    var allowsNetwork = true

    override func loadTile(
        at path: MKTileOverlayPath, result: @escaping (Data?, (any Error)?) -> Void
    ) {
        let tile = TilePath(z: path.z, x: path.x, y: path.y)
        if let cached = TileCache.data(for: tile) {
            result(cached, nil)
            return
        }
        guard allowsNetwork, TileSource.isConfigured else {
            result(nil, nil)
            return
        }
        let url = self.url(forTilePath: path)
        URLSession.shared.dataTask(with: url) { data, response, _ in
            guard let data, !data.isEmpty,
                (response as? HTTPURLResponse)?.statusCode == 200
            else {
                result(nil, nil)
                return
            }
            // Une tuile vue en ligne est conservée : elle sera là hors réseau.
            TileCache.store(data, for: tile)
            result(data, nil)
        }.resume()
    }
}

/// Téléchargement d'une zone de carte, avec avancement et annulation.
@MainActor
final class TileDownloader: ObservableObject {
    @Published private(set) var running = false
    /// Avancement de 0 à 1.
    @Published private(set) var progress: Double = 0
    @Published private(set) var done = 0
    @Published private(set) var total = 0
    @Published private(set) var failed = 0

    private var task: Task<Void, Never>?

    /// Garde-fou : au-delà, le téléchargement pèserait plusieurs gigaoctets.
    static let maxTiles = 20_000
    /// Requêtes simultanées : assez pour être rapide, assez peu pour rester
    /// courtois envers le serveur de tuiles.
    private static let parallel = 5

    /// Nombre de tuiles et poids estimé d'un téléchargement, avant lancement.
    static func estimate(region: MKCoordinateRegion, zooms: ClosedRange<Int>)
        -> (count: Int, bytes: Int)
    {
        let tiles = TileMath.tiles(in: region, zooms: zooms)
        let missing = tiles.filter { !TileCache.has($0) }
        return (missing.count, missing.count * TileCache.averageTileBytes)
    }

    func start(region: MKCoordinateRegion, zooms: ClosedRange<Int>) {
        guard !running, TileSource.isConfigured else { return }
        let tiles = TileMath.tiles(in: region, zooms: zooms)
            .filter { !TileCache.has($0) }
            .prefix(Self.maxTiles)
        guard !tiles.isEmpty else { return }

        running = true
        progress = 0
        done = 0
        failed = 0
        total = tiles.count
        let template = TileSource.template
        let list = Array(tiles)

        // Détachée : les appels vers l'acteur principal traversent alors
        // explicitement l'isolation, sans ambiguïté de contexte.
        task = Task.detached { [weak self] in
            await withTaskGroup(of: Bool.self) { group in
                var next = 0
                func addTask() {
                    guard next < list.count else { return }
                    let tile = list[next]
                    next += 1
                    group.addTask { await Self.fetch(tile, template: template) }
                }
                for _ in 0..<Swift.min(Self.parallel, list.count) { addTask() }
                while let ok = await group.next() {
                    if Task.isCancelled { break }
                    await self?.advance(success: ok)
                    addTask()
                }
            }
            await self?.finish()
        }
    }

    func cancel() {
        task?.cancel()
        task = nil
        running = false
    }

    private func advance(success: Bool) {
        done += 1
        if !success { failed += 1 }
        progress = total > 0 ? Double(done) / Double(total) : 0
    }

    private func finish() {
        running = false
        task = nil
    }

    /// Télécharge et enregistre une tuile. `false` en cas d'échec.
    nonisolated private static func fetch(_ tile: TilePath, template: String) async -> Bool {
        let address =
            template
            .replacingOccurrences(of: "{z}", with: String(tile.z))
            .replacingOccurrences(of: "{x}", with: String(tile.x))
            .replacingOccurrences(of: "{y}", with: String(tile.y))
        guard let url = URL(string: address) else { return false }
        guard
            let (data, response) = try? await URLSession.shared.data(from: url),
            (response as? HTTPURLResponse)?.statusCode == 200,
            !data.isEmpty
        else { return false }
        TileCache.store(data, for: tile)
        return true
    }
}
