import SwiftUI
import UIKit

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
