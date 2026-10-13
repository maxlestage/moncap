//! La vitrine, en vues `active` (la partie front de Paquets).
//!
//! Portage fidèle de l'ancienne version Yew, elle-même portage de l'ancien
//! `App.tsx` : même structure, mêmes classes CSS, mêmes attributs
//! d'accessibilité. La feuille de style n'a pas bougé d'une ligne, et c'est
//! voulu — un portage qui change deux choses à la fois ne se vérifie plus.
//!
//! Une vue est une fonction qui rend un [`Node`]. `construit` les rend en HTML
//! à la construction : chaque page part **complète** — texte, maquettes,
//! liens —, lisible sans script, par un robot comme par un aperçu de lien. Le
//! navigateur ne repeint rien : il réhydrate les deux seuls îlots
//! interactifs (la flèche de défilement et l'année du copyright, voir
//! [`crate::ILOTS`]) puis démarre le moteur de mouvement.
//!
//! Le mouvement — titres qui montent mot à mot, cascades, dévoilements,
//! compteurs, bandeau défilant — est déclaré ici par des attributs
//! (`data-mvt`, `data-compte`, `data-defile`, `data-aimant`) et joué par la
//! crate `mouvement`, partagée avec le front web. Le rendu ne lance rien : il
//! écrit le HTML immobile, que le moteur anime une fois chargé.
//!
//! Les maquettes sont du HTML et du CSS, pas des images : elles restent
//! nettes à toutes les résolutions, suivent le thème, se lisent au lecteur
//! d'écran, et ne coûtent pas un octet de téléchargement supplémentaire. Ce
//! sont des dessins de l'interface, marqués comme tels.

use mouvement::calcul;
use paquets::active::html::*;
use paquets::active::{el, fragment, island, text_dyn, Element, Node};

use crate::appstrings::app;
use crate::contenu::{
    Itineraire, Langue, PageLegale, AUTEUR, CONNEXION, LIEUX, MAQUETTE_ITINERAIRES,
    MAQUETTE_REGIONS, MODES, MODE_ACTIF, OUTILS, SOURCES,
};
use crate::traduire::{
    app_format,
    Valeur::{Nombre, Texte},
};

/// Le nom de l'îlot de l'année, côté rendu comme côté navigateur.
pub const ILOT_ANNEE: &str = "annee";

/// Les chemins des ressources, que seule la construction connaît (ils
/// portent une empreinte).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ressources {
    pub logo: String,
}

/// Les quatre thèmes, dans l'ordre des libellés fournis par la langue.
const THEMES: [&str; 4] = ["auto", "clair", "sombre", "orange"];

/// Quelle page afficher. Le chemin de l'URL la désigne.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Accueil,
    Mentions,
    Confidentialite,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Props {
    pub langue: Langue,
    pub page: Page,
    pub res: Ressources,
    /// L'année du copyright au moment du rendu. Le navigateur la remplace
    /// par la sienne (îlot [`annee`]) : une année gravée à la construction
    /// deviendrait fausse le 1er janvier, et personne ne pense à
    /// reconstruire pour ça.
    pub annee: i32,
}

/// Le corps d'une page, tel que `construit` l'écrit dans la coquille.
pub fn vitrine(p: &Props) -> Node {
    let c = p.langue.contenu();
    match p.page {
        Page::Accueil => accueil(p),
        Page::Mentions => legale(p, &c.mentions),
        Page::Confidentialite => legale(p, &c.confidentialite),
    }
}

// --- Les îlots -------------------------------------------------------------

/// L'îlot de l'année : le rendu écrit celle de la construction, le
/// navigateur la remplace par la sienne en réhydratant.
///
/// Un texte dynamique (`text_dyn`) et non fixe : un texte fixe serait
/// « réclamé » tel quel à l'hydratation, sans jamais être réécrit.
pub fn annee(props: &str) -> Node {
    let a = annee_du_navigateur().unwrap_or_else(|| props.to_string());
    text_dyn(move || a.clone())
}

/// L'année lue dans le navigateur ; `None` hors navigateur.
#[cfg(target_arch = "wasm32")]
fn annee_du_navigateur() -> Option<String> {
    Some((js_sys::Date::new_0().get_full_year() as i32).to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn annee_du_navigateur() -> Option<String> {
    None
}

// --- Le mouvement ----------------------------------------------------------

/// Un attribut posé seulement si `si` est vrai : `aria-hidden` sur la
/// seconde moitié d'un bandeau doublé, par exemple.
fn attr_si(e: Element, si: bool, nom: &'static str, valeur: &'static str) -> Element {
    if si {
        e.attr(nom, valeur)
    } else {
        e
    }
}

/// Un texte découpé en mots, chacun derrière son masque : le parent porte
/// `data-mvt="mots"`, la feuille fait monter les mots un à un. `depart` est
/// le rang du premier mot, pour enchaîner deux lignes dans la même cascade.
///
/// Les espaces insécables restent dans le mot (voir `calcul::mots`) : « 3,99
/// € » ne se coupe pas plus animé qu'immobile.
fn mots(texte: &str, depart: usize) -> Node {
    let liste: Vec<&str> = calcul::mots(texte).collect();
    let n = liste.len();
    let mut rendu: Vec<Node> = Vec::with_capacity(n * 2);
    for (k, m) in liste.into_iter().enumerate() {
        rendu.push(
            span()
                .class("mot")
                .child(
                    span()
                        .style(format!("--i:{}", depart + k))
                        .text(m.to_string()),
                )
                .into(),
        );
        if k + 1 < n {
            rendu.push(" ".into());
        }
    }
    fragment(rendu)
}

/// Le nombre de lettres d'un texte, blancs exclus : le rang de départ de la
/// ligne suivante dans une cascade de lettres.
fn nb_lettres(texte: &str) -> usize {
    calcul::mots(texte).map(|m| m.chars().count()).sum()
}

/// Un texte découpé en lettres, chacune derrière le masque de son mot. Plus
/// démonstratif que les mots — réservé aux grands titres.
///
/// Rendu pour l'œil seulement (`aria-hidden`) : épelé lettre à lettre, un
/// titre se lirait mal au lecteur d'écran. Qui l'appelle place le texte
/// entier à côté, en `visuel-cache`.
fn lettres(texte: &str, depart: usize) -> Node {
    let mut rang = depart;
    let liste: Vec<&str> = calcul::mots(texte).collect();
    let n = liste.len();
    let mut rendu: Vec<Node> = Vec::with_capacity(n * 2);
    for (k, m) in liste.into_iter().enumerate() {
        let lettres = m.chars().map(|ch| {
            let j = rang;
            rang += 1;
            span()
                .class("lettre")
                .style(format!("--j:{j}"))
                .text(ch.to_string())
        });
        rendu.push(span().class("mot").children(lettres).into());
        if k + 1 < n {
            rendu.push(" ".into());
        }
    }
    fragment(rendu)
}

/// Un libellé qui roule au survol : le texte monte et son double le
/// remplace. Le double est caché aux lecteurs d'écran, qui ne liraient
/// sinon chaque lien deux fois.
fn roule(texte: &'static str) -> Element {
    span().class("roule").child(span().text(texte)).child(
        span()
            .class("roule-double")
            .attr("aria-hidden", "true")
            .text(texte),
    )
}

/// Le numéro d'une section, et le trait qui le suit — l'index des sites
/// éditoriaux. Décoratif : le titre dit déjà de quoi il s'agit.
fn numero(n: usize) -> Element {
    p().class("numero")
        .attr("aria-hidden", "true")
        .attr("data-mvt", "trait")
        .text(format!("{n:02}"))
}

/// Le bandeau qui défile sous le chapeau : les promesses du hors-ligne,
/// déjà traduites dans les trois langues. Doublé pour boucler sans couture,
/// et lu une seule fois par les lecteurs d'écran — la seconde moitié leur
/// est cachée.
fn defilant(l: Langue) -> Element {
    let c = l.contenu();
    let suite = |cachee: bool| {
        attr_si(
            span().class("defilant-suite"),
            cachee,
            "aria-hidden",
            "true",
        )
        .children(c.hors_ligne_cartes.iter().map(|carte| {
            span()
                .text(carte.titre)
                .child(i().attr("aria-hidden", "true").text("✦"))
        }))
    };
    div().class("defilant").attr("data-defile", "").child(
        div()
            .class("defilant-piste")
            .child(suite(false))
            .child(suite(true)),
    )
}

/// Le second bandeau, en sens inverse et en lettres creuses : les outils
/// de la pile. Des noms propres, les mêmes dans les trois langues.
fn defilant_outils() -> Element {
    let suite = |cachee: bool| {
        attr_si(
            span().class("defilant-suite"),
            cachee,
            "aria-hidden",
            "true",
        )
        .children(OUTILS.iter().flat_map(|g| g.iter()).map(|o| {
            span()
                .text(*o)
                .child(i().attr("aria-hidden", "true").text("●"))
        }))
    };
    div()
        .class("defilant defilant-outils")
        .attr("data-defile", "")
        .attr("aria-hidden", "true")
        .child(
            div()
                .class("defilant-piste")
                .child(suite(false))
                .child(suite(true)),
        )
}

/// Une épingle de la carte vivante.
fn epingle(classe: &'static str, position: &'static str) -> Element {
    g().class(classe)
        .attr("transform", position)
        .child(circle().attr("r", "11").class("halo"))
        .child(circle().attr("r", "5"))
}

/// Le décor du bandeau : une carte qui vit. Des rues, des itinéraires qui se
/// tracent et s'effacent en boucle, des épingles qui pulsent, une voiture
/// qui roule. Elle suit la souris (`data-souris`) et s'éloigne quand on
/// descend (`data-defile`).
///
/// Sans moteur, elle est immobile et entièrement tracée : un dessin, pas un
/// manque.
fn carte_vivante() -> Element {
    const R1: &str = "M60 400 C120 310 190 340 250 270 S360 170 430 200 S540 130 585 62";
    const R2: &str = "M10 150 C110 160 170 96 250 116 S380 226 470 214 S560 300 600 292";
    const R3: &str = "M300 420 C290 344 342 300 330 232 S262 124 302 42";
    let route = |classe: &'static str, d: &'static str| {
        path().class(classe).attr("pathLength", "1").attr("d", d)
    };
    div()
        .class("carte-vivante")
        .attr("aria-hidden", "true")
        .attr("data-souris", "")
        .attr("data-defile", "")
        .child(
            svg()
                .attr("viewBox", "0 0 600 420")
                .attr("preserveAspectRatio", "xMidYMid slice")
                .attr("focusable", "false")
                .child(
                    g().class("cv-rues")
                        .child(path().attr(
                            "d",
                            "M0 84H600M0 196H600M0 308H600M92 0V420M252 0V420M412 0V420M532 0V420",
                        ))
                        .child(path().attr("d", "M0 372C150 336 222 254 332 244S520 154 600 126"))
                        .child(path().attr("d", "M0 40C120 70 160 160 230 210S400 300 460 420")),
                )
                .child(route("cv-route r1", R1))
                .child(route("cv-route r2", R2))
                .child(route("cv-route r3", R3))
                .child(epingle("cv-epingle e1", "translate(585 62)"))
                .child(epingle("cv-epingle e2", "translate(600 292)"))
                .child(epingle("cv-epingle e3", "translate(302 42)"))
                .child(
                    g().class("cv-voiture")
                        .child(circle().attr("r", "9").class("halo"))
                        .child(circle().attr("r", "4.5"))
                        // Le chemin recopié plutôt qu'un `<mpath>` : une
                        // référence `href="#…"` se lirait comme un lien de plus.
                        .child(
                            el("animateMotion")
                                .attr("dur", "11s")
                                .attr("repeatCount", "indefinite")
                                .attr("path", R1),
                        ),
                ),
        )
}

// --- Les maquettes --------------------------------------------------------

/// La légende d'une maquette.
fn legende(texte: &'static str) -> Element {
    figcaption().text(texte)
}

/// Le cadre commun des maquettes : dévoilé par le bas, redressé au défilement.
fn maquette() -> Element {
    figure()
        .class("maquette")
        .attr("data-mvt", "devoile")
        .attr("data-scrub", "")
}

/// Le coût d'un itinéraire : carburant seul, ou carburant, péages et total.
///
/// Le total se recalcule au lieu d'être rangé à côté de ses parts — deux
/// façons d'écrire le même nombre finissent toujours par diverger.
fn ligne_cout(langue: Langue, r: &Itineraire) -> String {
    match r.peages {
        None => app_format(langue, "⛽ ~%.2f € · sans péage", &[Nombre(r.carburant)]),
        Some(pe) => app_format(
            langue,
            "⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €",
            &[Nombre(r.carburant), Nombre(pe), Nombre(r.total())],
        ),
    }
}

/// L'écran de comparaison : le cœur de l'app quand le réseau est là.
fn maquette_comparateur(l: Langue) -> Element {
    let c = l.contenu();
    let modes = div()
        .class("modes")
        .attr("aria-hidden", "true")
        .children(MODES.iter().map(|m| {
            span()
                .class(if *m == MODE_ACTIF { "actif" } else { "" })
                .text(app(l, m))
        }));
    let routes = ul()
        .class("routes")
        .children(MAQUETTE_ITINERAIRES.iter().map(|r| {
            let mut detail = div()
                .child(
                    p().class("etiquette")
                        .class(r.teinte.classe())
                        .text(app(l, r.etiquette)),
                )
                .child(p().class("chiffres-route").text(app_format(
                    l,
                    "%.0f min · %@ · %d virages · arr. %@",
                    &[
                        Nombre(r.minutes as f64),
                        Texte(r.distance),
                        Nombre(r.virages as f64),
                        Texte(r.arrivee),
                    ],
                )));
            if let Some((montee, descente)) = r.denivele {
                detail = detail.child(p().class("detail").text(app_format(
                    l,
                    "↗ %.0f m de côte · ↘ %.0f m de descente",
                    &[Nombre(montee as f64), Nombre(descente as f64)],
                )));
            }
            detail = detail.child(p().class("detail").text(ligne_cout(l, r)));
            if let Some(m) = r.avertissement {
                detail = detail.child(p().class("alerte").text(app_format(
                    l,
                    "⚠️ Forte côte (%.0f m)",
                    &[Nombre(m as f64)],
                )));
            }
            li().child(
                span()
                    .class("pastille")
                    .class(r.teinte.classe())
                    .attr("aria-hidden", "true"),
            )
            .child(detail)
        }));
    maquette()
        .child(
            div()
                .class("ecran")
                .child(modes)
                .child(routes)
                // Le comparateur en propose jusqu'à quinze : trois lignes
                // seules laisseraient croire à trois.
                .child(
                    p().class("reste")
                        .attr("aria-hidden", "true")
                        .text(format!("⌄ {}", c.reste)),
                )
                .child(
                    p().class("demarrer")
                        .attr("aria-hidden", "true")
                        .text(app(l, "Démarrer")),
                ),
        )
        .child(legende(c.legendes.itineraires))
}

/// Le bandeau de guidage, avec les pastilles vitesse et météo.
fn maquette_guidage(l: Langue) -> Element {
    let c = l.contenu();
    let guidage = div()
        .class("guidage")
        .attr("aria-hidden", "true")
        .child(span().class("fleche").text("↱"))
        .child(
            div()
                .child(p().class("distance").text("300 m"))
                // Le bandeau ne montre pas une phrase mais un mot, lisible
                // d'un coup d'œil au volant, et sa précision en dessous.
                .child(
                    p().class("consigne")
                        .text(app_format(l, "Sortie %@", &[Texte("12")])),
                )
                .child(p().class("precision").text(app(l, "sortie"))),
        );
    let pastilles = div()
        .class("pastilles")
        .attr("aria-hidden", "true")
        .child(
            span()
                .class("meteo")
                .text(format!("🌤️ 21° {}", app(l, "Éclaircies"))),
        )
        .child(
            span()
                .class("vitesse")
                .child(strong().attr("data-compte", "").text("78"))
                .child(small().text("km/h"))
                .child(em().text("90")),
        )
        // Le panneau de commandes : un seul bloc arrondi. En guidage il ne
        // montre que la 3D et le recentrage.
        .child(
            span()
                .class("panneau")
                .child(span().text("3D"))
                .child(span().class("loc").text("➤")),
        );
    // La barre d'arrivée : les minutes, l'heure en orange — l'une répond à
    // « c'est encore long ? », l'autre à « j'y suis quand ? » — et le bouton
    // pour quitter.
    let eta = div()
        .class("eta-barre")
        .attr("aria-hidden", "true")
        .child(
            div()
                .child(
                    p().class("eta-min")
                        .text("12 min ")
                        .child(em().text("18:42")),
                )
                .child(
                    p().class("eta-km")
                        .text(app_format(l, "%.1f km restants", &[Nombre(8.4)])),
                ),
        )
        .child(span().class("eta-quitter").text(app(l, "Quitter")));
    maquette()
        .child(
            div()
                .class("ecran")
                .child(guidage)
                .child(pastilles)
                .child(eta),
        )
        .child(legende(c.legendes.guidage))
}

/// L'app Apple Watch : le cadran reçu pendant un guidage.
fn maquette_montre(l: Langue) -> Element {
    let c = l.contenu();
    let pied = div()
        .class("pied-poignet")
        .child(
            span()
                .child(strong().text("8.4 km"))
                .child(small().text("restant")),
        )
        .child(
            span()
                .child(strong().text("18:42"))
                .child(small().text("arrivée")),
        );
    maquette()
        .class("montre")
        .child(
            div()
                .class("cadran")
                .attr("aria-hidden", "true")
                .child(span().class("fleche-poignet").text("↱"))
                .child(p().class("distance-poignet").text("400 m"))
                .child(p().class("consigne-poignet").text(app(l, "Droite")))
                .child(pied),
        )
        .child(legende(c.legendes.montre))
}

/// Le bandeau des catégories de lieux.
fn maquette_lieux(l: Langue) -> Element {
    let c = l.contenu();
    let lieux = ul()
        .class("lieux")
        .children(LIEUX.iter().enumerate().map(|(k, lieu)| {
            li().class(if k == 0 { "actif" } else { "" })
                .child(span().attr("aria-hidden", "true").text(lieu.emoji))
                .text(format!(" {}", app(l, lieu.nom)))
        }));
    maquette()
        .child(div().class("ecran").child(lieux))
        .child(legende(c.legendes.lieux))
}

/// L'écran des régions installées, avec un téléchargement en cours.
fn maquette_regions(l: Langue) -> Element {
    let c = l.contenu();
    let regions = ul()
        .class("regions")
        .children(MAQUETTE_REGIONS.iter().map(|r| {
            let ligne = li().child(
                div()
                    .class("ligne")
                    .child(span().class("nom").text(r.nom))
                    .child(span().class("poids").text(r.poids)),
            );
            match r.avancement {
                None => ligne.child(
                    p().class("pret")
                        .attr("aria-hidden", "true")
                        .text(c.maquette.installee),
                ),
                Some(pct) => ligne
                    .child(
                        div()
                            .class("jauge")
                            .attr("aria-hidden", "true")
                            .child(span().style(format!("width: {pct}%"))),
                    )
                    .child(
                        p().class("encours")
                            .attr("data-compte", "")
                            .text(format!("{pct} %")),
                    ),
            }
        }));
    maquette()
        .child(div().class("ecran").child(regions))
        .child(legende(c.legendes.regions))
}

// --- Les pièces communes --------------------------------------------------

/// Choix du thème : quatre boutons radio que la feuille de style lit avec
/// `:has()`.
///
/// Toujours sans script, bien qu'on ait le WebAssembly sous la main : le CSS
/// le fait déjà, et le faire en Rust ne gagnerait rien tout en cessant de
/// fonctionner pendant le chargement. Le choix ne survit pas à un changement
/// de page, et la page le dit plutôt que de laisser croire le contraire.
fn themes(l: Langue) -> Element {
    let c = l.contenu();
    fieldset()
        .class("reglages")
        .attr("data-mvt", "monte")
        .child(legend().text(c.theme.libelle))
        .children(THEMES.iter().zip(c.theme.noms.iter()).map(|(t, nom)| {
            let id = format!("th-{t}");
            let bouton = input()
                .attr("type", "radio")
                .attr("name", "theme")
                .id(id.clone());
            span()
                .child(attr_si(bouton, *t == "auto", "checked", ""))
                .child(label().attr("for", id).text(*nom))
        }))
}

/// Les trois langues, la courante marquée comme telle.
fn langues(langue: Langue) -> Element {
    let c = langue.contenu();
    nav()
        .class("langues")
        .attr("aria-label", c.langue_libelle)
        .children(Langue::TOUTES.iter().map(|l| {
            let autre = l.contenu();
            if *l == langue {
                span().attr("aria-current", "true").text(autre.nom)
            } else {
                a().href(autre.chemin)
                    .attr("hreflang", autre.code)
                    .attr("lang", autre.code)
                    .child(roule(autre.nom))
            }
        }))
}

/// La marque : le logo de l'application et son nom.
fn marque(res: &Ressources) -> Element {
    p().class("marque")
        .child(
            img()
                .class("logo")
                .attr("src", res.logo.clone())
                .attr("width", "40")
                .attr("height", "40")
                .attr("alt", ""),
        )
        .text("MonCap GPS")
}

/// Le bouton « Se connecter », magnétique, vers l'application.
fn bouton_connexion(texte: &'static str) -> Element {
    a().class("bouton")
        .href(CONNEXION)
        .attr("data-aimant", "")
        .attr("data-curseur", "→")
        .child(span().child(roule(texte)))
}

/// Le pied complet, partagé entre la page principale et les pages légales.
///
/// Il reprend le bleu du bandeau : la page se ferme comme elle s'ouvre. Il
/// porte aussi le choix de langue, qu'on ne peut pas atteindre autrement
/// après avoir lu jusqu'ici.
///
/// Les ancres visent toujours la page principale (`/#id`, `/es#id`…) : le
/// même pied sert depuis une page légale, où `#id` seul ne mènerait nulle
/// part.
pub fn pied(langue: Langue, res: &Ressources, an: i32) -> Element {
    let c = langue.contenu();
    let colonnes = nav()
        .class("pied-colonnes")
        .attr("data-mvt", "monte")
        .child(
            div()
                .child(h3().text(c.pied.decouvrir))
                .child(ul().children(c.pied.ancres.iter().map(|ancre| {
                    li().child(
                        a().href(format!("{}#{}", c.chemin, ancre.id))
                            .child(roule(ancre.nom)),
                    )
                }))),
        )
        .child(
            div()
                .child(h3().text(c.pied.donnees))
                .child(ul().children(SOURCES.iter().map(|s| {
                    li().child(
                        a().href(s.url)
                            .attr("rel", "external noopener")
                            .attr("data-curseur", "↗")
                            .child(roule(s.nom)),
                    )
                }))),
        )
        .child(
            div().child(h3().text(c.pied.legal_titre)).child(
                ul().child(li().child(a().href(c.mentions.chemin).child(roule(c.mentions.titre))))
                    .child(
                        li().child(
                            a().href(c.confidentialite.chemin)
                                .child(roule(c.confidentialite.titre)),
                        ),
                    ),
            ),
        );
    footer()
        .child(
            div()
                .class("dedans")
                .child(
                    div()
                        .class("pied-haut")
                        .attr("data-mvt", "monte")
                        .child(marque(res))
                        .child(langues(langue)),
                )
                .child(
                    p().class("slogan")
                        .attr("data-mvt", "mots")
                        .child(mots(c.pied.slogan, 0)),
                )
                .child(
                    p().class("pied-liens")
                        .attr("data-mvt", "monte")
                        .child(bouton_connexion(c.se_connecter)),
                )
                .child(colonnes)
                .child(
                    p().class("signature")
                        .text(format!("{} ", c.signature))
                        .child(strong().text(AUTEUR))
                        .text("."),
                )
                .child(p().class("credits").text(c.credits))
                // L'année est un îlot : le navigateur la tient à jour.
                .child(
                    p().class("copyright")
                        .text("© ")
                        .child(island(ILOT_ANNEE, an.to_string(), annee))
                        .text(format!(" {AUTEUR}. {}", c.pied.copyright)),
                ),
        )
        // La signature en très grand, qui monte lettre à lettre et suit la
        // souris : la page se ferme sur la marque.
        .child(
            p().class("mot-geant")
                .attr("aria-hidden", "true")
                .attr("data-mvt", "lettres")
                .attr("data-souris", "")
                .child(lettres("MonCap GPS", 0)),
        )
}

// --- La page d'accueil ----------------------------------------------------

/// Une section de la page d'accueil : son numéro et son titre qui monte mot
/// à mot.
fn section_titree(n: usize, id: &'static str, titre: &'static str) -> Element {
    section()
        .attr("aria-labelledby", id)
        .child(numero(n))
        .child(h2().id(id).attr("data-mvt", "mots").child(mots(titre, 0)))
}

/// Un paragraphe d'introduction, qui monte à son tour.
fn intro(texte: &'static str) -> Element {
    p().class("intro").attr("data-mvt", "monte").text(texte)
}

/// Une liste de cartes en cascade : atouts, piliers du hors-ligne.
fn cartes(liste: &'static [crate::contenu::Atout]) -> Element {
    ul().class("cartes")
        .attr("data-mvt", "cascade")
        .children(liste.iter().enumerate().map(|(k, carte)| {
            li().style(format!("--i:{}", k.min(5)))
                .attr("data-incline", "")
                .child(
                    span()
                        .class("emoji")
                        .attr("aria-hidden", "true")
                        .text(carte.emoji),
                )
                .child(h3().text(carte.titre))
                .child(p().text(carte.texte))
        }))
}

fn accueil(pr: &Props) -> Node {
    let l = pr.langue;
    let c = l.contenu();
    // La mesure est mise en valeur au milieu de la phrase : le gabarit évite
    // de couper le texte en trois morceaux traduits séparément.
    let (avant, apres) = match c.preuve.texte.split_once("{mesure}") {
        Some((av, ap)) => (av, ap),
        None => (c.preuve.texte, ""),
    };

    // Le fond du bandeau va d'un bord à l'autre, mais son texte doit
    // s'aligner sur celui du corps : d'où ce conteneur, de la même largeur
    // que `main`. Il s'éloigne quand on descend (`data-defile` : le moteur
    // lui donne la progression).
    let dedans = div()
        .class("dedans")
        .attr("data-defile", "")
        .child(
            div()
                .class("barre")
                .attr("data-mvt", "monte")
                .child(marque(&pr.res))
                .child(langues(l)),
        )
        // Lettre à lettre pour l'œil ; le titre entier, d'un bloc, pour le
        // lecteur d'écran.
        .child(
            h1().attr("data-mvt", "lettres")
                .child(
                    span()
                        .class("visuel-cache")
                        .text(format!("{} {}", c.h1[0], c.h1[1])),
                )
                .child(
                    span()
                        .attr("aria-hidden", "true")
                        .child(lettres(c.h1[0], 0))
                        .child(br())
                        .child(em().child(lettres(c.h1[1], nb_lettres(c.h1[0])))),
                ),
        )
        .child(
            p().class("chapeau")
                .attr("data-mvt", "monte")
                .text(c.chapeau),
        )
        .child(
            div()
                .class("actions")
                .attr("data-mvt", "monte")
                .child(bouton_connexion(c.se_connecter)),
        )
        // Chaque vignette entre à son tour, et son nombre compte jusqu'à sa
        // valeur.
        .child(dl().class("chiffres").attr("data-mvt", "cascade").children(
            c.chiffres.iter().enumerate().map(|(k, n)| {
                div()
                    .style(format!("--i:{k}"))
                    .attr("data-incline", "10")
                    .child(dt().attr("data-compte", "").text(n.valeur))
                    .child(dd().text(n.legende))
            }),
        ))
        .child(themes(l));

    let entete = header()
        .class("hero")
        .id("haut")
        .child(carte_vivante())
        .child(dedans)
        .child(defilant(l));

    // Le prix a sa section, pas une ligne en petit : il compte autant que les
    // fonctions. Deux cartes côte à côte — Plus mis en avant, Pro pour ceux
    // qui roulent pour travailler. L'essai n'a pas de carte : un délai avant
    // paiement n'est pas une formule, et rien ici ne doit ressembler à du
    // 0 €.
    let formules = div()
        .class("formules")
        .attr("data-mvt", "cascade")
        .children(
            [&c.tarifs.plus, &c.tarifs.pro]
                .iter()
                .enumerate()
                .map(|(k, f)| {
                    div()
                        .class(if k == 0 { "formule vedette" } else { "formule" })
                        .style(format!("--i:{k}"))
                        .attr("data-incline", "4")
                        .child(h3().text(f.nom))
                        .child(
                            p().class("prix")
                                .child(span().attr("data-compte", "").text(f.prix))
                                .text(" ")
                                .child(span().text(f.periode)),
                        )
                        .child(ul().children(f.points.iter().map(|pt| li().text(*pt))))
                }),
        );

    let corps = div()
        .class("dedans")
        .child(
            section_titree(1, "comparateur-titre", c.comparateur.titre)
                .child(intro(c.comparateur.intro))
                .child(maquette_comparateur(l))
                .child(intro(c.comparateur.apres)),
        )
        .child(
            section_titree(2, "guidage-titre", c.guidage.titre)
                .child(intro(c.guidage.intro))
                .child(maquette_guidage(l)),
        )
        .child(
            section_titree(3, "montre-titre", c.montre.titre)
                .child(intro(c.montre.intro))
                .child(maquette_montre(l)),
        )
        .child(
            section_titree(4, "lieux-titre", c.lieux.titre)
                .child(intro(c.lieux.intro))
                .child(maquette_lieux(l)),
        )
        .child(section_titree(5, "atouts-titre", c.atouts_titre).child(cartes(c.atouts)))
        .child(
            section_titree(6, "hors-ligne-titre", c.hors_ligne.titre)
                .child(intro(c.hors_ligne.intro))
                .child(maquette_regions(l))
                .child(cartes(c.hors_ligne_cartes)),
        )
        .child(
            section_titree(7, "tarifs-titre", c.tarifs.titre)
                .class("tarifs")
                .child(intro(c.tarifs.intro))
                .child(formules)
                .child(
                    p().class("note")
                        .attr("data-mvt", "monte")
                        .text(c.tarifs.note),
                ),
        )
        .child(
            section()
                .class("preuve")
                .attr("aria-labelledby", "preuve-titre")
                .attr("data-mvt", "monte")
                .child(numero(8))
                .child(h2().id("preuve-titre").text(c.preuve.titre))
                .child(
                    p().text(avant)
                        .child(strong().attr("data-compte", "").text(c.preuve.mesure))
                        .text(apres),
                )
                .child(trajet_preuve())
                .child(p().class("note").text(c.preuve.note)),
        )
        .child(
            section_titree(9, "pile-titre", c.pile_titre)
                .class("pile")
                .child(
                    dl().attr("data-mvt", "cascade")
                        .children(c.pile.iter().enumerate().map(|(k, pi)| {
                            div()
                                .style(format!("--i:{}", k.min(5)))
                                .child(dt().text(pi.domaine))
                                .child(
                                    dd().child(
                                        ul().children(
                                            pi.outils.iter().enumerate().map(|(n, o)| {
                                                li().style(format!("--k:{n}")).text(*o)
                                            }),
                                        ),
                                    ),
                                )
                        })),
                ),
        )
        .child(
            section_titree(10, "franchise-titre", c.franchise_titre)
                .class("franchise")
                .child(
                    ul().attr("data-mvt", "cascade").children(
                        c.franchise
                            .iter()
                            .enumerate()
                            .map(|(k, f)| li().style(format!("--i:{}", k.min(5))).text(*f)),
                    ),
                ),
        );

    fragment([
        Node::from(lecture()),
        fil_route().into(),
        entete.into(),
        main().child(corps).child(defilant_outils()).into(),
        pied(l, &pr.res, pr.annee).into(),
        crate::fleche::ilot(l),
    ])
}

// --- Les pages légales ----------------------------------------------------

/// Une page légale — mentions ou confidentialité. Même squelette que la page
/// principale : le bandeau bleu, le corps sur papier, le pied complet. Le
/// texte est nu et sans lien : ce qui s'y dit doit rester vrai sans
/// entretien.
fn legale(pr: &Props, page: &'static PageLegale) -> Node {
    let c = pr.langue.contenu();
    let entete = header().class("hero legale").child(
        div()
            .class("dedans")
            .attr("data-defile", "")
            .child(
                div()
                    .class("barre")
                    .attr("data-mvt", "monte")
                    .child(marque(&pr.res)),
            )
            .child(
                h1().attr("data-mvt", "lettres")
                    .child(span().class("visuel-cache").text(page.titre))
                    .child(
                        span()
                            .attr("aria-hidden", "true")
                            .child(lettres(page.titre, 0)),
                    ),
            )
            .child(
                p().class("chapeau")
                    .attr("data-mvt", "monte")
                    .child(a().class("retour").href(c.chemin).child(roule(page.retour))),
            ),
    );
    let corps = div()
        .class("dedans juridique")
        .children(page.sections.iter().map(|s| {
            section()
                .attr("data-mvt", "monte")
                .child(h2().text(s.titre))
                .children(s.corps.iter().map(|para| p().text(*para)))
        }));
    fragment([
        Node::from(lecture()),
        entete.into(),
        main().child(corps).into(),
        pied(pr.langue, &pr.res, pr.annee).into(),
    ])
}

// --- Le décor -------------------------------------------------------------

/// Le fil de l'itinéraire : un tracé qui se dessine à mesure qu'on descend.
///
/// Purement décoratif, et traité comme tel. `pathLength="1"` déclare au
/// navigateur que ce chemin mesure une unité : le pointillé se compte alors
/// en fractions du parcours, là où il aurait fallu mesurer la courbe pour
/// l'animer.
fn fil_route() -> Element {
    svg()
        .class("fil-route")
        .attr("viewBox", "0 0 28 1000")
        .attr("preserveAspectRatio", "none")
        .attr("aria-hidden", "true")
        .attr("focusable", "false")
        .attr("data-defile", "")
        .child(path().attr("pathLength", "1").attr(
            "d",
            "M14 0C6 110 22 190 14 300C6 410 24 470 14 560C4 650 22 720 14 830C7 920 18 960 14 1000",
        ))
}

/// La barre de lecture : un trait en haut de l'écran, qui s'allonge avec la
/// page parcourue (`--defile`).
fn lecture() -> Element {
    div()
        .class("lecture")
        .attr("aria-hidden", "true")
        .attr("data-defile", "")
        .child(span())
}

/// Le trajet de la preuve, Bordeaux → Limoges, qui se trace au défilement
/// (`--p`) avec une voiture en tête.
///
/// La voiture est un tiret de longueur nulle aux bouts arrondis : un point,
/// que `stroke-dashoffset` fait glisser le long du même chemin. Il suit
/// ainsi la courbe à toutes les tailles, là où un élément posé par
/// `offset-path` se compterait en pixels et décrocherait dès que le dessin
/// est mis à l'échelle.
fn trajet_preuve() -> Element {
    const CHEMIN: &str = "M14 70C70 74 92 24 150 30S236 72 286 22";
    let ville = |cx: &'static str, cy: &'static str| {
        circle()
            .class("tp-ville")
            .attr("cx", cx)
            .attr("cy", cy)
            .attr("r", "5")
    };
    div()
        .class("trajet-preuve")
        .attr("aria-hidden", "true")
        .attr("data-scrub", "")
        .child(
            svg()
                .attr("viewBox", "0 0 300 92")
                .attr("focusable", "false")
                .child(path().class("tp-fond").attr("d", CHEMIN))
                .child(
                    path()
                        .class("tp-trace")
                        .attr("pathLength", "1")
                        .attr("d", CHEMIN),
                )
                .child(
                    path()
                        .class("tp-voiture")
                        .attr("pathLength", "1")
                        .attr("d", CHEMIN),
                )
                .child(ville("14", "70"))
                .child(ville("286", "22")),
        )
        .child(span().class("tp-nom tp-depart").text("Bordeaux"))
        .child(span().class("tp-nom tp-arrivee").text("Limoges"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendu(langue: Langue, page: Page) -> String {
        vitrine(&Props {
            langue,
            page,
            res: Ressources {
                logo: "/vitrine/logo.webp".into(),
            },
            annee: 2026,
        })
        .render()
    }

    /// Le texte des titres est découpé pour l'œil, mais le titre entier
    /// reste d'un bloc pour le lecteur d'écran — et pour qui cherche la
    /// phrase dans le HTML, comme le test du serveur.
    #[test]
    fn le_grand_titre_reste_lisible_d_un_bloc() {
        for l in Langue::TOUTES {
            let c = l.contenu();
            let html = rendu(l, Page::Accueil);
            assert!(
                html.contains(&format!(
                    r#"<span class="visuel-cache">{} {}</span>"#,
                    c.h1[0], c.h1[1]
                )),
                "{} : titre introuvable",
                l.code()
            );
        }
    }

    /// L'année est un îlot : le rendu l'écrit, marquée pour être réhydratée.
    #[test]
    fn l_annee_est_un_ilot() {
        let html = rendu(Langue::Fr, Page::Mentions);
        assert!(html.contains(
            r#"<active-island data-island="annee" data-props="2026"><!--#-->2026</active-island>"#
        ));
    }

    /// Un mot par masque, et un blanc entre deux mots — pas après le
    /// dernier, qui décalerait la ligne.
    #[test]
    fn les_mots_sont_separes_d_un_blanc() {
        let html = mots("un deux", 3).render();
        assert_eq!(
            html,
            r#"<span class="mot"><span style="--i:3">un</span></span> <span class="mot"><span style="--i:4">deux</span></span>"#
        );
        assert_eq!(nb_lettres("un deux"), 6);
    }

    /// Le bandeau défilant est doublé, et sa copie cachée aux lecteurs
    /// d'écran : sans quoi chaque promesse serait lue deux fois.
    #[test]
    fn le_double_du_bandeau_est_cache() {
        let html = Node::from(defilant(Langue::Fr)).render();
        assert_eq!(html.matches(r#"class="defilant-suite""#).count(), 2);
        assert_eq!(
            html.matches(r#"class="defilant-suite" aria-hidden="true""#)
                .count(),
            1
        );
    }

    /// Le thème « auto » est coché dans le HTML même : la feuille le lit
    /// avec `:has()`, avant tout script.
    #[test]
    fn le_theme_auto_est_coche_d_avance() {
        let html = Node::from(themes(Langue::Fr)).render();
        assert_eq!(html.matches("checked").count(), 1);
        assert!(html.contains(r#"id="th-auto" checked="""#));
    }
}
