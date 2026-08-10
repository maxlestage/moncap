import SwiftUI

/// Le système a besoin d'un délégué d'application pour une seule chose ici :
/// pouvoir réveiller l'app quand un téléchargement de fond s'achève alors
/// qu'elle n'est plus lancée. Sans cela, une archive de deux gigaoctets
/// arrivée pendant la nuit ne serait jamais inscrite.
final class AppDelegate: NSObject, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        handleEventsForBackgroundURLSession identifier: String,
        completionHandler: @escaping () -> Void
    ) {
        // Gardé de côté et rappelé quand la session a fini de tout remettre,
        // faute de quoi iOS finit par refuser les réveils suivants.
        OfflineRouter.rappelSysteme = completionHandler
    }
}

@main
struct MonCapGPSApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        WindowGroup {
            ContentView()
                // Une archive a pu arriver app fermée : on reprend la main sur
                // la session de fond avant que l'écran ne se peigne.
                .task { OfflineRouter.shared.reprend() }
        }
    }
}
