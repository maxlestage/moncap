import CoreLocation
import MapKit
import MapLibre
import SwiftUI

/// Écran « Hors ligne » : tout ce qui se prépare avant de perdre le réseau,
/// au même endroit.
///
/// Il y avait deux téléchargements à deux endroits — les fonds de carte ici,
/// les paquets d'itinéraires dans les réglages — et rien ne disait qu'il
/// fallait les deux. Ce sont bien deux jeux de données distincts : des images
/// d'un côté, un graphe routier de l'autre. Mais celui qui prépare un voyage
/// ne prépare pas une base de données : il regarde un endroit sur la carte et
/// veut pouvoir s'y rendre sans réseau. Les deux se demandent donc ici, sur la
/// zone qu'il a sous les yeux.
struct OfflineMapScreen: View {
    @Environment(\.dismiss) private var dismiss
    /// Position d'ouverture de la carte.
    let center: CLLocationCoordinate2D?

    /// Style vectoriel : renseigné, la carte est rendue par MapLibre plutôt
    /// que par des tuiles images.
    @AppStorage(TileSource.styleKey) private var styleURLString = ""
    /// Le calcul d'itinéraire sur l'appareil, activable une fois une région
    /// installée. Même clé que l'écran principal : c'est un seul réglage.
    @AppStorage("moncap.horsLigne") private var horsLigne = false
    @StateObject private var packs = VectorPackDownloader()
    @ObservedObject private var router = OfflineRouter.shared
    @State private var bounds: MLNCoordinateBounds?
    /// Le style est-il réellement servi ? Un serveur sans cartes rendrait une
    /// carte grise sans le moindre message.
    @State private var styleStatus = StyleStatus.checking

    /// Région d'itinéraires couvrant le point regardé, si on a su la nommer.
    @State private var suggestion: OfflineRouter.Region?
    /// Liste complète : deux cents entrées ne tiennent pas dans un panneau
    /// flottant, mais il faut pouvoir y aller quand la reconnaissance échoue.
    @State private var showRegions = false
    @State private var rechercheRegion = ""

    /// Fond réellement affiché : celui de l'utilisateur s'il répond, sinon le
    /// nôtre. Vérifié à l'ouverture, pour ne jamais rester devant un écran vide.
    @State private var style = URL(string: TileSource.defaultStyle)!
    @State private var depth = Depth.ville
    /// Champ de recherche actif : le panneau du bas se retire alors, sinon le
    /// clavier le remonte par-dessus la saisie et on n'y voit plus rien.
    @FocusState private var searching: Bool
    /// Tracé mémorisé, lu une seule fois (le corps de la vue est réévalué
    /// souvent : y lire le disque serait coûteux).
    @State private var route: [CLLocationCoordinate2D] = []
    /// Recherche de lieu : télécharger une zone suppose de pouvoir s'y rendre,
    /// or on prépare souvent un voyage depuis chez soi.
    @StateObject private var search = PlaceSearch()
    @State private var query = ""
    @State private var cam = MapCam(
        center: CLLocationCoordinate2D(latitude: 46.6, longitude: 2.4), zoom: 4.6)

    /// Profondeur de zoom téléchargée : plus on descend, plus c'est détaillé
    /// et lourd.
    private enum Depth: String, CaseIterable, Identifiable {
        case region = "Région"
        case ville = "Ville"
        case rue = "Rue"
        var id: String { rawValue }
        var zooms: ClosedRange<Int> {
            switch self {
            case .region: return 8...12
            case .ville: return 8...14
            case .rue: return 8...16
            }
        }
    }

    var body: some View {
        ZStack(alignment: .top) {
            VectorMapView(
                styleURL: style,
                route: route,
                cam: cam,
                onRegionChange: { bounds = $0 }
            )
            .ignoresSafeArea()
            VStack(spacing: 0) {
                header
                searchBar
                // La carte reste affichée malgré l'alerte : les zones déjà
                // enregistrées s'affichent sans réseau ni serveur.
                if let warning = styleStatus.message(hasStored: packs.packCount > 0) {
                    Text(warning)
                        .font(.caption)
                        .multilineTextAlignment(.center)
                        .padding(10)
                        .frame(maxWidth: .infinity)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
                        .padding(.horizontal)
                }
            }
            if !searching {
                VStack {
                    Spacer()
                    vectorPanel
                }
            }
        }
        .task(id: styleURLString) {
            styleStatus = .checking
            style = await TileSource.resolvedStyle(custom: styleURLString)
            // Le fond retenu répond, sauf absence de réseau : c'est le seul
            // cas qui mérite encore un message.
            styleStatus = await StyleProbe.check(style)
        }
        // Reconnaissance relancée par case de 0,25° — environ vingt-cinq
        // kilomètres — et non à chaque image : un glissement de carte en
        // produit des dizaines par seconde, et le géocodeur du système est
        // limité en cadence.
        .task(id: caseRegardee) { await chercheRegion() }
        .sheet(isPresented: $showRegions) { listeRegions }
        .onAppear {
            route = RouteStore.load()?.coordinates ?? []
            packs.refresh()
            search.center = center
            if let center { cam = MapCam(center: center, zoom: 11) }
        }
    }

    // MARK: - Itinéraires hors réseau

    /// Centre de la zone visible.
    private var centre: CLLocationCoordinate2D? {
        guard let bounds else { return nil }
        return CLLocationCoordinate2D(
            latitude: (bounds.sw.latitude + bounds.ne.latitude) / 2,
            longitude: (bounds.sw.longitude + bounds.ne.longitude) / 2)
    }

    /// Case de 0,25° contenant le centre, qui sert d'identité au géocodage.
    private var caseRegardee: String? {
        guard let c = centre else { return nil }
        return "\((c.latitude * 4).rounded())|\((c.longitude * 4).rounded())"
    }

    /// Quelle région d'itinéraires couvre le point regardé ?
    ///
    /// Les paquets Valhalla n'ont pas de contour : ils portent un nom
    /// Geofabrik — « France · Nouvelle-Aquitaine » — et leur poids, rien de
    /// plus. On demande donc au système le nom administratif du point regardé,
    /// et on le cherche dans la liste publiée.
    ///
    /// C'est une reconnaissance par le nom, pas une couverture géométrique :
    /// elle échoue quand le découpage administratif ne suit pas celui de
    /// Geofabrik, et elle demande du réseau une fois. D'où la liste complète,
    /// toujours à côté — cette suggestion fait gagner du temps, elle n'est pas
    /// le seul chemin.
    /// `@MainActor` : `OfflineRouter` l'est, et l'état de la vue aussi.
    @MainActor
    private func chercheRegion() async {
        guard let c = centre else { return }
        // D'abord ce qui marche sans réseau : demander aux paquets installés
        // s'ils couvrent le point. Le reste — nommer la région, la retrouver
        // dans les Releases — a besoin d'une connexion, et échouera en mode
        // avion sans que cela doive effacer la réponse qu'on a déjà.
        router.chercheRegionCouvrante(c)

        await router.loadRegions()
        guard
            let lieu = try? await CLGeocoder().reverseGeocodeLocation(
                CLLocation(latitude: c.latitude, longitude: c.longitude)
            ).first
        else { return }

        let parRegion = lieu.administrativeArea.flatMap { router.recherche($0).first }
        // À défaut, un pays livré d'un seul bloc — Monaco, Andorre — qui n'a
        // pas de sous-région. Prendre la première région d'un grand pays
        // désignerait un endroit au hasard.
        let parPays = lieu.country.flatMap { nom in
            router.recherche(nom).first { $0.detail.isEmpty }
        }
        suggestion = parRegion ?? parPays
    }

    private var header: some View {
        HStack(spacing: 10) {
            Button { dismiss() } label: {
                Image(systemName: "xmark")
                    .font(.headline)
                    .foregroundStyle(.primary)
                    .frame(width: 40, height: 40)
                    .background(.regularMaterial, in: Circle())
            }
            VStack(alignment: .leading, spacing: 1) {
                Text("Carte hors ligne").font(.headline)
                Text(
                    packs.packCount > 0
                        ? "\(packs.packCount) zone\(packs.packCount > 1 ? "s" : "") · "
                            + Self.megabytes(packs.storedBytes)
                        : "Aucune zone enregistrée"
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.regularMaterial, in: Capsule())
            Spacer()
        }
        .padding()
    }

    /// Panneau de téléchargement en mode vectoriel : MapLibre gère lui-même le
    /// découpage en tuiles et leur conservation.
    private var vectorPanel: some View {
        VStack(spacing: 12) {
            // Chaque moitié du panneau porte son titre : deux téléchargements
            // cohabitent ici — des images, un graphe routier — et sans
            // en-têtes, boutons et réglages semblaient tous sortir du même
            // sac. « Tout effacer » vit à côté de ce qu'il efface, et
            // seulement quand il y a quelque chose à effacer.
            HStack {
                Label("Fonds de carte", systemImage: "square.3.layers.3d.down.right")
                    .font(.subheadline.weight(.semibold))
                Spacer()
                if packs.packCount > 0 {
                    Button("Tout effacer", role: .destructive) {
                        packs.clearAll()
                    }
                    .font(.caption.weight(.semibold))
                }
            }

            Picker("Détail", selection: $depth) {
                ForEach(Depth.allCases) { d in Text(LocalizedStringKey(d.rawValue)).tag(d) }
            }
            .pickerStyle(.segmented)

            if packs.running {
                BarreProgression(valeur: packs.progress, titre: "Fonds de carte")
            } else {
                let devis = estimate
                // Déjà enregistrée : réappuyer empilerait une copie de la même
                // zone, occupant à nouveau la place pour rien.
                let deja =
                    bounds.map {
                        packs.covers(bounds: $0, maxZoom: Double(depth.zooms.upperBound))
                    } ?? false
                let bloque = deja || devis?.toolarge == true
                Button {
                    if let bounds, let devis, !devis.toolarge, !deja {
                        packs.start(
                            styleURL: style, bounds: bounds,
                            maxZoom: Double(depth.zooms.upperBound))
                    }
                } label: {
                    Label(
                        deja
                            ? "Zone déjà enregistrée"
                            : devis.map { "Télécharger cette zone · \(Self.megabytes($0.bytes))" }
                                ?? "Télécharger cette zone",
                        systemImage: deja ? "checkmark.circle.fill" : "arrow.down.circle.fill"
                    )
                    .font(.subheadline.weight(.semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
                    .background(
                        (bloque ? Color.gray : Color.blue).opacity(0.18),
                        in: RoundedRectangle(cornerRadius: 14))
                }
                // Télécharger d'un serveur sans cartes ne produirait qu'un
                // pack vide, sans erreur visible.
                .disabled(bounds == nil || !styleStatus.canDownload || bloque)
                // Le trajet mémorisé se télécharge en couloir : sur plusieurs
                // centaines de kilomètres, le rectangle englobant réclamerait
                // dix à trente fois plus de tuiles pour la même route.
                if route.count >= 2, let trajet = routeEstimate, !trajet.toolarge {
                    Button {
                        packs.startRoute(
                            styleURL: style, route: route,
                            maxZoom: Double(depth.zooms.upperBound))
                    } label: {
                        Label(
                            "Mon itinéraire · \(Self.megabytes(trajet.bytes))",
                            systemImage: "point.topleft.down.to.point.bottomright.curvepath.fill"
                        )
                        .font(.subheadline.weight(.semibold))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                        .background(Color.green.opacity(0.18), in: RoundedRectangle(cornerRadius: 14))
                    }
                    .disabled(!styleStatus.canDownload)
                } else if route.count >= 2, let trajet = routeEstimate {
                    // Un seul littéral : « "a" + "b" » produit une `String`,
                    // et `Text(String)` ne consulte aucun catalogue.
                    Text(
                        "Itinéraire trop long à ce niveau de détail (\(Self.megabytes(trajet.bytes))). Choisis « Région »."
                    )
                    .font(.caption2).foregroundStyle(.orange)
                    .multilineTextAlignment(.center)
                }
                if let devis, devis.toolarge {
                    Text(
                        "Zone trop vaste : \(Self.megabytes(devis.bytes)) environ. Rapproche-toi, ou choisis un niveau de détail moindre."
                    )
                    .font(.caption2).foregroundStyle(.orange)
                    .multilineTextAlignment(.center)
                }
                if let message = packs.message {
                    Text(message).font(.caption2).foregroundStyle(.orange)
                }
            }

            Divider()
            routage

            // Le crédit seul, centré : il partageait sa ligne avec « Tout
            // effacer », un remerciement collé à un bouton destructif.
            Text("© OpenStreetMap contributors")
                .font(.caption2).foregroundStyle(.secondary)
                .frame(maxWidth: .infinity)
        }
        .padding(14)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 20))
        .padding()
    }

    /// Le calcul d'itinéraire sans réseau, demandé sur la zone regardée.
    ///
    /// Les tuiles au-dessus servent à *voir* la carte ; ce paquet-ci à *y
    /// calculer une route*. Sans lui, une carte enregistrée s'affiche hors
    /// couverture mais ne sait plus où aller — c'est exactement la déception
    /// que deux téléchargements séparés produisaient.
    private var routage: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                // Une ligne chacun : pliés, le titre et le lien occupaient
                // quatre lignes à eux deux et l'en-tête devenait un pavé.
                Label("Itinéraires hors réseau", systemImage: "point.topleft.down.to.point.bottomright.curvepath")
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                Spacer(minLength: 10)
                Button("Toutes les régions") { showRegions = true }
                    .font(.caption.weight(.semibold))
                    .lineLimit(1)
                    .fixedSize()
            }

            if router.downloading {
                // Le nom de la région et son poids sous la barre : sur cinq
                // cents mégaoctets, savoir ce qui descend et combien il en
                // reste vaut mieux qu'un pourcentage seul.
                BarreProgression(
                    valeur: router.progress,
                    titre: "Itinéraires hors réseau",
                    detail: router.enCours)
            } else if let region = suggestion {
                let deja = router.estInstallee(region)
                Button {
                    Task { await router.install(region) }
                } label: {
                    HStack(spacing: 10) {
                        Image(
                            systemName: deja
                                ? "checkmark.circle.fill" : "arrow.down.circle.fill")
                        VStack(alignment: .leading, spacing: 1) {
                            // Un nom Geofabrik est un nom propre : `Text(String)`
                            // est verbatim, et c'est voulu — « Nouvelle-Aquitaine »
                            // ne se traduit pas.
                            Text(region.nom)
                                .font(.subheadline.weight(.semibold))
                            Text(deja ? "Déjà installée" : "Calcul sur l'appareil · \(region.poids)")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.vertical, 12)
                    .padding(.horizontal, 12)
                    .background(
                        (deja ? Color.green : Color.indigo).opacity(0.18),
                        in: RoundedRectangle(cornerRadius: 14))
                }
                .disabled(deja)
            } else if router.chargement || router.verifieCouverture {
                ProgressView()
            } else if let couvrante = router.regionCouvrante {
                // Ce qui est déjà installé et couvre le point se dit **avant**
                // toute proposition de téléchargement : annoncer « aucune
                // région reconnue ici » à quelqu'un dont le paquet couvre
                // précisément l'endroit où il se trouve, c'est le pousser à
                // retélécharger ce qu'il a déjà.
                HStack(spacing: 10) {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                    VStack(alignment: .leading, spacing: 1) {
                        // Nom propre Geofabrik : verbatim, il ne se traduit pas.
                        Text(couvrante.nom)
                            .font(.subheadline.weight(.semibold))
                        Text("Couvre ta position")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                }
                .padding(.vertical, 12)
                .padding(.horizontal, 12)
                .background(Color.green.opacity(0.18), in: RoundedRectangle(cornerRadius: 14))
            } else if router.ready {
                // Des régions installées, mais aucune qui réponde ici : c'est
                // un fait, et il vaut mieux le dire que laisser croire à un
                // simple défaut de reconnaissance.
                Text("Aucune région installée ne répond à cet endroit.")
                    .font(.caption2).foregroundStyle(.secondary)
            } else {
                Text("Aucune région reconnue ici. Ouvre la liste complète.")
                    .font(.caption2).foregroundStyle(.secondary)
            }

            // Le réglage vit là où on installe ce dont il dépend : proposé
            // ailleurs, il restait grisé sans qu'on sache quoi faire.
            if router.ready {
                Toggle("Naviguer hors ligne", isOn: $horsLigne)
                    .font(.subheadline)
            }
            // L'avertissement ne s'affiche que s'il apporte quelque chose.
            // Une tentative ratée — au sortir du mode avion, la radio met
            // quelques secondes à se réveiller — épinglait « il faut du
            // réseau » pour toujours, juste sous une région installée
            // annoncée en vert : l'écran se contredisait. Quand une carte
            // utile est déjà affichée, le message se tait ; sinon il
            // s'accompagne du geste qui le résout.
            if let m = router.message, suggestion == nil, router.regionCouvrante == nil {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(m).font(.caption2).foregroundStyle(.orange)
                    Spacer(minLength: 8)
                    Button("Réessayer") {
                        Task { await router.loadRegions() }
                    }
                    .font(.caption.weight(.semibold))
                }
            }
        }
    }

    /// Les deux cents régions publiées, en feuille séparée : elles ne tiennent
    /// pas dans un panneau posé sur la carte.
    private var listeRegions: some View {
        NavigationStack {
            List {
                if !router.installees.isEmpty {
                    Section("Sur cet appareil") {
                        ForEach(router.installees) { gardee in
                            HStack {
                                Label(gardee.nom, systemImage: "checkmark.circle.fill")
                                    .foregroundStyle(.green)
                                Spacer()
                                Text(gardee.poids)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                                Button(role: .destructive) {
                                    router.remove(gardee.id)
                                } label: {
                                    Image(systemName: "trash")
                                }
                                .buttonStyle(.plain)
                                .foregroundStyle(.red)
                            }
                        }
                        if router.installees.count > 1 {
                            Text("\(router.installees.count) régions · \(router.poidsTotal)")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                }

                Section {
                    if router.chargement {
                        ProgressView()
                    } else if router.regions.isEmpty {
                        // Le routeur dit *pourquoi* quand il le sait : un
                        // serveur qui refuse ne se répare pas en cherchant du
                        // réseau. Deux branches et non un `??` : celui-ci
                        // produirait une `String`, et `Text(String)` est
                        // verbatim — le repli ne serait jamais traduit.
                        if let m = router.message {
                            Text(m).font(.footnote).foregroundStyle(.orange)
                        } else {
                            Text("Liste des régions indisponible — il faut du réseau une fois.")
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                    } else if rechercheRegion.isEmpty {
                        // Un pays par rangée, ses régions dessous : à deux
                        // cents entrées, une liste à plat serait impraticable.
                        // Un pays livré d'un bloc n'ouvre rien.
                        ForEach(router.groupes) { groupe in
                            if groupe.regions.count == 1, let seule = groupe.regions.first {
                                RegionButton(region: seule, router: router)
                            } else {
                                DisclosureGroup("\(groupe.pays) · \(groupe.regions.count)") {
                                    ForEach(groupe.regions) { region in
                                        RegionButton(region: region, router: router)
                                    }
                                }
                            }
                        }
                    } else {
                        // À plat pendant la recherche : déplier un pays pour
                        // trouver ce qu'on a déjà nommé n'aurait aucun sens.
                        let trouvees = router.recherche(rechercheRegion)
                        if trouvees.isEmpty {
                            Text("Aucune région de ce nom.")
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                        ForEach(trouvees) { region in
                            RegionButton(region: region, router: router, complet: true)
                        }
                    }
                } footer: {
                    Text(
                        "Permet de calculer et de recalculer un itinéraire sans réseau. Garde autant de régions que tu veux : un trajet cherche sa route dans chacune. En revanche il doit tenir entier dans l'une d'elles — c'est à cela que servent les zones, qui se chevauchent."
                    )
                }
            }
            .searchable(text: $rechercheRegion, prompt: Text("Chercher une région"))
            .navigationTitle("Itinéraires hors réseau")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Fermer") { showRegions = false }
                }
            }
            .task { await router.loadRegions() }
        }
    }

    /// Volume attendu pour un couloir suivant l'itinéraire mémorisé.
    private var routeEstimate: OfflineEstimate? {
        guard route.count >= 2 else { return nil }
        return OfflineEstimate.of(route: route, zooms: depth.zooms)
    }

    /// Volume attendu pour la zone visible et le niveau de détail choisi.
    private var estimate: OfflineEstimate? {
        guard let bounds else { return nil }
        return OfflineEstimate.of(sw: bounds.sw, ne: bounds.ne, zooms: depth.zooms)
    }

    /// Barre de recherche : sans elle on ne pouvait enregistrer que là où l'on
    /// se trouve, alors qu'on prépare un voyage avant de partir.
    private var searchBar: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                TextField("Chercher un lieu, une ville, un pays", text: $query)
                    .focused($searching)
                    .autocorrectionDisabled()
                    .onChange(of: query) { _, q in search.search(q) }
                    .onSubmit { search.search(query) }
                if !query.isEmpty {
                    Button {
                        query = ""
                        search.clear()
                    } label: {
                        Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                    }
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(.regularMaterial, in: Capsule())

            if !search.results.isEmpty {
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(search.results, id: \.self) { item in
                            Button {
                                let c = item.placemark.coordinate
                                // Zoom « ville » : la zone à enregistrer tient
                                // alors à l'écran d'un seul tenant.
                                cam = MapCam(center: c, zoom: 11)
                                query = ""
                                search.clear()
                                searching = false
                            } label: {
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(item.name ?? "Lieu").font(.subheadline)
                                    if let city = item.placemark.locality
                                        ?? item.placemark.country
                                    {
                                        Text(city).font(.caption2).foregroundStyle(.secondary)
                                    }
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.vertical, 8)
                                .padding(.horizontal, 14)
                            }
                            .buttonStyle(.plain)
                            Divider()
                        }
                    }
                }
                .frame(maxHeight: 220)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
            }
        }
        .padding(.horizontal)
        .padding(.top, 4)
    }

    /// « 24 Mo » — l'espace disque parle plus qu'un nombre de tuiles.
    private static func megabytes(_ bytes: Int) -> String {
        let mo = Double(bytes) / 1_000_000
        return mo < 10
            ? String(format: "%.1f Mo", mo) : String(format: "%.0f Mo", mo)
    }
}

/// Une région d'itinéraires téléchargeable.
///
/// Extraite en vue à part : elle apparaît à deux endroits — un pays livré d'un
/// bloc, et une région dans son pays déplié.
struct RegionButton: View {
    let region: OfflineRouter.Region
    @ObservedObject var router: OfflineRouter
    /// Nom entier, pays compris : indispensable dans une liste de recherche,
    /// où plus rien ne dit de quel pays vient la ligne.
    var complet = false

    var body: some View {
        let deja = router.estInstallee(region)
        Button {
            Task { await router.install(region) }
        } label: {
            Label(
                "\(libelle) · \(region.poids)",
                systemImage: deja ? "checkmark.circle.fill" : "arrow.down.circle"
            )
            .foregroundStyle(deja ? Color.green : Color.accentColor)
        }
        // Retélécharger ce qu'on a déjà ne ferait que perdre le forfait.
        .disabled(deja)
    }

    private var libelle: String {
        // Sous un pays déplié, le répéter sur chaque ligne mangerait la
        // largeur ; un pays livré d'un bloc n'a lui aucun détail à montrer.
        if complet || region.detail.isEmpty { return region.nom }
        return region.detail
    }
}
