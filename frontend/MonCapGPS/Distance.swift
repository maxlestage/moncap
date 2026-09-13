import Foundation

/// Les distances, écrites comme la route se parle : des **mètres** tant
/// qu'on est sous le kilomètre, des **kilomètres** au-delà.
///
/// L'app tenait deux discours. La manœuvre savait déjà le faire — « 400 m »
/// puis « 1.2 km » — mais le restant s'obstinait aux kilomètres : « 0.2 km
/// restants » à deux cents mètres de l'arrivée, et jusqu'à « 0 km » sur
/// l'écran verrouillé, qui n'arrondissait qu'à l'unité. Une seule règle
/// désormais, tenue ici, et à laquelle chaque écran se réfère.
///
/// Les cibles séparées — le widget Guidage, l'app montre — n'accèdent pas à
/// ce fichier : elles gardent leur copie de `distanceCourte`, mot pour mot
/// la même règle.
enum Distance {
    /// « 400 m » sous le kilomètre, « 1.2 km » au-dessus.
    ///
    /// Le point décimal est celui de `String(format:)` sans locale : c'est
    /// la forme employée partout dans l'app, et que la vitrine reproduit.
    static func courte(_ metres: Double) -> String {
        // Un GPS qui recule donnerait une distance négative : elle ne
        // s'écrit pas, elle se plafonne à zéro.
        let m = max(0, metres)
        // Le seuil porte sur la valeur **arrondie** : 999,6 m arrondi à
        // l'unité donnerait « 1000 m », qui est un kilomètre et doit
        // s'écrire comme tel.
        return m.rounded() < 1_000
            ? String(format: "%.0f m", m)
            : String(format: "%.1f km", m / 1_000)
    }

    /// Idem depuis des kilomètres — ce que porte le guidage.
    static func courteKm(_ km: Double) -> String { courte(km * 1_000) }

    /// La même règle, en phrase traduite : « 400 m restants ».
    static func restantsKm(_ km: Double) -> String {
        let m = max(0, km * 1_000)
        return m < 1_000
            ? String(format: NSLocalizedString("%.0f m restants", comment: ""), m)
            : String(format: NSLocalizedString("%.1f km restants", comment: ""), m / 1_000)
    }
}
