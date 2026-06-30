import Foundation

/// Avatars « combi » disponibles. L'`id` est partagé avec le backend/web.
enum Avatars {
    static let all = ["green", "orange", "blue", "mint"]

    static let labels: [String: String] = [
        "green": "Vert", "orange": "Orange", "blue": "Bleu", "mint": "Menthe",
    ]

    /// Nom de l'image dans Assets.xcassets (ex. "green" → "vanGreen").
    static func asset(_ id: String) -> String {
        let valid = all.contains(id) ? id : "green"
        return "van" + valid.prefix(1).uppercased() + valid.dropFirst()
    }
}
