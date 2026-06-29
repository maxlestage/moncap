import MapKit

/// Calcule un itinéraire routier réel (le plus simple) via MapKit.
enum RouteService {
    struct Result {
        let coordinates: [CLLocationCoordinate2D]
        let distanceKm: Double
        let minutes: Double
    }

    /// Itinéraire en voiture passant par les points dans l'ordre.
    /// `requestsAlternateRoutes = false` → on garde la route la plus directe.
    static func roadRoute(through points: [CLLocationCoordinate2D]) async -> Result? {
        guard points.count >= 2 else { return nil }
        var coords: [CLLocationCoordinate2D] = []
        var distance = 0.0
        var time = 0.0

        for i in 0..<(points.count - 1) {
            let request = MKDirections.Request()
            request.source = MKMapItem(placemark: MKPlacemark(coordinate: points[i]))
            request.destination = MKMapItem(placemark: MKPlacemark(coordinate: points[i + 1]))
            request.transportType = .automobile
            request.requestsAlternateRoutes = false

            guard let route = try? await MKDirections(request: request).calculate().routes.first else {
                continue
            }
            coords.append(contentsOf: route.polyline.coordinates)
            distance += route.distance
            time += route.expectedTravelTime
        }

        guard !coords.isEmpty else { return nil }
        return Result(coordinates: coords, distanceKm: distance / 1000, minutes: time / 60)
    }
}

extension MKPolyline {
    /// Les coordonnées de la polyligne.
    var coordinates: [CLLocationCoordinate2D] {
        var result = [CLLocationCoordinate2D](repeating: .init(), count: pointCount)
        getCoordinates(&result, range: NSRange(location: 0, length: pointCount))
        return result
    }
}
