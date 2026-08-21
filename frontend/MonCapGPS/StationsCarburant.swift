import CoreLocation
import SwiftUI

/// La station la moins chère **le long du trajet** — pas la plus proche :
/// celle qui vaut le détour, chiffres à l'appui.
///
/// Même open data que le prix moyen (prix-des-carburants,
/// data.economie.gouv.fr) : l'itinéraire est échantillonné en quelques
/// points, les stations à moins de dix kilomètres remontent, et la liste se
/// classe par prix. L'économie est dite sur un plein de 45 litres face au
/// prix moyen local — un chiffre qu'on comprend à la pompe.
struct StationCarburant: Identifiable {
    let id: String
    let nom: String
    let coord: CLLocationCoordinate2D
    let prix: Double
}

enum StationsCarburant {
    /// Les stations autour des points donnés, dédoublonnées, par prix
    /// croissant. Quatre points au plus sont interrogés : l'open data n'a
    /// pas à recevoir une rafale pour un seul trajet.
    static func autour(de points: [CLLocationCoordinate2D], type: String)
        async -> [StationCarburant]
    {
        var vues: [String: StationCarburant] = [:]
        for p in points.prefix(4) {
            var comps = URLComponents(
                string: "https://data.economie.gouv.fr/api/records/1.0/search/")!
            comps.queryItems = [
                URLQueryItem(
                    name: "dataset",
                    value: "prix-des-carburants-en-france-flux-instantane-v2"),
                URLQueryItem(name: "rows", value: "50"),
                URLQueryItem(
                    name: "geofilter.distance",
                    value: "\(p.latitude),\(p.longitude),10000"),
            ]
            guard let url = comps.url,
                let (data, _) = try? await URLSession.shared.data(from: url),
                let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                let records = json["records"] as? [[String: Any]]
            else { continue }
            for r in records {
                guard let fields = r["fields"] as? [String: Any] else { continue }
                let brut = fields["\(type)_prix"]
                let prix: Double? =
                    (brut as? Double)
                    ?? (brut as? String).flatMap {
                        Double($0.replacingOccurrences(of: ",", with: "."))
                    }
                guard let prix, prix > 0.4, prix < 3.5 else { continue }
                guard let geo = fields["geo_point_2d"] as? [Double], geo.count == 2
                else { continue }
                let id = (r["recordid"] as? String) ?? "\(geo[0]),\(geo[1])"
                let ville = (fields["ville"] as? String) ?? ""
                let adresse = (fields["adresse"] as? String) ?? ""
                let nom = [adresse.localizedCapitalized, ville.localizedCapitalized]
                    .filter { !$0.isEmpty }
                    .joined(separator: ", ")
                vues[id] = StationCarburant(
                    id: id,
                    nom: nom.isEmpty ? "⛽" : nom,
                    coord: CLLocationCoordinate2D(latitude: geo[0], longitude: geo[1]),
                    prix: prix)
            }
        }
        return vues.values.sorted { $0.prix < $1.prix }
    }
}

/// La liste des stations le long de l'itinéraire choisi, la moins chère en
/// tête, chacune ajoutable en étape (cinq minutes d'arrêt, le temps du
/// plein).
struct StationsCarburantView: View {
    let itineraire: [CLLocationCoordinate2D]
    let type: String
    /// Le prix moyen local — la référence de l'économie affichée.
    let prixMoyen: Double
    var onEtape: (StationCarburant) -> Void

    @Environment(\.dismiss) private var ferme
    @State private var stations: [StationCarburant] = []
    @State private var charge = true

    /// Litres du plein de référence pour parler d'économie en euros.
    private static let plein: Double = 45

    /// Quelques points répartis sur le tracé : début, tiers, deux tiers,
    /// arrivée — assez pour couvrir, sans mitrailler l'open data.
    private var echantillon: [CLLocationCoordinate2D] {
        guard !itineraire.isEmpty else { return [] }
        let pas = max(1, itineraire.count / 3)
        var pts = stride(from: 0, to: itineraire.count, by: pas).map { itineraire[$0] }
        if let dernier = itineraire.last { pts.append(dernier) }
        return Array(pts.prefix(4))
    }

    var body: some View {
        NavigationStack {
            List {
                if charge {
                    HStack(spacing: 10) {
                        ProgressView()
                        Text("On regarde les pompes le long du trajet…")
                            .foregroundStyle(.secondary)
                    }
                } else if stations.isEmpty {
                    Text("Aucune station le long de ce trajet — les prix ne couvrent que la France.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(stations.prefix(12)) { s in
                        HStack(alignment: .firstTextBaseline) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(verbatim: s.nom)
                                    .font(.subheadline.weight(.semibold))
                                    .lineLimit(1)
                                HStack(spacing: 6) {
                                    Text(verbatim: String(format: "%.3f €/L", s.prix))
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                    if prixMoyen > s.prix + 0.001 {
                                        Text(
                                            String(
                                                format: NSLocalizedString(
                                                    "≈ %.2f € gagnés sur un plein",
                                                    comment: ""),
                                                (prixMoyen - s.prix) * Self.plein))
                                            .font(.caption.weight(.semibold))
                                            .foregroundStyle(.green)
                                    }
                                }
                            }
                            Spacer()
                            Button {
                                onEtape(s)
                                ferme()
                            } label: {
                                Label("En étape", systemImage: "plus.circle.fill")
                                    .font(.caption.weight(.semibold))
                            }
                            .buttonStyle(.bordered)
                            .tint(.green)
                            .controlSize(.small)
                        }
                    }
                }
            }
            .navigationTitle("La moins chère sur la route")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Fermer") { ferme() }
                }
            }
        }
        .presentationBackground(Color(.systemBackground))
        .presentationDetents([.medium, .large])
        .task {
            stations = await StationsCarburant.autour(de: echantillon, type: type)
            charge = false
        }
    }
}
