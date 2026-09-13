import CarPlay
import Combine
import CoreLocation
import MapLibre
import UIKit

/// L'écran CarPlay : la carte de l'app sur l'écran de la voiture, le tracé
/// du guidage en cours, et la consigne du moment dans un bandeau.
///
/// Dormant tant qu'Apple n'a pas accordé l'entitlement CarPlay
/// (`com.apple.developer.carplay-maps`, à demander sur le portail
/// développeur) : sans lui, iOS ne crée jamais cette scène et ce fichier ne
/// coûte rien. Avec lui, aucune autre modification n'est nécessaire — la
/// scène est déjà déclarée dans l'Info.plist.
///
/// La carte est le même MapLibre que le téléphone, même style, mêmes tuiles
/// — y compris un serveur personnel si l'utilisateur en a un. Le guidage
/// vient de `NavigationManager.instanceCourante` : l'instance vit dans la
/// scène du téléphone, ce sujet est le pont entre les deux.
final class CarPlaySceneDelegate: UIResponder, CPTemplateApplicationSceneDelegate {
    private var controleur: CPInterfaceController?
    private var carte: CarPlayCarteVC?

    func templateApplicationScene(
        _ templateApplicationScene: CPTemplateApplicationScene,
        didConnect interfaceController: CPInterfaceController,
        to window: CPWindow
    ) {
        controleur = interfaceController
        let vc = CarPlayCarteVC()
        carte = vc
        window.rootViewController = vc

        let gabarit = CPMapTemplate()
        gabarit.automaticallyHidesNavigationBar = true
        let recentre = CPMapButton { [weak vc] _ in vc?.recentre() }
        recentre.image = UIImage(systemName: "location.fill")
        gabarit.mapButtons = [recentre]
        interfaceController.setRootTemplate(gabarit, animated: false, completion: nil)
    }

    func templateApplicationScene(
        _ templateApplicationScene: CPTemplateApplicationScene,
        didDisconnect interfaceController: CPInterfaceController,
        from window: CPWindow
    ) {
        controleur = nil
        carte = nil
    }
}

/// La carte elle-même : MapLibre plein écran, la position suivie cap en
/// haut, le tracé du guidage en surimpression, et le bandeau de consigne.
final class CarPlayCarteVC: UIViewController, MLNMapViewDelegate {
    private var mapView: MLNMapView!
    private let bandeau = UIView()
    private let consigne = UILabel()
    private let distance = UILabel()
    private var traceActuelle: MLNPolyline?
    /// Deux jeux séparés : l'abonnement au sujet (posé une fois) et ceux de
    /// l'instance courante (remplacés à chaque nouvelle instance). Les
    /// confondre boucle : un CurrentValueSubject rejoue sa valeur à chaque
    /// abonnement, et se réabonner dedans rappellerait `branche` sans fin.
    private var lienInstance: AnyCancellable?
    private var abonnements: [AnyCancellable] = []

    override func viewDidLoad() {
        super.viewDidLoad()
        mapView = MLNMapView(frame: view.bounds, styleURL: TileSource.styleURL)
        mapView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        mapView.delegate = self
        mapView.showsUserLocation = true
        mapView.userTrackingMode = .followWithCourse
        mapView.compassView.isHidden = true
        mapView.attributionButton.alpha = 0.4
        view.addSubview(mapView)

        // Le bandeau de consigne : lisible d'un coup d'œil, comme sur le
        // téléphone, mais aux proportions d'un écran de voiture.
        bandeau.backgroundColor = UIColor.systemBackground.withAlphaComponent(0.92)
        bandeau.layer.cornerRadius = 14
        bandeau.translatesAutoresizingMaskIntoConstraints = false
        consigne.font = .systemFont(ofSize: 24, weight: .bold)
        consigne.adjustsFontSizeToFitWidth = true
        consigne.minimumScaleFactor = 0.6
        distance.font = .systemFont(ofSize: 18, weight: .semibold)
        distance.textColor = .secondaryLabel
        let pile = UIStackView(arrangedSubviews: [distance, consigne])
        pile.axis = .vertical
        pile.spacing = 2
        pile.translatesAutoresizingMaskIntoConstraints = false
        bandeau.addSubview(pile)
        view.addSubview(bandeau)
        NSLayoutConstraint.activate([
            bandeau.topAnchor.constraint(
                equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 8),
            bandeau.leadingAnchor.constraint(
                equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 8),
            bandeau.widthAnchor.constraint(
                lessThanOrEqualTo: view.widthAnchor, multiplier: 0.55),
            pile.topAnchor.constraint(equalTo: bandeau.topAnchor, constant: 10),
            pile.bottomAnchor.constraint(equalTo: bandeau.bottomAnchor, constant: -10),
            pile.leadingAnchor.constraint(equalTo: bandeau.leadingAnchor, constant: 14),
            pile.trailingAnchor.constraint(equalTo: bandeau.trailingAnchor, constant: -14),
        ])
        bandeau.isHidden = true

        // Le pont vers le guidage : l'instance peut ne pas encore exister
        // (CarPlay branché avant le téléphone déverrouillé) — le sujet nous
        // la tendra quand elle arrivera.
        lienInstance = NavigationManager.instanceCourante
            .receive(on: DispatchQueue.main)
            .sink { [weak self] nav in self?.branche(nav) }
    }

    func recentre() {
        mapView.setUserTrackingMode(.followWithCourse, animated: true)
    }

    /// S'abonne aux publications du guidage : consigne, distance, tracé.
    private func branche(_ nav: NavigationManager?) {
        abonnements.removeAll()
        guard let nav else { return }
        nav.$active
            .receive(on: DispatchQueue.main)
            .sink { [weak self] actif in self?.bandeau.isHidden = !actif }
            .store(in: &abonnements)
        nav.$instruction
            .receive(on: DispatchQueue.main)
            .sink { [weak self] texte in self?.consigne.text = texte }
            .store(in: &abonnements)
        nav.$distanceToNext
            .receive(on: DispatchQueue.main)
            .sink { [weak self] m in
                // La règle de la maison, une seule fois : l'écran voiture
                // réécrivait la sienne, et elle a divergé (le seuil ne
                // portait pas sur la valeur arrondie).
                self?.distance.text = Distance.courte(m)
            }
            .store(in: &abonnements)
        nav.$routeCoords
            .receive(on: DispatchQueue.main)
            .sink { [weak self] coords in self?.montreTrace(coords) }
            .store(in: &abonnements)
    }

    private func montreTrace(_ coords: [CLLocationCoordinate2D]) {
        if let ancienne = traceActuelle {
            mapView.removeAnnotation(ancienne)
            traceActuelle = nil
        }
        guard coords.count >= 2 else { return }
        var points = coords
        let ligne = MLNPolyline(coordinates: &points, count: UInt(points.count))
        mapView.addAnnotation(ligne)
        traceActuelle = ligne
    }

    // MARK: - MLNMapViewDelegate

    func mapView(_ mapView: MLNMapView, strokeColorForShapeAnnotation annotation: MLNShape)
        -> UIColor
    {
        UIColor.systemGreen
    }

    func mapView(_ mapView: MLNMapView, lineWidthForPolylineAnnotation annotation: MLNPolyline)
        -> CGFloat
    {
        6
    }

    func mapView(_ mapView: MLNMapView, alphaForShapeAnnotation annotation: MLNShape)
        -> CGFloat
    {
        0.85
    }
}
