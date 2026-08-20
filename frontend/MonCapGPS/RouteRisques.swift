import CoreLocation
import Foundation

/// Ce que l'itinéraire choisi va rencontrer : feux, air, pluie.
///
/// L'app savait déjà tout cela — la carte des feux, celle de l'air, la météo —
/// mais chacun dans son écran, et rien ne le disait au moment où l'on choisit
/// sa route. Le croisement se fait ici : trois requêtes légères au moment du
/// calcul, sur des sources que l'app paie déjà — nos deux relais pour les feux
/// et l'air, Open-Meteo sans clé pour la pluie.
///
/// Les seuils sont francs : un feu à moins de deux kilomètres du tracé, un air
/// « mauvais » ou pire à l'arrivée, une pluie à cinquante pour cent ou plus.
/// En deçà, se taire — une alerte qui crie pour rien apprend à être ignorée.
enum RouteRisques {

    /// Un point chaud, tel que le relais des feux le rend.
    struct Feu: Decodable {
        let lat: Double
        let lon: Double
    }

    /// Les points chauds dans le rectangle couvrant le trajet, marges
    /// comprises. Une seule requête pour toutes les variantes : elles longent
    /// le même pays.
    static func feux(couvrant coords: [CLLocationCoordinate2D]) async -> [Feu] {
        guard let premier = coords.first else { return [] }
        var o = premier.longitude, e = premier.longitude
        var su = premier.latitude, no = premier.latitude
        for c in coords {
            o = min(o, c.longitude)
            e = max(e, c.longitude)
            su = min(su, c.latitude)
            no = max(no, c.latitude)
        }
        let marge = 0.05
        var comps = URLComponents(
            url: APIClient().baseURL.appendingPathComponent("fires"),
            resolvingAgainstBaseURL: false)!
        comps.queryItems = [
            URLQueryItem(
                name: "bbox",
                value: String(
                    format: "%.3f,%.3f,%.3f,%.3f",
                    o - marge, su - marge, e + marge, no + marge))
        ]
        guard let url = comps.url,
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let feux = try? JSONDecoder().decode([Feu].self, from: data)
        else { return [] }
        return feux
    }

    /// Kilomètre du tracé où un feu passe à moins de deux kilomètres, s'il y
    /// en a un. Le tracé est échantillonné : au kilomètre près suffit pour
    /// dire « vers le km 34 », et un tracé de cinq cents kilomètres n'a pas à
    /// être parcouru point à point contre quatre mille foyers.
    static func kmDuPremierFeu(
        trace: [CLLocationCoordinate2D], feux: [Feu]
    ) -> Int? {
        guard !feux.isEmpty, trace.count >= 2 else { return nil }
        let pas = max(1, trace.count / 80)
        var parcouru = 0.0
        var precedent = trace[0]
        for i in stride(from: 0, to: trace.count, by: pas) {
            let p = trace[i]
            parcouru += CLLocation(latitude: precedent.latitude, longitude: precedent.longitude)
                .distance(from: CLLocation(latitude: p.latitude, longitude: p.longitude))
            precedent = p
            let ici = CLLocation(latitude: p.latitude, longitude: p.longitude)
            for f in feux {
                if ici.distance(from: CLLocation(latitude: f.lat, longitude: f.lon)) < 2_000 {
                    return Int((parcouru / 1_000).rounded())
                }
            }
        }
        return nil
    }

    /// Niveau d'air à l'arrivée (0 bon → 5 extrêmement mauvais), ou nil si la
    /// mesure manque.
    static func niveauAirArrivee(_ c: CLLocationCoordinate2D) async -> Int? {
        var comps = URLComponents(
            url: APIClient().baseURL.appendingPathComponent("air"),
            resolvingAgainstBaseURL: false)!
        comps.queryItems = [
            URLQueryItem(
                name: "bbox",
                value: String(
                    format: "%.3f,%.3f,%.3f,%.3f",
                    c.longitude - 0.15, c.latitude - 0.15,
                    c.longitude + 0.15, c.latitude + 0.15))
        ]
        guard let url = comps.url,
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let points = try? JSONDecoder().decode([AirPoint].self, from: data),
            let point = points.first(where: { $0.contient(c) }) ?? points.first
        else { return nil }
        return point.niveau
    }

    /// Probabilité de pluie (%) à l'arrivée, ou nil si la prévision manque.
    ///
    /// Open-Meteo, sans clé, comme la météo du bandeau. La prévision est
    /// horaire et en UTC : l'heure d'arrivée est arrondie à l'heure pleine la
    /// plus proche, ce qui est déjà plus fin que la pluie.
    static func pluieArrivee(
        _ c: CLLocationCoordinate2D, dans minutes: Double
    ) async -> Int? {
        var comps = URLComponents(string: "https://api.open-meteo.com/v1/forecast")!
        comps.queryItems = [
            URLQueryItem(name: "latitude", value: String(format: "%.4f", c.latitude)),
            URLQueryItem(name: "longitude", value: String(format: "%.4f", c.longitude)),
            URLQueryItem(name: "hourly", value: "precipitation_probability"),
            URLQueryItem(name: "forecast_days", value: "2"),
            URLQueryItem(name: "timezone", value: "UTC"),
        ]
        struct Reponse: Decodable {
            struct Horaire: Decodable {
                let time: [String]
                let precipitation_probability: [Int?]
            }
            let hourly: Horaire
        }
        guard let url = comps.url,
            let (data, resp) = try? await URLSession.shared.data(from: url),
            (resp as? HTTPURLResponse)?.statusCode == 200,
            let reponse = try? JSONDecoder().decode(Reponse.self, from: data)
        else { return nil }

        let arrivee = Date().addingTimeInterval(minutes * 60)
        let format = DateFormatter()
        format.dateFormat = "yyyy-MM-dd'T'HH:00"
        format.timeZone = TimeZone(identifier: "UTC")
        let cle = format.string(from: arrivee)
        guard let index = reponse.hourly.time.firstIndex(of: cle),
            index < reponse.hourly.precipitation_probability.count
        else { return nil }
        return reponse.hourly.precipitation_probability[index] ?? nil
    }
}
