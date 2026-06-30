import Foundation

/// Avatars disponibles (combis VW + voitures de F1).
/// L'`id` est partagé avec le backend/web.
enum Avatars {
    static let all = [
        "green", "orange", "blue", "mint",
        "ferrari", "alpine", "merc1", "merc2", "red1",
    ]

    static let labels: [String: String] = [
        "green": "Vert", "orange": "Orange", "blue": "Bleu", "mint": "Menthe",
        "ferrari": "Ferrari", "alpine": "Alpine", "merc1": "Mercedes argent",
        "merc2": "Mercedes noire", "red1": "F1 rouge",
    ]

    /// Nom de l'image dans Assets.xcassets pour chaque `id`.
    private static let assets: [String: String] = [
        "green": "vanGreen", "orange": "vanOrange", "blue": "vanBlue", "mint": "vanMint",
        "ferrari": "carFerrari", "alpine": "carAlpine", "merc1": "carMerc1",
        "merc2": "carMerc2", "red1": "carRed1",
    ]

    /// Nom de l'image dans Assets.xcassets (ex. "green" → "vanGreen").
    static func asset(_ id: String) -> String {
        assets[id] ?? "vanGreen"
    }
}
