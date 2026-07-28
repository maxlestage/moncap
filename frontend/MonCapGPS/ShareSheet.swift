import SwiftUI
import UIKit

/// Partage fiable, à la demande.
///
/// Historique : les partages (ETA, lien de suivi, GPX) étaient déclenchés en
/// affectant un état `@State` relié à un `.sheet(item:)` sur la vue racine.
/// Comme l'action part souvent d'une feuille déjà présentée (menu « Lieux »,
/// barre d'outils…), iOS refusait d'empiler une seconde feuille sur la
/// racine : rien ne s'affichait. `Sharer` contourne ça en présentant la
/// feuille de partage native **impérativement** depuis le contrôleur le plus
/// haut réellement visible.
@MainActor
enum Sharer {
    /// Présente la feuille de partage système avec les éléments donnés
    /// (texte, URL, fichier…), depuis le contrôleur le plus haut à l'écran.
    static func present(_ items: [Any]) {
        guard !items.isEmpty, let host = topViewController() else { return }
        let vc = UIActivityViewController(activityItems: items, applicationActivities: nil)
        // iPad : la feuille est un popover, il faut une ancre sinon plantage.
        if let pop = vc.popoverPresentationController {
            pop.sourceView = host.view
            pop.sourceRect = CGRect(
                x: host.view.bounds.midX, y: host.view.bounds.midY, width: 0, height: 0)
            pop.permittedArrowDirections = []
        }
        host.present(vc, animated: true)
    }

    /// Contrôleur le plus haut de la scène active (au-dessus des feuilles déjà
    /// présentées), pour pouvoir présenter par-dessus tout ce qui est visible.
    private static func topViewController() -> UIViewController? {
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
            ?? UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
        let root = scene?.windows.first { $0.isKeyWindow }?.rootViewController
            ?? scene?.windows.first?.rootViewController
        var top = root
        while let presented = top?.presentedViewController {
            top = presented
        }
        return top
    }
}

/// Enveloppe `UIActivityViewController` pour partager des fichiers.
struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

/// Une URL rendue `Identifiable` pour piloter une feuille modale.
struct IdentifiableURL: Identifiable {
    let id = UUID()
    let url: URL
}

/// Un texte rendu `Identifiable` pour piloter une feuille de partage.
struct IdentifiableText: Identifiable {
    let id = UUID()
    let text: String
}
