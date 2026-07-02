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
        guard query.count >= 3 else {
            results = []
            return
        }
        task = Task { [weak self] in
            // Anti-rebond : on attend que l'utilisateur arrête de taper.
            try? await Task.sleep(nanoseconds: 300_000_000)
            guard let self, !Task.isCancelled else { return }

            let request = MKLocalSearch.Request()
            request.naturalLanguageQuery = query
            if let center = self.center {
                request.region = MKCoordinateRegion(
                    center: center,
                    latitudinalMeters: 40_000, longitudinalMeters: 40_000)
            }
            let response = try? await MKLocalSearch(request: request).start()
            guard !Task.isCancelled else { return }
            self.results = response?.mapItems ?? []
        }
    }

    func clear() {
        task?.cancel()
        results = []
    }
}
