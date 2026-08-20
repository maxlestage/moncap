import CoreLocation
import Foundation

/// Lecture d'une trace GPX — une randonnée exportée de Komoot, un parcours
/// vélo de Strava, un fichier posé dans l'app Fichiers.
///
/// Le format est du XML simple : des points `trkpt` (une trace enregistrée),
/// `rtept` (une route dessinée) ou `wpt` (des jalons), chacun portant `lat` et
/// `lon` en attributs. On prend les trois dans cet ordre de préférence : une
/// trace vaut mieux qu'une route, une route mieux que des jalons épars.
///
/// Tout se passe sur l'appareil — un fichier local, un parseur local : la
/// trace importée est utilisable en mode avion, ce qui tombe bien, c'est
/// souvent là qu'on en a besoin.
enum GPXImport {

    struct Trace {
        let nom: String?
        let points: [CLLocationCoordinate2D]
    }

    /// Lit un fichier choisi dans le sélecteur de documents. L'accès aux
    /// fichiers hors du bac à sable passe par la portée de sécurité — sans
    /// elle, la lecture échoue silencieusement sur un fichier iCloud.
    static func lit(_ url: URL) -> Trace? {
        let protege = url.startAccessingSecurityScopedResource()
        defer { if protege { url.stopAccessingSecurityScopedResource() } }
        guard let data = try? Data(contentsOf: url) else { return nil }
        return lit(data)
    }

    static func lit(_ data: Data) -> Trace? {
        let lecteur = Lecteur()
        let parseur = XMLParser(data: data)
        parseur.delegate = lecteur
        parseur.parse()
        let points =
            !lecteur.trk.isEmpty ? lecteur.trk : !lecteur.rte.isEmpty ? lecteur.rte : lecteur.wpt
        guard points.count >= 2 else { return nil }
        return Trace(nom: lecteur.nom, points: echantillonne(points, max: 4_000))
    }

    /// Une trace enregistrée à la seconde peut compter cent mille points ;
    /// la carte et le guidage n'en demandent pas tant.
    private static func echantillonne(
        _ points: [CLLocationCoordinate2D], max n: Int
    ) -> [CLLocationCoordinate2D] {
        guard points.count > n, n > 1 else { return points }
        let pas = Double(points.count - 1) / Double(n - 1)
        return (0..<n).map { points[Int((Double($0) * pas).rounded())] }
    }

    private final class Lecteur: NSObject, XMLParserDelegate {
        var trk: [CLLocationCoordinate2D] = []
        var rte: [CLLocationCoordinate2D] = []
        var wpt: [CLLocationCoordinate2D] = []
        var nom: String?
        private var litNom = false
        private var texteNom = ""

        func parser(
            _ parser: XMLParser, didStartElement elementName: String,
            namespaceURI: String?, qualifiedName qName: String?,
            attributes attributeDict: [String: String] = [:]
        ) {
            switch elementName {
            case "trkpt", "rtept", "wpt":
                guard let lat = Double(attributeDict["lat"] ?? ""),
                    let lon = Double(attributeDict["lon"] ?? ""),
                    abs(lat) <= 90, abs(lon) <= 180
                else { return }
                let c = CLLocationCoordinate2D(latitude: lat, longitude: lon)
                if elementName == "trkpt" {
                    trk.append(c)
                } else if elementName == "rtept" {
                    rte.append(c)
                } else {
                    wpt.append(c)
                }
            case "name" where nom == nil:
                litNom = true
                texteNom = ""
            default:
                break
            }
        }

        func parser(_ parser: XMLParser, foundCharacters string: String) {
            if litNom { texteNom += string }
        }

        func parser(
            _ parser: XMLParser, didEndElement elementName: String,
            namespaceURI: String?, qualifiedName qName: String?
        ) {
            if elementName == "name", litNom {
                litNom = false
                let propre = texteNom.trimmingCharacters(in: .whitespacesAndNewlines)
                if !propre.isEmpty { nom = propre }
            }
        }
    }
}
