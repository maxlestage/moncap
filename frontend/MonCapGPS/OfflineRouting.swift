import CoreLocation
import Foundation
import Valhalla

/// Calcul d'itinéraire **sur l'appareil**, sans réseau.
///
/// Apple exige une connexion pour `MKDirections` : rater une sortie hors
/// couverture laissait l'app sans rien à proposer. Valhalla calcule le même
/// genre d'itinéraire localement, à condition de disposer de ses propres
/// tuiles routières — distinctes des tuiles d'affichage, et bien plus légères
/// qu'attendu : 259 Mo pour l'Aquitaine entière.
///
/// L'échange se fait en JSON brut plutôt qu'avec les types Swift du paquet :
/// c'est le format documenté de Valhalla, stable, et cela évite de dépendre
/// de modèles susceptibles de changer d'une version à l'autre.
/// Traduction d'un message affiché. Recopiée ici plutôt qu'importée : les
/// helpers de `NavigationManager` sont privés à leur fichier, et les exposer
/// pour deux appels coûterait plus que ces quatre lignes.
private func tr(_ cle: String) -> String {
    NSLocalizedString(cle, comment: "")
}

/// Idem, avec une valeur à verser dans la phrase traduite.
private func trf(_ cle: String, _ valeurs: CVarArg...) -> String {
    String(format: NSLocalizedString(cle, comment: ""), arguments: valeurs)
}

@MainActor
final class OfflineRouter: NSObject, ObservableObject {
    static let shared = OfflineRouter()

    /// Rappel confié par le système quand il réveille l'app pour lui remettre
    /// un transfert achevé. À rappeler une fois l'écran à jour, sinon iOS
    /// considère l'app fautive et finit par lui refuser les réveils.
    nonisolated(unsafe) static var rappelSysteme: (() -> Void)?

    /// Identifiant de la session de fond. Stable : c'est par lui que le
    /// système retrouve les transferts d'un lancement à l'autre.
    private static let sessionID = "moncap.tuiles"

    /// Une région téléchargeable, telle que publiée par le workflow
    /// « Tuiles Valhalla » : une Release par région, hébergement gratuit et
    /// adresse stable.
    ///
    /// La liste n'est pas écrite ici : l'Europe en compte plus de deux cents,
    /// et une copie embarquée serait périmée dès la construction suivante.
    /// Elle est lue sur les Releases, cf. `loadRegions()`.
    struct Region: Identifiable, Hashable, Decodable {
        /// Étiquette de la Release, qui sert aussi de nom de fichier.
        let id: String
        /// « France · Corse », tel que nommé par Geofabrik.
        let nom: String
        /// Poids de l'archive, mesuré sur la pièce jointe.
        let octets: Int

        var archive: URL { URL(string: "\(Region.base)/\(id)/archive")! }
        var config: URL { URL(string: "\(Region.base)/\(id)/config")! }

        /// Premier segment du nom : de quoi regrouper deux cents régions.
        var pays: String { nom.components(separatedBy: " · ").first ?? nom }

        /// Ce qui reste une fois le pays retiré — vide pour un petit pays
        /// livré d'un bloc, qui n'a pas de sous-région.
        var detail: String {
            let parts = nom.components(separatedBy: " · ")
            return parts.count > 1 ? parts.dropFirst().joined(separator: " · ") : ""
        }

        var poids: String { String(format: "%.0f Mo", Double(octets) / 1_000_000) }

        /// Les paquets passent par **le backend**, pas par GitHub.
        ///
        /// L'app interrogeait l'API GitHub sans jeton. Le dépôt étant privé,
        /// GitHub répondait 404 : la liste restait vide, aucune région n'était
        /// proposée, et l'écran accusait le réseau. Un jeton ne peut pas
        /// voyager dans un binaire — n'importe qui l'en extrait, et il ouvrirait
        /// le dépôt entier. Le serveur le détient donc, publie la liste, et
        /// pour un fichier transmet l'adresse signée que GitHub lui rend :
        /// l'archive descend directement sur le téléphone, sans traverser le
        /// dyno.
        static let base = APIClient().baseURL.appendingPathComponent("offline/regions")
            .absoluteString
    }

    /// Une région gardée sur l'appareil.
    ///
    /// Le nom est conservé avec elle : hors réseau la liste des Releases est
    /// vide, et l'écran n'aurait plus qu'une étiquette technique à montrer.
    struct Installee: Codable, Identifiable, Hashable {
        let id: String
        let nom: String
        /// Poids de l'archive sur le disque, mesuré à l'installation.
        var octets: Int = 0

        var poids: String { String(format: "%.0f Mo", Double(octets) / 1_000_000) }
    }

    /// Régions publiées, relevées sur les Releases.
    @Published private(set) var regions: [Region] = []
    /// Régions gardées sur l'appareil, dans l'ordre d'installation.
    @Published private(set) var installees: [Installee] = []
    @Published private(set) var downloading = false
    @Published private(set) var chargement = false
    @Published private(set) var progress: Double = 0
    @Published private(set) var message: String?

    /// Moteurs chargés, un par région, construits à la première demande :
    /// ouvrir une archive coûte, et la plupart des trajets se calculent en
    /// ligne. Gardés ensuite, un trajet en enchaînant souvent d'autres.
    private var moteurs: [String: Valhalla] = [:]

    /// Session de fond : le système poursuit le transfert app quittée, et la
    /// relance à la fin. Construite paresseusement, mais une seule fois — deux
    /// sessions de même identifiant sont une erreur fatale.
    private lazy var session: URLSession = {
        let config = URLSessionConfiguration.background(withIdentifier: Self.sessionID)
        // Deux gigaoctets ne se téléchargent pas en itinérance sans prévenir.
        config.allowsCellularAccess = true
        config.isDiscretionary = false
        config.sessionSendsLaunchEvents = true
        return URLSession(configuration: config, delegate: self, delegateQueue: nil)
    }()

    private override init() {
        let brut = UserDefaults.standard.data(forKey: Self.key)
        let lues = brut.flatMap { try? JSONDecoder().decode([Installee].self, from: $0) } ?? []
        super.init()
        // Une région effacée à la main ne doit pas rester annoncée.
        installees = lues.filter {
            FileManager.default.fileExists(atPath: Self.archivePath($0.id).path)
        }
        if installees.count != lues.count { enregistre() }
    }

    private static let key = "moncap.valhallaRegions"
    private static let keyAttentes = "moncap.valhallaAttentes"

    private func enregistre() {
        if let data = try? JSONEncoder().encode(installees) {
            UserDefaults.standard.set(data, forKey: Self.key)
        }
    }

    nonisolated private static var dossier: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let dir = base.appendingPathComponent("itineraire", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    nonisolated private static func archivePath(_ id: String) -> URL {
        dossier.appendingPathComponent("\(id).tar")
    }

    nonisolated private static func configPath(_ id: String) -> URL {
        dossier.appendingPathComponent("\(id).json")
    }

    /// Un itinéraire peut-il être calculé sans réseau ?
    var ready: Bool { !installees.isEmpty }

    /// Place occupée par les régions gardées.
    var poidsTotal: String {
        let octets = installees.reduce(0) { $0 + $1.octets }
        return String(format: "%.1f Go", Double(octets) / 1_000_000_000)
    }

    func estInstallee(_ region: Region) -> Bool {
        installees.contains { $0.id == region.id }
    }

    /// Régions d'un même pays, pour l'affichage.
    struct Groupe: Identifiable {
        var id: String { pays }
        let pays: String
        let regions: [Region]
    }

    /// Les régions rangées par pays : deux cents entrées à plat ne se
    /// parcourent pas.
    var groupes: [Groupe] {
        Dictionary(grouping: regions, by: \.pays)
            .map { Groupe(pays: $0.key, regions: $0.value) }
            .sorted { $0.pays.localizedStandardCompare($1.pays) == .orderedAscending }
    }

    /// Régions dont le nom contient le texte cherché.
    ///
    /// `localizedStandardContains` ignore casse et accents : « ile de france »
    /// trouve « Île-de-France », ce qu'une comparaison brute manquerait — et
    /// les noms viennent de Geofabrik, donc dans la langue du pays.
    func recherche(_ texte: String) -> [Region] {
        let cherche = texte.trimmingCharacters(in: .whitespaces)
        guard !cherche.isEmpty else { return [] }
        return regions.filter { $0.nom.localizedStandardContains(cherche) }
    }

    /// Relève les régions publiées, avec leur nom et leur poids.
    ///
    /// La liste vit sur les Releases plutôt que dans le code : l'Europe en
    /// compte plus de deux cents, et chaque reconstruction en change les
    /// tailles. Une copie embarquée mentirait dès le lendemain.
    func loadRegions() async {
        guard regions.isEmpty, !chargement else { return }
        chargement = true
        defer { chargement = false }
        message = nil

        let url = URL(string: Region.base)!
        let data: Data
        let reponse: URLResponse
        do {
            (data, reponse) = try await URLSession.shared.data(from: url)
        } catch {
            // Seul cas où le réseau est vraiment en cause.
            message = tr("Liste des régions indisponible — il faut du réseau une fois.")
            return
        }

        // Trois pannes distinctes, trois messages : le geste à faire n'est pas
        // le même, et un message unique avait déjà envoyé chercher du côté du
        // réseau alors qu'il allait très bien.
        if let http = reponse as? HTTPURLResponse, http.statusCode != 200 {
            message = trf("Le serveur a refusé la liste (HTTP %lld).", http.statusCode)
            return
        }
        guard let recues = try? JSONDecoder().decode([Region].self, from: data) else {
            // Un 200 qui n'est pas du JSON. Le repli du front sert sa page pour
            // toute adresse qu'il ne connaît pas, et il la sert en 200 : un
            // backend trop ancien répond donc du HTML, sans erreur apparente.
            message = tr("Le serveur ne connaît pas cette adresse — backend à mettre à jour.")
            return
        }
        regions = recues.sorted {
            $0.nom.localizedStandardCompare($1.nom) == .orderedAscending
        }
    }

    // MARK: - Installation

    /// Télécharge les tuiles d'une région et sa configuration.
    ///
    /// Plusieurs régions peuvent coexister. Elles ne se fondent pas en un seul
    /// graphe — cela ne se fait qu'à la construction — mais un trajet cherche
    /// sa route dans chacune, si bien qu'en garder plusieurs couvre plus de
    /// terrain sans rien retélécharger.
    /// Le transfert est confié au système : deux gigaoctets ne descendent pas
    /// en gardant l'écran allumé sur l'app. iOS poursuit la session de fond
    /// app quittée, et réveille celle-ci quand le fichier est là.
    func install(_ region: Region) async {
        guard !downloading, !estInstallee(region) else { return }
        downloading = true
        progress = 0
        message = nil

        attentes[region.id] = region.nom
        // Les deux fichiers passent par la session de fond, y compris la
        // configuration pourtant minuscule : une requête ordinaire serait
        // suspendue si l'on bascule vers une autre app dans la seconde qui
        // suit, et plus rien ne démarrerait.
        for (url, nature) in [(region.config, Self.nConfig), (region.archive, Self.nArchive)] {
            let tache = session.downloadTask(with: url)
            // L'étiquette survit à la mort de l'app : c'est par elle qu'on
            // retrouve la région et la nature du fichier.
            tache.taskDescription = "\(region.id)|\(nature)"
            tache.resume()
        }
    }

    nonisolated static let nConfig = "config"
    nonisolated static let nArchive = "archive"

    /// Sépare l'étiquette d'une tâche en région et nature de fichier.
    nonisolated static func etiquette(_ brut: String?) -> (id: String, nature: String)? {
        guard let parts = brut?.components(separatedBy: "|"), parts.count == 2 else { return nil }
        return (parts[0], parts[1])
    }

    /// Régions dont les fichiers sont en cours de transfert, par identifiant.
    private var attentes: [String: String] {
        get {
            UserDefaults.standard.dictionary(forKey: Self.keyAttentes) as? [String: String] ?? [:]
        }
        set { UserDefaults.standard.set(newValue, forKey: Self.keyAttentes) }
    }

    /// Reprend la main sur une session de fond après un lancement à froid.
    ///
    /// Sans cela, une archive arrivée pendant que l'app était fermée resterait
    /// sur le disque sans que personne ne l'inscrive.
    func reprend() {
        let attendues = Set(attentes.keys)
        guard !attendues.isEmpty else { return }
        // Seules des valeurs simples traversent la fermeture : y capturer
        // l'objet entier reviendrait à le toucher hors du fil principal.
        session.getAllTasks { taches in
            let enCours = Set(taches.compactMap(\.taskDescription))
            let actif = taches.contains { $0.state == .running }
            // Une archive déjà déposée par le système, dont personne n'a vu
            // la fin : plus aucune tâche ne la porte, on l'inscrit maintenant.
            let finies = attendues.subtracting(enCours)
            Task { @MainActor in
                OfflineRouter.shared.downloading = actif
                for id in finies { OfflineRouter.shared.termine(id) }
            }
        }
    }

    /// Inscrit une région une fois ses deux fichiers arrivés.
    ///
    /// Appelée à chaque fichier reçu : tant que l'autre manque, elle ne fait
    /// rien. Les deux tâches arrivent dans un ordre que le système décide,
    /// et l'archive met bien plus longtemps que la configuration.
    fileprivate func termine(_ id: String) {
        let octets = Self.poids(Self.archivePath(id))
        guard octets > 0, Self.poids(Self.configPath(id)) > 0 else { return }

        var restantes = attentes
        let nom = restantes.removeValue(forKey: id) ?? id
        attentes = restantes
        downloading = !restantes.isEmpty

        moteurs[id] = nil
        if !installees.contains(where: { $0.id == id }) {
            installees.append(Installee(id: id, nom: nom, octets: octets))
            enregistre()
        }
        progress = 1
    }

    nonisolated static func poids(_ url: URL) -> Int {
        let attrs = try? FileManager.default.attributesOfItem(atPath: url.path)
        return (attrs?[.size] as? NSNumber)?.intValue ?? 0
    }

    fileprivate func echoue(_ id: String) {
        var restantes = attentes
        restantes.removeValue(forKey: id)
        attentes = restantes
        downloading = !restantes.isEmpty
        message = "Téléchargement interrompu."
        // Les deux fichiers partent ensemble : en garder un seul laisserait
        // une région à moitié installée, que rien ne viendrait compléter.
        try? FileManager.default.removeItem(at: Self.archivePath(id))
        try? FileManager.default.removeItem(at: Self.configPath(id))
    }

    /// Efface une région gardée.
    func remove(_ id: String) {
        try? FileManager.default.removeItem(at: Self.archivePath(id))
        try? FileManager.default.removeItem(at: Self.configPath(id))
        moteurs[id] = nil
        installees.removeAll { $0.id == id }
        enregistre()
    }

    /// Efface toutes les régions gardées.
    func removeAll() {
        for r in installees {
            try? FileManager.default.removeItem(at: Self.archivePath(r.id))
            try? FileManager.default.removeItem(at: Self.configPath(r.id))
        }
        moteurs.removeAll()
        installees.removeAll()
        enregistre()
    }

    /// Réécrit les chemins de la configuration engendrée sur le runner.
    ///
    /// Elle porte des chemins absolus (`/data/…`) qui n'existent pas sur
    /// l'appareil : seuls ceux-là changent, tout le reste — des dizaines de
    /// réglages de coût, de vitesse et de tri — est conservé tel quel.
    nonisolated static func corrige(config: Data, pour id: String) throws -> Data {
        guard
            var racine = try JSONSerialization.jsonObject(with: config) as? [String: Any],
            var mjolnir = racine["mjolnir"] as? [String: Any]
        else { throw CocoaError(.propertyListReadCorrupt) }

        mjolnir["tile_extract"] = archivePath(id).path
        // Le dossier de tuiles n'a pas lieu d'être : tout est dans l'archive,
        // et un chemin inexistant ferait échouer l'ouverture.
        mjolnir.removeValue(forKey: "tile_dir")
        mjolnir.removeValue(forKey: "admin")
        mjolnir.removeValue(forKey: "timezone")
        racine["mjolnir"] = mjolnir
        return try JSONSerialization.data(withJSONObject: racine)
    }

    // MARK: - Calcul

    /// Itinéraire routier entre deux points, calculé sur l'appareil.
    ///
    /// Chaque région gardée est interrogée jusqu'à ce que l'une réponde. Un
    /// paquet qui ne couvre qu'une des deux extrémités refuse la demande
    /// (« No suitable edges near location »), si bien que la première réponse
    /// vient forcément d'un paquet contenant le trajet entier.
    ///
    /// Ce qu'on ne peut pas faire, et qui n'a rien d'un oubli : router d'un
    /// paquet à l'autre. Chaque extrait a coupé ses routes à sa frontière, et
    /// seul un graphe bâti des deux ensemble les raccorde — d'où les zones.
    ///
    /// Renvoie `nil` si aucune région n'est gardée ou si aucune ne couvre le
    /// trajet : l'appelant garde alors son itinéraire précédent.
    func route(from: CLLocationCoordinate2D, to: CLLocationCoordinate2D) -> OfflineRoute? {
        routes(from: from, to: to, max: 1).first
    }

    /// Plusieurs itinéraires entre deux points, calculés sur l'appareil.
    ///
    /// Valhalla sait proposer des variantes — le paramètre `alternates` — et
    /// les renvoie à côté de l'itinéraire principal. Le croire incapable
    /// revenait à n'offrir qu'un seul trajet hors réseau là où le mode
    /// connecté en compare quinze.
    ///
    /// Le calcul reste local : demander des variantes coûte du temps de
    /// processeur, pas de la donnée. Cinq par défaut, parce qu'au-delà les
    /// tracés se ressemblent trop pour qu'un choix ait du sens, et que chacun
    /// se paie en secondes d'attente sur un téléphone.
    func routes(
        from: CLLocationCoordinate2D, to: CLLocationCoordinate2D, max nombre: Int = 5
    ) -> [OfflineRoute] {
        var demande: [String: Any] = [
            "locations": [
                ["lat": from.latitude, "lon": from.longitude],
                ["lat": to.latitude, "lon": to.longitude],
            ],
            "costing": "auto",
            "directions_options": [
                "units": "kilometers",
                // Les consignes suivent la langue de l'app : elles alimentent
                // le bandeau et les annonces vocales.
                "language": Self.langueValhalla,
            ],
        ]
        // `alternates` compte les variantes **en plus** du principal.
        if nombre > 1 { demande["alternates"] = nombre - 1 }

        guard let json = try? JSONSerialization.data(withJSONObject: demande) else { return [] }
        let texte = String(decoding: json, as: UTF8.self)

        for region in installees {
            guard let moteur = moteur(region.id) else { continue }
            let trouves = OfflineRoute.toutes(json: moteur.route(rawRequest: texte))
            if !trouves.isEmpty { return trouves }
        }
        return []
    }

    /// Langue des consignes, accordée à celle que le paquet a retenue.
    private static let langueValhalla: String = {
        switch Bundle.main.preferredLocalizations.first {
        case "es": return "es-ES"
        case "en": return "en-US"
        default: return "fr-FR"
        }
    }()

    /// Moteur d'une région, ouvert à la première demande et gardé ensuite.
    private func moteur(_ id: String) -> Valhalla? {
        if let deja = moteurs[id] { return deja }
        // Les fuseaux horaires que réclame Valhalla sont installés par le
        // paquet lui-même : son utilitaire de fichiers n'est pas exposé, donc
        // rien à faire ici.
        guard let neuf = try? Valhalla(configPath: Self.configPath(id).path) else { return nil }
        moteurs[id] = neuf
        return neuf
    }
}

/// Itinéraire renvoyé par Valhalla, décodé de sa réponse JSON.
struct OfflineRoute {
    let coordinates: [CLLocationCoordinate2D]
    let steps: [NavStep]
    let km: Double
    let minutes: Double
    /// L'itinéraire emprunte des sections à péage (`summary.has_toll`).
    let hasTolls: Bool

    /// Tous les itinéraires d'une réponse : le principal, puis les variantes.
    ///
    /// Valhalla place celles-ci dans `alternates`, chacune enveloppée dans son
    /// propre objet `trip` — d'où la lecture en deux temps plutôt qu'une
    /// simple boucle.
    static func toutes(json: String) -> [OfflineRoute] {
        guard
            let data = json.data(using: .utf8),
            let racine = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [] }

        var trips: [[String: Any]] = []
        if let principal = racine["trip"] as? [String: Any] { trips.append(principal) }
        for variante in (racine["alternates"] as? [[String: Any]]) ?? [] {
            if let trip = variante["trip"] as? [String: Any] { trips.append(trip) }
        }
        return trips.compactMap(OfflineRoute.init(trip:))
    }

    init?(json: String) {
        guard
            let data = json.data(using: .utf8),
            let racine = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let trip = racine["trip"] as? [String: Any]
        else { return nil }
        self.init(trip: trip)
    }

    private init?(trip: [String: Any]) {
        guard
            let legs = trip["legs"] as? [[String: Any]], !legs.isEmpty
        else { return nil }

        var points: [CLLocationCoordinate2D] = []
        var etapes: [NavStep] = []
        for leg in legs {
            let forme = (leg["shape"] as? String).map(OfflineRoute.decode) ?? []
            let depart = points.count
            points += forme
            for m in (leg["maneuvers"] as? [[String: Any]]) ?? [] {
                let debut = (m["begin_shape_index"] as? Int) ?? 0
                let position = min(points.count - 1, max(0, depart + debut))
                guard position >= 0, !points.isEmpty else { continue }
                etapes.append(
                    NavStep(
                        text: (m["instruction"] as? String) ?? "",
                        coord: points[position]))
            }
        }
        guard points.count >= 2 else { return nil }

        let resume = trip["summary"] as? [String: Any]
        coordinates = points
        steps = etapes
        km = (resume?["length"] as? Double) ?? 0
        minutes = ((resume?["time"] as? Double) ?? 0) / 60
        hasTolls = (resume?["has_toll"] as? Bool) ?? false
    }

    /// Décodage de la polyligne Valhalla, précision six décimales.
    ///
    /// Le format est celui de Google à un détail près — le facteur d'échelle —
    /// et c'est précisément ce détail qui placerait l'itinéraire à des
    /// centaines de kilomètres s'il était pris pour l'autre.
    static func decode(_ encoded: String) -> [CLLocationCoordinate2D] {
        var out: [CLLocationCoordinate2D] = []
        var index = encoded.startIndex
        var lat = 0
        var lon = 0
        func morceau() -> Int? {
            var resultat = 0
            var decalage = 0
            while index < encoded.endIndex {
                let valeur = Int(encoded[index].asciiValue ?? 0) - 63
                index = encoded.index(after: index)
                resultat |= (valeur & 0x1F) << decalage
                decalage += 5
                if valeur < 0x20 {
                    return (resultat & 1) != 0 ? ~(resultat >> 1) : (resultat >> 1)
                }
            }
            return nil
        }
        while index < encoded.endIndex {
            guard let dLat = morceau(), let dLon = morceau() else { break }
            lat += dLat
            lon += dLon
            out.append(
                CLLocationCoordinate2D(
                    latitude: Double(lat) / 1e6, longitude: Double(lon) / 1e6))
        }
        return out
    }
}

/// Réception des transferts de fond.
///
/// Le système appelle ces méthodes hors du fil principal, et jusque après un
/// lancement à froid : rien ici ne peut supposer que l'écran existe encore.
extension OfflineRouter: URLSessionDownloadDelegate {
    nonisolated func urlSession(
        _ session: URLSession, downloadTask: URLSessionDownloadTask,
        didFinishDownloadingTo location: URL
    ) {
        // Le fichier temporaire disparaît au retour de cette méthode : il faut
        // le déplacer ici même, sans attendre de repasser sur le fil principal.
        guard let (id, nature) = OfflineRouter.etiquette(downloadTask.taskDescription) else {
            return
        }
        if nature == OfflineRouter.nConfig {
            // La configuration porte les chemins du runner : elle est corrigée
            // ici, sur ce fil-ci, l'app pouvant ne plus être là ensuite.
            guard let brut = try? Data(contentsOf: location),
                let corrigee = try? OfflineRouter.corrige(config: brut, pour: id)
            else { return }
            try? corrigee.write(to: OfflineRouter.configPath(id))
        } else {
            let destination = OfflineRouter.archivePath(id)
            try? FileManager.default.removeItem(at: destination)
            try? FileManager.default.moveItem(at: location, to: destination)
        }
        Task { @MainActor in OfflineRouter.shared.termine(id) }
    }

    nonisolated func urlSession(
        _ session: URLSession, downloadTask: URLSessionDownloadTask,
        didWriteData bytesWritten: Int64, totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64
    ) {
        guard totalBytesExpectedToWrite > 0 else { return }
        let part = Double(totalBytesWritten) / Double(totalBytesExpectedToWrite)
        Task { @MainActor in OfflineRouter.shared.progress = part }
    }

    nonisolated func urlSession(
        _ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?
    ) {
        // Sans erreur, c'est `didFinishDownloadingTo` qui a déjà tout fait.
        guard error != nil,
            let (id, _) = OfflineRouter.etiquette(task.taskDescription)
        else { return }
        Task { @MainActor in OfflineRouter.shared.echoue(id) }
    }

    /// Tous les transferts remis : l'app peut rendre la main au système.
    nonisolated func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        Task { @MainActor in
            let rappel = OfflineRouter.rappelSysteme
            OfflineRouter.rappelSysteme = nil
            rappel?()
        }
    }
}
