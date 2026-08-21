import SwiftUI

/// L'app montre : un seul écran, la consigne de guidage en cours.
/// Tout l'état vient de l'iPhone via WatchConnectivity (LienTelephone).
@main
struct MontreApp: App {
    var body: some Scene {
        WindowGroup {
            MontreVue()
        }
    }
}
