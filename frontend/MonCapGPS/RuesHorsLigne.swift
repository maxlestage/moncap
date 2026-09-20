import CoreLocation
import Foundation

/// L'index des voies nommées d'une région, lu dans l'annexe `<id>.rues.zz`
/// téléchargée avec elle.
///
/// C'est le trou que le hors ligne gardait : on avait les tuiles, le calcul
/// d'itinéraire, les limites de vitesse et l'annuaire des lieux — mais taper
/// « 12 rue des Lilas » restait l'affaire du réseau. En mode avion, chercher
/// une pharmacie marchait et chercher son adresse ne marchait pas.
///
/// **Des voies, pas des numéros.** Un index de numéros pèserait des dizaines
/// de mégaoctets par région pour une précision dont un GPS n'a pas besoin :
/// on guide jusqu'à la rue, les cinquante derniers mètres se font à l'œil.
/// Le fichier reste donc de l'ordre du mégaoctet, à côté d'une archive de
/// tuiles qui en pèse deux cent soixante.
///
/// Le workflow des tuiles produit une ligne par ancrage — `nom⇥lat⇥lon` —
/// dédoublonnée par maille de deux kilomètres : une longue avenue garde
/// quelques ancrages le long de son tracé, ce qui est juste puisqu'elle
/// traverse réellement plusieurs quartiers.
actor RuesHorsLigne {
    static let shared = RuesHorsLigne()

    private struct Rue {
        let nom: String
        /// Le nom replié une fois pour toutes : sans accents ni casse. Replier
        /// à chaque frappe sur quatre-vingt mille rues coûtait le double.
        let cle: String
        let lat: Double
        let lon: Double
    }

    /// Rues chargées par région installée. L'ensemble `charges` retient aussi
    /// les régions sans annexe — une région bâtie avant que le workflow ne
    /// produise ce fichier n'en a pas, et ce n'est pas une panne.
    private var ruesParRegion: [String: [Rue]] = [:]
    private var charges: Set<String> = []

    /// À l'installation, la mise à jour ou l'effacement d'une région.
    func oublie(_ id: String) {
        ruesParRegion[id] = nil
        charges.remove(id)
    }

    /// Les voies dont le nom contient la requête, les plus proches d'abord.
    ///
    /// Le type de voie est respecté : demander « avenue Victor Hugo » ne
    /// ramène pas la rue du même nom, par `TypeDeVoie` — la règle posée pour
    /// la recherche en ligne vaut ici aussi, et elle compte davantage hors
    /// ligne où l'on n'a rien d'autre pour se rattraper.
    ///
    /// Le numéro tapé est ignoré pour la comparaison mais gardé pour
    /// l'affichage : « 12 rue des Lilas » cherche « rue des Lilas » et
    /// s'affiche avec son numéro, pour que le résultat ressemble à la demande.
    func cherche(
        _ texte: String, autour centre: CLLocationCoordinate2D, max: Int = 20
    ) async -> [POI] {
        let demande = texte.trimmingCharacters(in: .whitespacesAndNewlines)
        let sansNumero = Self.retireLeNumero(demande)
        let requete = Self.replie(sansNumero)
        // Deux lettres suffisaient pour un nom de commerce ; pour une rue
        // elles ramèneraient la moitié de la région. Trois est le seuil où la
        // recherche commence à vouloir dire quelque chose.
        guard requete.count >= 3 else { return [] }

        let ids = await MainActor.run { OfflineRouter.shared.installees.map(\.id) }
        for id in ids where !charges.contains(id) {
            charges.insert(id)
            ruesParRegion[id] = Self.litRues(OfflineRouter.ruesPath(id))
        }

        var trouves: [(d: Double, r: Rue)] = []
        for id in ids {
            for r in ruesParRegion[id] ?? [] where r.cle.contains(requete) {
                if TypeDeVoie.contredit(demande: sansNumero, candidat: r.nom) {
                    continue
                }
                let dx = (r.lon - centre.longitude) * Self.cosLat(centre.latitude)
                let dy = r.lat - centre.latitude
                trouves.append((dx * dx + dy * dy, r))
            }
        }
        trouves.sort { $0.d < $1.d }

        // Une rue rend plusieurs ancrages ; on n'en montre qu'un par nom, le
        // plus proche — c'est celui vers lequel on veut aller.
        var vus = Set<String>()
        var sortie: [POI] = []
        let numero = Self.numeroDe(demande)
        for t in trouves {
            guard vus.insert(t.r.cle).inserted else { continue }
            sortie.append(
                POI(
                    name: numero.isEmpty ? t.r.nom : "\(numero) \(t.r.nom)",
                    coordinate: CLLocationCoordinate2D(
                        latitude: t.r.lat, longitude: t.r.lon)))
            if sortie.count >= max { break }
        }
        return sortie
    }

    // MARK: - Lecture et mise en forme

    /// Le facteur de resserrement des longitudes à cette latitude.
    ///
    /// Sans lui, trier « au plus proche » sur des degrés bruts fausse tout aux
    /// latitudes françaises : un degré de longitude y vaut environ soixante-dix
    /// pour cent d'un degré de latitude, et une rue à l'est paraissait plus
    /// loin qu'une rue au nord à distance réelle égale.
    private static func cosLat(_ lat: Double) -> Double {
        cos(lat * .pi / 180)
    }

    private static func replie(_ t: String) -> String {
        t.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil)
    }

    /// « 12 bis rue des Lilas » → « rue des Lilas ».
    ///
    /// Interne plutôt que privé : c'est la seule logique délicate du fichier,
    /// et les tests doivent pouvoir l'atteindre.
    static func retireLeNumero(_ t: String) -> String {
        var mots = t.split(separator: " ").map(String.init)
        let suffixes: Set<String> = ["bis", "ter", "quater"]
        while let premier = mots.first,
            premier.allSatisfy(\.isNumber)
                || suffixes.contains(replie(premier))
        {
            mots.removeFirst()
        }
        return mots.joined(separator: " ")
    }

    /// Le numéro tapé, s'il y en a un — rendu tel quel pour l'affichage.
    /// Interne pour la même raison que `retireLeNumero`.
    static func numeroDe(_ t: String) -> String {
        let mots = t.split(separator: " ").map(String.init)
        guard let premier = mots.first, premier.allSatisfy(\.isNumber) else {
            return ""
        }
        let suffixes: Set<String> = ["bis", "ter", "quater"]
        if mots.count > 1, suffixes.contains(replie(mots[1])) {
            return "\(premier) \(mots[1])"
        }
        return premier
    }

    /// Une ligne par rue : `nom⇥lat⇥lon`, en DEFLATE brut.
    private static func litRues(_ url: URL) -> [Rue]? {
        guard let brut = try? Data(contentsOf: url),
            let donnees = try? (brut as NSData).decompressed(using: .zlib) as Data,
            let texte = String(data: donnees, encoding: .utf8)
        else { return nil }
        var out: [Rue] = []
        texte.enumerateLines { ligne, _ in
            let champs = ligne.components(separatedBy: "\t")
            guard champs.count >= 3, !champs[0].isEmpty,
                let lat = Double(champs[1]), let lon = Double(champs[2])
            else { return }
            out.append(
                Rue(nom: champs[0], cle: replie(champs[0]), lat: lat, lon: lon))
        }
        return out.isEmpty ? nil : out
    }
}
