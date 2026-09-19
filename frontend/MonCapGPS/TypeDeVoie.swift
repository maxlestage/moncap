import Foundation

/// Le type de voie d'une adresse : rue, avenue, boulevard, impasse…
///
/// Pourquoi ce fichier existe : les moteurs d'adresses (MKLocalSearch,
/// CLGeocoder) cherchent « au sens proche ». Demandez « avenue Victor Hugo »
/// dans une ville qui n'a qu'une *rue* Victor Hugo, ils rendent la rue sans
/// prévenir. Sur un plan, l'erreur se voit ; dans un GPS qui enchaîne sur le
/// guidage, elle vous emmène ailleurs — parfois à l'autre bout de la commune,
/// car rue et avenue de même nom n'ont aucune raison d'être voisines.
///
/// La règle tenue ici : **quand la demande annonce un type de voie, un
/// résultat qui en annonce un autre est écarté.** Un résultat qui n'annonce
/// rien (un commerce, un lieu-dit, un point d'intérêt) est gardé : l'absence
/// d'information n'est pas une contradiction.
enum TypeDeVoie {

    // MARK: - Le vocabulaire

    /// Formes rencontrées → forme canonique. Les abréviations comptent autant
    /// que les mots entiers : personne n'écrit « boulevard » en entier dans un
    /// champ de recherche, et « bd » doit valoir « boulevard ».
    ///
    /// Les clés sont déjà normalisées (minuscules, sans accent, sans point).
    private static let vocabulaire: [String: String] = {
        let familles: [String: [String]] = [
            "rue": ["rue", "r"],
            "avenue": ["avenue", "av", "ave", "aven"],
            "boulevard": ["boulevard", "bd", "bld", "blvd", "boul", "bvd"],
            "place": ["place", "pl"],
            "chemin": ["chemin", "ch", "chem"],
            "route": ["route", "rte"],
            "impasse": ["impasse", "imp"],
            "allee": ["allee", "allees", "all"],
            "quai": ["quai", "quais"],
            "cours": ["cours", "crs"],
            "square": ["square", "sq"],
            "passage": ["passage", "pass", "psg"],
            "sentier": ["sentier", "sente"],
            "voie": ["voie"],
            "esplanade": ["esplanade", "espl"],
            "rond point": ["rond point", "rondpoint", "rd pt", "rdpt"],
            "faubourg": ["faubourg", "fbg", "fg"],
            "villa": ["villa"],
            "cite": ["cite"],
            "hameau": ["hameau", "ham"],
            "lieu dit": ["lieu dit", "lieudit"],
            "traverse": ["traverse"],
            "montee": ["montee"],
            "descente": ["descente"],
            "promenade": ["promenade"],
            "parvis": ["parvis"],
            "mail": ["mail"],
            "clos": ["clos"],
            "domaine": ["domaine"],
            "residence": ["residence", "res"],
            "rampe": ["rampe"],
            "galerie": ["galerie"],
            "peripherique": ["peripherique", "periph"],
        ]
        var table: [String: String] = [:]
        for (canon, formes) in familles {
            for forme in formes { table[forme] = canon }
        }
        return table
    }()

    /// Mots qui séparent le numéro du type de voie : « 5 bis avenue… ».
    private static let apresLeNumero: Set<String> = [
        "bis", "ter", "quater", "quinquies", "a", "b", "c", "d",
    ]

    // MARK: - Lecture

    /// Le type de voie annoncé par un libellé d'adresse, ou `nil`.
    ///
    /// Le type doit venir **en tête**, après un éventuel numéro : c'est ainsi
    /// que s'écrit une adresse française. Chercher le mot n'importe où dans la
    /// chaîne serait tentant et faux — « Café du Boulevard » deviendrait un
    /// boulevard, et « Hôtel de la Place » une place.
    static func dans(_ texte: String) -> String? {
        let mots = decoupe(texte)
        var i = 0
        // On saute le numéro et son suffixe : « 5 », « 5 bis », « 5-7 ».
        while i < mots.count,
            mots[i].allSatisfy(\.isNumber) || apresLeNumero.contains(mots[i])
        {
            i += 1
        }
        guard i < mots.count else { return nil }
        // Les types en deux mots d'abord (« rond point », « lieu dit »),
        // sinon « rond » seul ne dirait rien et « point » non plus.
        if i + 1 < mots.count,
            let deux = vocabulaire["\(mots[i]) \(mots[i + 1])"]
        {
            return deux
        }
        return vocabulaire[mots[i]]
    }

    /// Le libellé `candidat` contredit-il le type demandé ?
    ///
    /// Faux quand la demande n'annonce aucun type, quand le candidat n'en
    /// annonce aucun, ou quand les deux s'accordent. Vrai seulement quand les
    /// deux parlent, et se contredisent — le seul cas où l'on peut écarter
    /// sans risquer de jeter un bon résultat.
    static func contredit(demande: String, candidat: String) -> Bool {
        guard let attendu = dans(demande) else { return false }
        guard let trouve = dans(candidat) else { return false }
        return attendu != trouve
    }

    /// Le candidat, décrit par plusieurs libellés (nom, voie, adresse
    /// complète), contredit-il la demande ? Un seul libellé qui s'accorde
    /// suffit à l'innocenter : les moteurs rangent la voie tantôt dans le
    /// nom, tantôt dans l'adresse.
    static func contredit(demande: String, libelles: [String?]) -> Bool {
        guard dans(demande) != nil else { return false }
        let connus = libelles.compactMap { $0 }.filter { dans($0) != nil }
        guard !connus.isEmpty else { return false }
        return connus.allSatisfy { contredit(demande: demande, candidat: $0) }
    }

    // MARK: - Normalisation

    /// Découpe en mots normalisés : minuscules, sans accent, sans ponctuation.
    ///
    /// Les tirets deviennent des espaces : « rond-point » et « rond point »
    /// sont le même mot pour qui tape vite, et « Saint-Étienne » ne change pas
    /// de sens en perdant son tiret puisqu'on ne lit que le premier mot.
    private static func decoupe(_ texte: String) -> [String] {
        let sansAccent =
            texte
            .folding(options: [.diacriticInsensitive, .caseInsensitive], locale: Locale(identifier: "fr"))
        let aplati = sansAccent.map { c -> Character in
            c.isLetter || c.isNumber ? c : " "
        }
        return String(aplati).split(separator: " ").map(String.init)
    }
}
