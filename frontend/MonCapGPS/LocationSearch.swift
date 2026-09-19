import MapKit

/// Recherche de lieux / adresses via MapKit (MKLocalSearch), avec anti-rebond.
@MainActor
final class PlaceSearch: ObservableObject {
    @Published private(set) var results: [MKMapItem] = []
    /// Vraie entre la saisie et la réponse : la feuille montre
    /// « Recherche… » au lieu d'un vide qui ressemble à une panne.
    @Published private(set) var enCours = false

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
            enCours = false
            return
        }
        enCours = true
        task = Task { [weak self] in
            // Anti-rebond court : la recherche démarre vite.
            try? await Task.sleep(nanoseconds: 150_000_000)
            guard let self, !Task.isCancelled else { return }

            func run(region: MKCoordinateRegion?) async -> [MKMapItem] {
                let request = MKLocalSearch.Request()
                request.naturalLanguageQuery = query
                request.resultTypes = [.address, .pointOfInterest]
                if let region { request.region = region }
                let bruts = (try? await MKLocalSearch(request: request).start())?.mapItems ?? []
                // MKLocalSearch cherche « au sens proche » : demandez une
                // avenue dans une ville qui n'a que la rue du même nom, il
                // rend la rue sans prévenir. Sur un plan l'erreur se voit ;
                // ici elle enchaîne sur le guidage et vous emmène ailleurs —
                // rue et avenue de même nom n'ont aucune raison d'être
                // voisines. On écarte donc ce qui contredit le type demandé,
                // et seulement cela : un résultat qui n'annonce aucun type
                // (commerce, lieu-dit) reste, l'absence n'est pas une
                // contradiction.
                return bruts.filter {
                    !TypeDeVoie.contredit(
                        demande: query,
                        libelles: [$0.name, $0.placemark.thoroughfare, $0.placemark.title])
                }
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
            // Toujours rien — le réseau manque, sans doute : l'annuaire
            // embarqué prend le relais. Les lieux nommés des régions
            // installées se trouvent en mode avion ; les adresses, elles,
            // restent l'affaire du réseau.
            if items.isEmpty, let centre = self.center {
                items = await LieuxHorsLigne.shared
                    .cherche(query, autour: centre)
                    .map { poi in
                        let item = MKMapItem(
                            placemark: MKPlacemark(coordinate: poi.coordinate))
                        item.name = poi.name
                        return item
                    }
                guard !Task.isCancelled else { return }
            }
            self.results = items
            self.enCours = false
        }
    }

    func clear() {
        task?.cancel()
        results = []
        enCours = false
    }
}
