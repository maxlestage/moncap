import Foundation

/// Avatars disponibles (combis VW + voitures de F1).
/// L'`id` est partagé avec le backend/web.
enum Avatars {
    static let all = [
        "green", "orange", "blue", "mint",
        "ferrari", "alpine", "merc1", "merc2", "red1",
        "yarisGrey", "yarisWhite", "yarisBlue", "abarthWhite", "abarthRed",
        "vwOrange", "vwTeal", "vwMint", "vwGreen", "vwSurf",
    ]

    static let labels: [String: String] = [
        "green": "Vert", "orange": "Orange", "blue": "Bleu", "mint": "Menthe",
        "ferrari": "Ferrari", "alpine": "Alpine", "merc1": "Mercedes argent",
        "merc2": "Mercedes noire", "red1": "F1 rouge",
        "yarisGrey": "GR Yaris gris", "yarisWhite": "GR Yaris blanc",
        "yarisBlue": "GR Yaris bleu", "abarthWhite": "Abarth 500 blanc",
        "abarthRed": "Abarth 595 rouge",
        "vwOrange": "VW T2 orange", "vwTeal": "VW T2 turquoise",
        "vwMint": "VW T2 vert d'eau", "vwGreen": "VW T2 vert", "vwSurf": "VW T2 surf",
    ]

    /// Nom de l'image dans Assets.xcassets pour chaque `id`.
    private static let assets: [String: String] = [
        "green": "vanGreen", "orange": "vanOrange", "blue": "vanBlue", "mint": "vanMint",
        "ferrari": "carFerrari", "alpine": "carAlpine", "merc1": "carMerc1",
        "merc2": "carMerc2", "red1": "carRed1",
        "yarisGrey": "carYarisGrey", "yarisWhite": "carYarisWhite",
        "yarisBlue": "carYarisBlue", "abarthWhite": "carAbarthWhite",
        "abarthRed": "carAbarthRed",
        "vwOrange": "vwOrange", "vwTeal": "vwTeal", "vwMint": "vwMint",
        "vwGreen": "vwGreen", "vwSurf": "vwSurf",
    ]

    /// Nom de l'image dans Assets.xcassets (ex. "green" → "vanGreen").
    static func asset(_ id: String) -> String {
        assets[id] ?? "vanGreen"
    }
}
