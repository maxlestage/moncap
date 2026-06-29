import SwiftUI
import MapKit

struct ContentView: View {
    @StateObject private var location = LocationManager()
    private let api = APIClient()

    @State private var positions: [Position] = []
    @State private var stats: Stats?
    @State private var routeInfo: String?
    @State private var camera: MapCameraPosition = .automatic
    @State private var showList = false
    @State private var gpxFile: IdentifiableURL?

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                Map(position: $camera) {
                    UserAnnotation()
                    ForEach(positions) { p in
                        Marker(p.label, coordinate: .init(latitude: p.lat, longitude: p.lon))
                    }
                    if positions.count >= 2 {
                        MapPolyline(coordinates: positions.map {
                            .init(latitude: $0.lat, longitude: $0.lon)
                        })
                        .stroke(.blue, lineWidth: 3)
                    }
                }
                .frame(maxHeight: .infinity)

                controls
            }
            .navigationTitle("MonCap GPS")
            .toolbar {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    Button("Exporter GPX", systemImage: "square.and.arrow.up") {
                        Task { await exportGPX() }
                    }
                    .disabled(positions.isEmpty)
                    Button("Liste", systemImage: "list.bullet") { showList = true }
                }
            }
            .sheet(isPresented: $showList) {
                positionList
            }
            .sheet(item: $gpxFile) { file in
                ShareSheet(items: [file.url])
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
                    .disabled(location.coordinate == nil)
                Button("Itinéraire complet", action: { Task { await fullRoute() } })
                    .buttonStyle(.bordered)
                    .disabled(positions.count < 2)
            }
        }
        .padding()
    }

    /// Feuille listant les positions, avec suppression par glissement.
    private var positionList: some View {
        NavigationStack {
            List {
                if let s = stats {
                    Section("Statistiques") {
                        Text("\(s.count) positions · itinéraire \(String(format: "%.1f", s.total_km)) km")
                            .font(.subheadline)
                    }
                }
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

    /// Recharge la liste des positions et les statistiques depuis le backend.
    private func refresh() async {
        positions = (try? await api.positions()) ?? []
        stats = try? await api.stats()
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

    /// Calcule la distance totale et la durée de l'itinéraire.
    private func fullRoute() async {
        let points = positions.map { Coord(lat: $0.lat, lon: $0.lon) }
        if let r = try? await api.multiRoute(points) {
            routeInfo = String(format: "Itinéraire : %.1f km · %.0f min", r.total_km, r.duration_min)
        }
    }

    /// Télécharge l'export GPX et présente la feuille de partage.
    private func exportGPX() async {
        if let url = try? await api.exportGPX() {
            gpxFile = IdentifiableURL(url: url)
        }
    }
}
