import MapKit

/// Recherche de lieux / adresses via MapKit (MKLocalSearch), avec anti-rebond.
@MainActor
final class PlaceSearch: ObservableObject {
    @Published private(set) var results: [MKMapItem] = []

    /// Centre de recherche (position de l'utilisateur) pour prioriser les
    /// résultats proches.
    var center: CLLocationCoordinate2D?

    private var task: Task<Void, Never>?

    func search(_ text: String) {
        task?.cancel()
        let query = text.trimmingCharacters(in: .whitespacesAndNewlines)
        // Recherche agressive : démarre dès 2 caractères.
        guard query.count >= 2 else {
            results = []
            return
        }
        task = Task { [weak self] in
            // Anti-rebond court : la recherche démarre vite.
            try? await Task.sleep(nanoseconds: 150_000_000)
            guard let self, !Task.isCancelled else { return }

            func run(region: MKCoordinateRegion?) async -> [MKMapItem] {
                let request = MKLocalSearch.Request()
                request.naturalLanguageQuery = query
                request.resultTypes = [.address, .pointOfInterest]
                if let region { request.region = region }
                return (try? await MKLocalSearch(request: request).start())?.mapItems ?? []
            }

            // Grande zone (~1500 km) centrée sur l'utilisateur : priorise les
            // résultats proches sans exclure les lieux lointains.
            let near = self.center.map {
                MKCoordinateRegion(
                    center: $0, latitudinalMeters: 1_500_000, longitudinalMeters: 1_500_000)
            }
            var items = await run(region: near)
            guard !Task.isCancelled else { return }
            // Repli agressif : rien trouvé près → nouvelle recherche sans biais
            // régional, pour ramener les lieux éloignés.
            if items.isEmpty {
                items = await run(region: nil)
                guard !Task.isCancelled else { return }
            }
            self.results = items
        }
    }

    func clear() {
        task?.cancel()
        results = []
    }
}
