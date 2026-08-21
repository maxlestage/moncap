import XCTest

@testable import MonCapGPS

/// Les premiers tests de l'app : la logique pure qui porte l'argent et la
/// confiance — le barème fiscal, le décodage des trajets, la validation des
/// identifiants, le choix de la fonction de coût. Pas d'interface ici : ce
/// qui se teste sans écran se teste d'abord.
final class BaremeFiscalTests: XCTestCase {
    func testTrancheBasse() {
        // 4 000 km en 5 CV : d × 0,636.
        XCTAssertEqual(
            BaremeFiscal.montant(km: 4000, puissance: .cv5, electrique: false),
            4000 * 0.636, accuracy: 0.01)
    }

    func testTrancheMoyenne() {
        // 10 000 km en 5 CV : d × 0,357 + 1 395.
        XCTAssertEqual(
            BaremeFiscal.montant(km: 10000, puissance: .cv5, electrique: false),
            10000 * 0.357 + 1395, accuracy: 0.01)
    }

    func testTrancheHaute() {
        // 25 000 km en 5 CV : d × 0,427.
        XCTAssertEqual(
            BaremeFiscal.montant(km: 25000, puissance: .cv5, electrique: false),
            25000 * 0.427, accuracy: 0.01)
    }

    func testMajorationElectrique() {
        let thermique = BaremeFiscal.montant(km: 8000, puissance: .cv4, electrique: false)
        let electrique = BaremeFiscal.montant(km: 8000, puissance: .cv4, electrique: true)
        XCTAssertEqual(electrique, thermique * 1.2, accuracy: 0.01)
    }

    func testBornesDesTranches() {
        // 5 000 km reste dans la tranche basse, 5 001 passe dans la moyenne.
        XCTAssertEqual(
            BaremeFiscal.montant(km: 5000, puissance: .cv3, electrique: false),
            5000 * 0.529, accuracy: 0.01)
        XCTAssertEqual(
            BaremeFiscal.montant(km: 5001, puissance: .cv3, electrique: false),
            5001 * 0.316 + 1065, accuracy: 0.01)
    }

    func testToutesLesPuissancesRendentUnMontant() {
        for p in BaremeFiscal.Puissance.allCases {
            XCTAssertGreaterThan(
                BaremeFiscal.montant(km: 1000, puissance: p, electrique: false), 0)
        }
    }
}

final class TripTests: XCTestCase {
    private func trajet(_ polyline: String) -> Trip {
        Trip(
            id: 1, label: "Essai", distance_km: 1, duration_min: 1,
            polyline: polyline, created_at: 0)
    }

    func testTraceDecodee() {
        let coords = trajet("48.1,-1.6;48.2,-1.5").coordinates
        XCTAssertEqual(coords.count, 2)
        XCTAssertEqual(coords[0].lat, 48.1, accuracy: 0.0001)
        XCTAssertEqual(coords[1].lon, -1.5, accuracy: 0.0001)
    }

    func testTraceCabosseeIgnoreLesMorceauxInvalides() {
        // Un morceau sans virgule, un morceau à trois valeurs : seuls les
        // couples valides survivent — jamais de plantage sur une donnée sale.
        let coords = trajet("abc;48.2,-1.5;1,2,3;;x,y").coordinates
        XCTAssertEqual(coords.count, 1)
        XCTAssertEqual(coords[0].lat, 48.2, accuracy: 0.0001)
    }

    func testTraceVide() {
        XCTAssertTrue(trajet("").coordinates.isEmpty)
    }
}

@MainActor
final class AuthTests: XCTestCase {
    func testNormaliseLesEspaces() {
        XCTAssertEqual(AuthStore.normalize("  paul@mail.fr \n"), "paul@mail.fr")
    }

    func testEmailsValides() {
        XCTAssertTrue(AuthStore.isValidEmail("paul@mail.fr"))
        XCTAssertTrue(AuthStore.isValidEmail("p.l+tag@sous.domaine.org"))
    }

    func testEmailsInvalides() {
        XCTAssertFalse(AuthStore.isValidEmail("paul"))
        XCTAssertFalse(AuthStore.isValidEmail("paul@mail"))
        XCTAssertFalse(AuthStore.isValidEmail("paul mail@x.fr"))
        XCTAssertFalse(AuthStore.isValidEmail("@mail.fr"))
    }
}

final class TravelModeTests: XCTestCase {
    override func tearDown() {
        UserDefaults.standard.removeObject(forKey: "moncap.moto")
        UserDefaults.standard.removeObject(forKey: "moncap.utilitaire")
        super.tearDown()
    }

    func testVoitureParDefaut() {
        UserDefaults.standard.removeObject(forKey: "moncap.moto")
        UserDefaults.standard.removeObject(forKey: "moncap.utilitaire")
        XCTAssertEqual(TravelMode.car.costing, "auto")
    }

    func testModeMoto() {
        UserDefaults.standard.set(true, forKey: "moncap.moto")
        XCTAssertEqual(TravelMode.car.costing, "motorcycle")
    }

    func testUtilitaireGagneSurLaMoto() {
        // Les ponts bas n'attendent pas : le gabarit prime.
        UserDefaults.standard.set(true, forKey: "moncap.moto")
        UserDefaults.standard.set(true, forKey: "moncap.utilitaire")
        XCTAssertEqual(TravelMode.car.costing, "truck")
    }

    func testLesAutresModesNeBougentPas() {
        XCTAssertEqual(TravelMode.walk.costing, "pedestrian")
        XCTAssertEqual(TravelMode.bike.costing, "bicycle")
    }
}

final class OptionsDeCoutTests: XCTestCase {
    override func tearDown() {
        UserDefaults.standard.removeObject(forKey: "moncap.motoPlaisir")
        UserDefaults.standard.removeObject(forKey: "moncap.util.hauteur")
        super.tearDown()
    }

    func testGabaritUtilitaireDeSecours() {
        // Réglage jamais touché : les dimensions d'un grand fourgon.
        UserDefaults.standard.removeObject(forKey: "moncap.util.hauteur")
        let options = OfflineRouter.optionsDeCout("truck")
        let truck = options?["truck"] as? [String: Any]
        XCTAssertEqual(truck?["height"] as? Double, 2.7)
    }

    func testMotoSansPlaisirSansOptions() {
        UserDefaults.standard.set(false, forKey: "moncap.motoPlaisir")
        XCTAssertNil(OfflineRouter.optionsDeCout("motorcycle"))
    }

    func testMotoPlaisirFuitLAutoroute() {
        UserDefaults.standard.set(true, forKey: "moncap.motoPlaisir")
        let options = OfflineRouter.optionsDeCout("motorcycle")
        let moto = options?["motorcycle"] as? [String: Any]
        XCTAssertEqual(moto?["use_trails"] as? Double, 0.8)
    }
}
