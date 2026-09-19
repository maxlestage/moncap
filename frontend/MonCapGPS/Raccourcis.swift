import AppIntents
import CoreLocation
import Foundation

/// Les raccourcis Siri : « Ramène-moi à la maison », « Au travail », et un
/// itinéraire vers n'importe quelle adresse dictée.
///
/// Tout passe par App Intents — pas d'extension, pas d'entitlement : les
/// intentions vivent dans l'app et s'annoncent d'elles-mêmes à Siri et à
/// l'app Raccourcis dès l'installation.
///
/// Le pont vers l'écran est double, parce que l'app peut être froide comme
/// tiède : l'intention écrit la demande dans UserDefaults **puis** poste une
/// notification. Ouverte, l'app entend la notification ; lancée à froid, elle
/// relit la demande à l'apparition de la vue. Une demande de plus de deux
/// minutes est ignorée — elle date d'un lancement qui n'a pas abouti.
enum GuidageDemande {
    static let cle = "moncap.guidage.attente"
    static let signal = Notification.Name("moncap.guidage")

    /// Dépose une demande de guidage et réveille l'app si elle écoute.
    static func demande(lat: Double, lon: Double, demarre: Bool) {
        UserDefaults.standard.set(
            [
                "lat": lat, "lon": lon, "demarre": demarre,
                "quand": Date().timeIntervalSince1970,
            ] as [String: Any], forKey: cle)
        NotificationCenter.default.post(name: signal, object: nil)
    }

    /// Relève la demande en attente, si elle est encore fraîche.
    static func releve() -> (lat: Double, lon: Double, demarre: Bool)? {
        guard let brut = UserDefaults.standard.dictionary(forKey: cle) else { return nil }
        UserDefaults.standard.removeObject(forKey: cle)
        guard let lat = brut["lat"] as? Double, let lon = brut["lon"] as? Double,
            let quand = brut["quand"] as? Double,
            Date().timeIntervalSince1970 - quand < 120
        else { return nil }
        return (lat, lon, brut["demarre"] as? Bool ?? false)
    }
}

/// « Dis Siri, ramène-moi à la maison » : le guidage démarre vers le
/// domicile enregistré dans les favoris — hors ligne compris, la position
/// du domicile étant déjà sur l'appareil.
struct RentrerMaison: AppIntent {
    static var title: LocalizedStringResource = "Rentrer à la maison"
    static var openAppWhenRun = true

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        guard let maison = Session.home else {
            return .result(
                dialog: "Définis d'abord ton domicile dans les favoris de MonCap GPS.")
        }
        GuidageDemande.demande(lat: maison.lat, lon: maison.lon, demarre: true)
        return .result(dialog: "C'est parti vers la maison.")
    }
}

/// « Dis Siri, emmène-moi au travail » : même geste, vers l'autre favori.
struct AllerAuTravail: AppIntent {
    static var title: LocalizedStringResource = "Aller au travail"
    static var openAppWhenRun = true

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        guard let travail = Session.work else {
            return .result(
                dialog: "Définis d'abord ton travail dans les favoris de MonCap GPS.")
        }
        GuidageDemande.demande(lat: travail.lat, lon: travail.lon, demarre: true)
        return .result(dialog: "C'est parti vers le travail.")
    }
}

/// Itinéraire vers une adresse libre, dictée ou tapée dans Raccourcis. Le
/// géocodage demande le réseau : hors ligne, Siri le dit au lieu d'échouer
/// en silence.
struct NaviguerVers: AppIntent {
    static var title: LocalizedStringResource = "Itinéraire vers"
    static var openAppWhenRun = true

    @Parameter(title: "Destination")
    var destination: String

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        // Le premier résultat qui ne contredit pas le type de voie dicté :
        // à la voix, « avenue » et « rue » se distinguent parfaitement, et
        // partir vers l'autre bout de la commune sans rien dire serait pire
        // ici qu'ailleurs — personne ne regarde l'écran.
        let trouves = (try? await CLGeocoder().geocodeAddressString(destination)) ?? []
        guard
            let lieu = trouves.first(where: {
                !TypeDeVoie.contredit(
                    demande: destination, libelles: [$0.thoroughfare, $0.name])
            }),
            let c = lieu.location?.coordinate
        else {
            return .result(
                dialog:
                    "Je n'ai pas trouvé cette adresse — il faut du réseau pour chercher un nom de lieu.")
        }
        GuidageDemande.demande(lat: c.latitude, lon: c.longitude, demarre: false)
        return .result(dialog: "Voilà les itinéraires.")
    }
}

/// Les phrases que Siri reconnaît sans configuration. Les intentions à
/// paramètre libre (« Itinéraire vers ») restent disponibles dans l'app
/// Raccourcis, où le texte se dicte ou se tape.
struct MonCapRaccourcis: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: RentrerMaison(),
            phrases: [
                "Ramène-moi à la maison avec \(.applicationName)",
                "Rentrer à la maison avec \(.applicationName)",
            ],
            shortTitle: "Maison",
            systemImageName: "house.fill")
        AppShortcut(
            intent: AllerAuTravail(),
            phrases: [
                "Emmène-moi au travail avec \(.applicationName)",
                "Aller au travail avec \(.applicationName)",
            ],
            shortTitle: "Travail",
            systemImageName: "briefcase.fill")
    }
}
