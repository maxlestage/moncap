import Foundation
import Network

/// L'état **réel** du réseau, observé en continu.
///
/// Le réglage « Naviguer hors ligne » est un choix ; ceci est un fait. Les
/// deux alimentent le badge de la carte : on doit pouvoir être sûr qu'on
/// roule sans réseau — qu'on l'ait voulu, ou qu'une vallée l'ait décidé.
@MainActor
final class Reseau: ObservableObject {
    static let partage = Reseau()

    /// Vrai quand aucun chemin réseau n'est utilisable (mode avion, zone
    /// blanche). `satisfied` seul fait foi : un chemin « en attente » ne
    /// transporte rien.
    @Published private(set) var coupe = false

    private let moniteur = NWPathMonitor()

    private init() {
        moniteur.pathUpdateHandler = { [weak self] chemin in
            let coupe = chemin.status != .satisfied
            Task { @MainActor in
                guard let self, self.coupe != coupe else { return }
                self.coupe = coupe
            }
        }
        moniteur.start(queue: DispatchQueue(label: "moncap.reseau"))
    }
}
