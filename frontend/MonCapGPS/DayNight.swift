import CoreLocation
import Foundation

/// Jour ou nuit selon la position réelle du soleil (élévation solaire),
/// pour basculer automatiquement l'app en mode nuit.
enum DayNight {
    /// Vrai entre le coucher et le lever du soleil (soleil à plus de 3° sous
    /// l'horizon — crépuscule inclus dans le jour). Approximation largement
    /// suffisante pour un thème jour/nuit.
    static func isNight(lat: Double, lon: Double, date: Date = Date()) -> Bool {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC") ?? .current

        let dayOfYear = Double(cal.ordinality(of: .day, in: .year, for: date) ?? 172)
        let comps = cal.dateComponents([.hour, .minute, .second], from: date)
        let utcHours = Double(comps.hour ?? 12)
            + Double(comps.minute ?? 0) / 60
            + Double(comps.second ?? 0) / 3600

        // Déclinaison solaire approchée, puis élévation du soleil.
        let decl = -23.44 * cos((360.0 / 365.0) * (dayOfYear + 10) * .pi / 180) * .pi / 180
        let hourAngle = ((utcHours - 12) * 15 + lon) * .pi / 180
        let latRad = lat * .pi / 180
        let elevation = asin(sin(latRad) * sin(decl) + cos(latRad) * cos(decl) * cos(hourAngle))
        return elevation < -3.0 * .pi / 180
    }
}
