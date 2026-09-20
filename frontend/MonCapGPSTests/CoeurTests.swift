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

/// L'écriture des distances : des mètres sous le kilomètre, des kilomètres
/// au-delà. Une règle si simple qu'elle avait trois écritures différentes
/// dans l'app — dont une qui annonçait « 0 km » à quatre cents mètres de
/// l'arrivée.
final class DistanceTests: XCTestCase {
    func testSousLeKilometreEnMetres() {
        XCTAssertEqual(Distance.courte(0), "0 m")
        XCTAssertEqual(Distance.courte(7), "7 m")
        XCTAssertEqual(Distance.courte(400), "400 m")
        XCTAssertEqual(Distance.courte(999), "999 m")
    }

    func testAuDelaEnKilometres() {
        XCTAssertEqual(Distance.courte(1_000), "1.0 km")
        XCTAssertEqual(Distance.courte(1_234), "1.2 km")
        XCTAssertEqual(Distance.courte(12_345), "12.3 km")
    }

    /// 999,6 m arrondi à l'unité donnerait « 1000 m » : c'est un kilomètre.
    func testLaBasculeSuitLArrondi() {
        XCTAssertEqual(Distance.courte(999.6), "1.0 km")
        XCTAssertEqual(Distance.courte(999.4), "999 m")
    }

    /// Un relevé GPS qui recule ne fait pas écrire une distance négative.
    func testJamaisDeDistanceNegative() {
        XCTAssertEqual(Distance.courte(-5), "0 m")
    }

    func testDepuisDesKilometres() {
        XCTAssertEqual(Distance.courteKm(0.2), "200 m")
        XCTAssertEqual(Distance.courteKm(8.4), "8.4 km")
    }
}

/// Le type de voie : rue, avenue, boulevard…
///
/// La règle est volontairement timide : on n'écarte un résultat que lorsque
/// la demande **et** le résultat annoncent chacun un type, et que les deux se
/// contredisent. Ces tests couvrent surtout les cas où il ne faut *pas*
/// écarter — c'est là qu'une règle trop zélée fait des dégâts invisibles.
final class TypeDeVoieTests: XCTestCase {

    func testLesTypesCourantsSontReconnus() {
        XCTAssertEqual(TypeDeVoie.dans("Avenue Victor Hugo"), "avenue")
        XCTAssertEqual(TypeDeVoie.dans("rue de la Paix"), "rue")
        XCTAssertEqual(TypeDeVoie.dans("Boulevard Saint-Germain"), "boulevard")
        XCTAssertEqual(TypeDeVoie.dans("Impasse des Lilas"), "impasse")
    }

    /// Personne n'écrit « boulevard » en entier dans un champ de recherche.
    func testLesAbreviationsValentLesMotsEntiers() {
        XCTAssertEqual(TypeDeVoie.dans("av. Victor Hugo"), "avenue")
        XCTAssertEqual(TypeDeVoie.dans("Bd Saint-Germain"), "boulevard")
        XCTAssertEqual(TypeDeVoie.dans("all. des Tilleuls"), "allee")
    }

    /// Accents, majuscules et ponctuation ne changent pas le type.
    func testLaCasseEtLesAccentsNeComptentPas() {
        XCTAssertEqual(TypeDeVoie.dans("ALLÉE DES ROSES"), "allee")
        XCTAssertEqual(TypeDeVoie.dans("Allée des Roses"), "allee")
        XCTAssertEqual(TypeDeVoie.dans("Cité Bergère"), "cite")
    }

    /// Le numéro, et son suffixe, précèdent le type sans le masquer.
    func testLeNumeroEtSonSuffixeSontFranchis() {
        XCTAssertEqual(TypeDeVoie.dans("5 avenue Victor Hugo"), "avenue")
        XCTAssertEqual(TypeDeVoie.dans("5 bis avenue Victor Hugo"), "avenue")
        XCTAssertEqual(TypeDeVoie.dans("12 ter rue de la Paix"), "rue")
    }

    /// Les types en deux mots : « rond » seul ne dit rien, « point » non plus.
    func testLesTypesEnDeuxMots() {
        XCTAssertEqual(TypeDeVoie.dans("Rond-point de l'Europe"), "rond point")
        XCTAssertEqual(TypeDeVoie.dans("rond point de l'Europe"), "rond point")
        XCTAssertEqual(TypeDeVoie.dans("Lieu-dit Les Granges"), "lieu dit")
    }

    /// Le type doit venir en tête. Chercher le mot n'importe où serait
    /// tentant et faux : un café ne devient pas un boulevard.
    func testUnNomDeLieuNEstPasUnTypeDeVoie() {
        XCTAssertNil(TypeDeVoie.dans("Café du Boulevard"))
        XCTAssertNil(TypeDeVoie.dans("Hôtel de la Place"))
        XCTAssertNil(TypeDeVoie.dans("Gare de Lyon"))
        XCTAssertNil(TypeDeVoie.dans("Carrefour Market"))
        XCTAssertNil(TypeDeVoie.dans(""))
    }

    // MARK: - Ce qu'on écarte, et ce qu'on garde

    func testUneAvenueNeRamenePasUneRue() {
        XCTAssertTrue(
            TypeDeVoie.contredit(demande: "avenue Victor Hugo", candidat: "Rue Victor Hugo"))
        XCTAssertTrue(
            TypeDeVoie.contredit(demande: "12 bd Voltaire", candidat: "12 rue Voltaire"))
    }

    func testUnTypeIdentiqueNeContreditPas() {
        XCTAssertFalse(
            TypeDeVoie.contredit(demande: "avenue Victor Hugo", candidat: "Avenue Victor Hugo"))
        // L'abréviation d'un côté, le mot entier de l'autre : même voie.
        XCTAssertFalse(
            TypeDeVoie.contredit(demande: "av Victor Hugo", candidat: "Avenue Victor Hugo"))
    }

    /// Le silence n'est pas une contradiction : sans type d'un côté ou de
    /// l'autre, on ne sait pas, donc on ne jette pas.
    func testLeSilenceNeContreditPas() {
        XCTAssertFalse(
            TypeDeVoie.contredit(demande: "Victor Hugo", candidat: "Rue Victor Hugo"))
        XCTAssertFalse(
            TypeDeVoie.contredit(demande: "avenue Victor Hugo", candidat: "Victor Hugo"))
        XCTAssertFalse(
            TypeDeVoie.contredit(demande: "avenue Victor Hugo", candidat: "Boulangerie Hugo"))
    }

    /// Les moteurs rangent la voie tantôt dans le nom, tantôt dans l'adresse :
    /// un seul libellé qui s'accorde suffit à garder le résultat.
    func testUnSeulLibelleQuiSAccordeSuffit() {
        XCTAssertFalse(
            TypeDeVoie.contredit(
                demande: "avenue Victor Hugo",
                libelles: ["Rue Victor Hugo", "Avenue Victor Hugo, Paris"]))
        XCTAssertTrue(
            TypeDeVoie.contredit(
                demande: "avenue Victor Hugo",
                libelles: ["Rue Victor Hugo", "Rue Victor Hugo, 75016 Paris"]))
    }

    /// Un candidat sans aucun libellé exploitable est gardé : on n'écarte
    /// jamais faute d'information.
    func testAucunLibelleExploitableGardeLeResultat() {
        XCTAssertFalse(
            TypeDeVoie.contredit(demande: "avenue Victor Hugo", libelles: [nil, nil]))
        XCTAssertFalse(
            TypeDeVoie.contredit(
                demande: "avenue Victor Hugo", libelles: [nil, "Boulangerie Hugo"]))
    }
}

/// Chaque mode de déplacement a sa vue.
///
/// Les quatre partageaient une seule carte, donc un seul discours — celui de
/// la voiture, servi à tout le monde. Ces tests fixent la table des
/// différences : ce que chaque mode montre, et ce qu'il tait.
final class VueDuModeTests: XCTestCase {

    /// Les virages disent la simplicité d'un trajet en voiture. À pied on en
    /// fait trente sans y penser, et en tram ils n'existent pas.
    func testLesViragesNeComptentQuEnVoiture() {
        XCTAssertTrue(TravelMode.car.montreVirages)
        XCTAssertFalse(TravelMode.walk.montreVirages)
        XCTAssertFalse(TravelMode.bike.montreVirages)
        XCTAssertFalse(TravelMode.transit.montreVirages)
    }

    /// Le dénivelé décide du chemin à pied et à vélo ; au volant, quarante
    /// mètres de côte sont du bruit.
    func testLeDeniveleEstPourLesJambes() {
        XCTAssertTrue(TravelMode.walk.montreDenivele)
        XCTAssertTrue(TravelMode.bike.montreDenivele)
        XCTAssertFalse(TravelMode.car.montreDenivele)
        XCTAssertFalse(TravelMode.transit.montreDenivele)
    }

    /// Carburant et péages : la voiture seule les paie.
    func testSeuleLaVoiturePaieLeTrajet() {
        XCTAssertTrue(TravelMode.car.montreCout)
        for m in [TravelMode.walk, .bike, .transit] {
            XCTAssertFalse(m.montreCout, "\(m.rawValue) ne paie pas de carburant")
        }
    }

    /// Les étapes se chaînent partout sauf en transports, et les horaires
    /// n'appartiennent qu'aux transports.
    func testEtapesEtHorairesSExcluent() {
        for m in [TravelMode.walk, .bike, .car] {
            XCTAssertTrue(m.montreEtapes)
            XCTAssertFalse(m.montreHoraires)
        }
        XCTAssertFalse(TravelMode.transit.montreEtapes)
        XCTAssertTrue(TravelMode.transit.montreHoraires)
    }

    /// Chaque mode annonce ce que sa vue met en avant — quatre phrases
    /// distinctes, sinon le sélecteur changerait la carte sans rien expliquer.
    func testChaqueModeAnnonceSaVue() {
        let resumes = TravelMode.allCases.map(\.resumeDuMode)
        XCTAssertEqual(Set(resumes).count, TravelMode.allCases.count)
        for r in resumes { XCTAssertFalse(r.isEmpty) }
    }

    // MARK: - La ligne de résumé

    /// La voiture compte ses virages, le piéton non — et c'est la seule
    /// différence entre les deux lignes.
    func testLaLigneAPiedNAnnoncePasDeVirages() {
        let voiture = ResumeItineraire.ligne(
            mode: .car, minutes: 23, km: 18.4, virages: 12, arrivee: "14:05")
        let pied = ResumeItineraire.ligne(
            mode: .walk, minutes: 23, km: 18.4, virages: 12, arrivee: "14:05")
        XCTAssertTrue(voiture.contains("12"), voiture)
        XCTAssertFalse(pied.contains("12"), pied)
        XCTAssertTrue(pied.contains("14:05"), pied)
    }

    /// Sous le kilomètre, la distance se dit en mètres. Indifférent au
    /// volant, décisif à pied : « 0.8 km » pour huit cents mètres de marche
    /// n'est la façon de parler de personne.
    func testSousLeKilometreLaMarcheSeCompteEnMetres() {
        let pied = ResumeItineraire.ligne(
            mode: .walk, minutes: 9, km: 0.8, virages: 4, arrivee: "09:12")
        XCTAssertTrue(pied.contains("800 m"), pied)
        XCTAssertFalse(pied.contains("0.8 km"), pied)
    }

    /// Au-delà du kilomètre, on repasse aux kilomètres — même règle que
    /// partout ailleurs dans l'app.
    func testAuDelaDuKilometreOnRepasseAuxKilometres() {
        let velo = ResumeItineraire.ligne(
            mode: .bike, minutes: 41, km: 12.6, virages: 8, arrivee: "10:30")
        XCTAssertTrue(velo.contains("12.6 km"), velo)
    }

    /// Deux jambes ou deux roues, même discours : ni l'un ni l'autre ne
    /// compte ses virages, tous deux veulent la distance juste.
    func testLePietonEtLeCyclisteParlentPareil() {
        let pied = ResumeItineraire.ligne(
            mode: .walk, minutes: 30, km: 2.4, virages: 9, arrivee: "16:00")
        let velo = ResumeItineraire.ligne(
            mode: .bike, minutes: 30, km: 2.4, virages: 9, arrivee: "16:00")
        XCTAssertEqual(pied, velo)
    }
}
