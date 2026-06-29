import SwiftUI
import MapKit

struct ContentView: View {
    @StateObject private var location = LocationManager()
    private let api = APIClient()

    @State private var positions: [Position] = []
    @State private var routeInfo: String?
    @State private var camera: MapCameraPosition = .automatic
    @State private var showList = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                Map(position: $camera) {
                    UserAnnotation()
                    ForEach(positions) { p in
                        Marker(p.label, coordinate: .init(latitude: p.lat, longitude: p.lon))
                    }
                }
                .frame(maxHeight: .infinity)

                controls
            }
            .navigationTitle("MonCap GPS")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Liste", systemImage: "list.bullet") { showList = true }
                }
            }
            .sheet(isPresented: $showList) {
                positionList
            }
            .task {
                location.start()
                await refresh()
            }
        }
    }

    /// Panneau de boutons sous la carte.
    private var controls: some View {
        VStack(spacing: 12) {
            if let info = routeInfo {
                Text(info).font(.headline)
            }

            HStack {
                Button("Enregistrer ma position", action: { Task { await saveCurrent() } })
                    .buttonStyle(.borderedProminent)
                Button("Trajet vers la 1re", action: { Task { await routeToFirst() } })
                    .buttonStyle(.bordered)
            }
            .disabled(location.coordinate == nil)
        }
        .padding()
    }

    /// Feuille listant les positions, avec suppression par glissement.
    private var positionList: some View {
        NavigationStack {
            List {
                ForEach(positions) { p in
                    VStack(alignment: .leading) {
                        Text(p.label).font(.headline)
                        Text(String(format: "%.4f, %.4f", p.lat, p.lon))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .onDelete(perform: deletePositions)
            }
            .navigationTitle("Positions")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("OK") { showList = false }
                }
            }
        }
    }

    /// Recharge la liste des positions depuis le backend.
    private func refresh() async {
        positions = (try? await api.positions()) ?? []
    }

    /// Enregistre la position courante sur le backend.
    private func saveCurrent() async {
        guard let c = location.coordinate else { return }
        let pos = NewPosition(lat: c.latitude, lon: c.longitude, label: "Point \(positions.count + 1)")
        _ = try? await api.add(pos)
        await refresh()
    }

    /// Supprime les positions sélectionnées (côté backend puis local).
    private func deletePositions(at offsets: IndexSet) {
        let ids = offsets.map { positions[$0].id }
        Task {
            for id in ids { try? await api.delete(id: id) }
            await refresh()
        }
    }

    /// Calcule le trajet de la position courante vers la première enregistrée.
    private func routeToFirst() async {
        guard let c = location.coordinate, let dest = positions.first else { return }
        let from = Coord(lat: c.latitude, lon: c.longitude)
        let to = Coord(lat: dest.lat, lon: dest.lon)
        if let r = try? await api.route(from: from, to: to) {
            routeInfo = String(format: "%.1f km · cap %.0f°", r.distance_km, r.bearing_deg)
        }
    }
}
