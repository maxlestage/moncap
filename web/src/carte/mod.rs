//! La carte, écrite en Rust : tuiles OpenStreetMap, repères, traces et
//! gestes.
//!
//! Elle remplace Leaflet. Ce n'est pas un caprice de pureté : Leaflet 1.9.4
//! date de 2023, sa version 2 n'est toujours qu'en alpha, et il s'utilisait
//! ici par un objet global `L` chargé depuis un CDN tiers — un script que
//! personne ne relisait, sur un domaine que le site ne contrôle pas. Et ses
//! bulles prenaient du HTML brut : il fallait échapper chaque libellé à la
//! main pour ne pas ouvrir la porte à une injection.
//!
//! Les bulles sont maintenant des nœuds comme les autres : un libellé y est
//! du texte, jamais du balisage, et l'échappement cesse d'être une chose à
//! ne pas oublier.
//!
//! La géométrie vit dans [`projection`], testée sans navigateur. Ce module
//! ne fait que la brancher sur l'écran et sur les doigts.
//!
//! **Le plan.** Tout ce qui est posé sur la carte — tuiles, traces, repères
//! — vit dans un calque unique, déplacé d'un seul `transform` quand on fait
//! glisser la carte. Les repères gardent donc leur position dans le calque
//! pendant un glissement, et la transition CSS qui fait rouler les voitures
//! d'un relevé à l'autre ne se déclenche que lorsqu'elles bougent vraiment.
//!
//! **Le rendu.** Sans DOM virtuel : ce que la carte montre tient dans un
//! seul état, [`Rendu`] — la vue, l'origine du plan, la taille. Chaque
//! tuile et chaque repère est construit une fois, gardé par sa clé, et seul
//! son `style` suit cet état. Une image, ce sont quelques attributs
//! récrits, rien de reconstruit ; une voiture qui avance garde son élément,
//! et sa transition.

pub mod projection;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use gloo_timers::callback::Timeout;
use mouvement::calcul;
use paquets::active::prelude::*;
use paquets::active::Dom;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent, PointerEvent, ResizeObserver, WheelEvent};

use crate::catalogue::{categorie, Ressources};
use crate::navigateur::{self, brut, fenetre};
use crate::types::{Alerte, Coord, Lieu, Position, Voiture};
use projection::{cadre, deprojette, echelle, niveau, projette, tuiles_visibles, Tuile, Vue};

/// Ce qu'on ouvre d'un toucher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Objet {
    Position(i64),
    Voiture(u64),
    Alerte(i64),
    Destination,
}

/// Une demande de cadrage : la carte s'y rend chaque fois que `numero`
/// change. Un numéro plutôt qu'un booléen : deux demandes identiques de
/// suite restent deux demandes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cadrage {
    pub numero: u32,
    pub points: Rc<Vec<Coord>>,
    pub zoom_max: f64,
}

/// Ce que la carte reçoit : les états qu'elle montre, et ce qu'elle dit en
/// retour.
pub struct Proprietes {
    pub positions: State<Rc<Vec<Position>>>,
    pub voitures: State<Rc<Vec<Voiture>>>,
    pub alertes: State<Rc<Vec<Alerte>>>,
    pub moi: State<Option<Coord>>,
    pub destination: State<Option<Lieu>>,
    /// Le point qu'on est en train de nommer…
    pub brouillon: State<Option<Coord>>,
    /// … et le contenu de sa bulle.
    pub bulle_brouillon: Rc<dyn Fn() -> Node>,
    pub cadrage: State<Cadrage>,
    pub on_clic: Rc<dyn Fn(Coord)>,
    pub on_vote: Rc<dyn Fn(i64, bool)>,
    pub on_echap: Rc<dyn Fn()>,
    /// Les adresses des avatars : elles ne changent pas pendant la vie de
    /// la page.
    pub ressources: Rc<Ressources>,
}

/// Ce que la carte dit à son parent — une fois relâché tout ce qu'elle
/// tient, pour que le parent puisse lui répondre aussitôt.
enum Sortie {
    Clic(Coord),
    Vote(i64, bool),
    Echap,
}

/// Ce qu'il faut pour placer quoi que ce soit à l'écran.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rendu {
    vue: Vue,
    /// L'origine du plan, en Mercator normalisé. Elle ne bouge qu'au zoom :
    /// tant qu'elle reste fixe, glisser la carte ne déplace que le plan.
    origine: (f64, f64),
    taille: (f64, f64),
    /// Vrai pendant un zoom : les transitions des repères sont coupées, sans
    /// quoi chaque repère glisserait vers sa nouvelle place au lieu d'y être.
    zoome: bool,
    /// Le niveau de tuiles précédent, gardé un instant sous le nouveau pour
    /// qu'un changement de niveau ne montre jamais de trou.
    niveau_prec: Option<u32>,
}

impl Rendu {
    /// Le décalage du plan : là où son origine tombe à l'écran.
    ///
    /// Arrondi au pixel. Translaté d'une fraction, le plan pose le bord de
    /// chaque tuile entre deux pixels ; le navigateur l'adoucit, les deux
    /// tuiles voisines deviennent chacune à moitié transparentes sur leur
    /// bord commun, et le fond de la carte passe entre elles en un liseré
    /// clair — constaté à l'image.
    fn translation_plan(&self) -> (f64, f64) {
        let s = echelle(self.vue.zoom);
        (
            (self.taille.0 / 2.0 + (self.origine.0 - self.vue.centre.0) * s).round(),
            (self.taille.1 / 2.0 + (self.origine.1 - self.vue.centre.1) * s).round(),
        )
    }

    /// La place d'un point dans le plan.
    fn dans_plan(&self, c: Coord) -> (f64, f64) {
        let (x, y) = projette(c);
        let s = echelle(self.vue.zoom);
        ((x - self.origine.0) * s, (y - self.origine.1) * s)
    }

    fn place(&self, c: Coord) -> String {
        let (x, y) = self.dans_plan(c);
        format!("transform:translate3d({x:.1}px,{y:.1}px,0)")
    }

    /// Les bords sont arrondis à partir des positions, et non la taille à
    /// part : deux tuiles voisines partagent exactement le même bord, et
    /// aucun liseré clair ne passe entre elles aux zooms fractionnaires.
    fn style_tuile(&self, t: Tuile) -> String {
        let s = echelle(self.vue.zoom);
        let n = (1u64 << t.z) as f64;
        let (cx, cy) = t.coin();
        let x0 = ((cx - self.origine.0) * s).round();
        let y0 = ((cy - self.origine.1) * s).round();
        let x1 = ((cx + 1.0 / n - self.origine.0) * s).round();
        let y1 = ((cy + 1.0 / n - self.origine.1) * s).round();
        format!(
            "transform:translate3d({x0}px,{y0}px,0);width:{}px;height:{}px",
            x1 - x0,
            y1 - y0
        )
    }

    /// Les tuiles d'un niveau, à afficher.
    fn tuiles(&self, z: u32) -> Vec<Tuile> {
        tuiles_visibles(&self.vue, self.taille, z)
    }

    /// Un tracé SVG passant par ces points.
    fn chemin(&self, points: impl Iterator<Item = Coord>) -> String {
        let mut d = String::new();
        for (i, c) in points.enumerate() {
            let (x, y) = self.dans_plan(c);
            d.push_str(&format!("{}{x:.1} {y:.1}", if i == 0 { "M" } else { "L" }));
        }
        d
    }
}

/// Le geste en cours.
enum Geste {
    Aucun,
    Glisse {
        x: f64,
        y: f64,
        depart: (f64, f64),
        t0: f64,
    },
    Pince {
        ecart0: f64,
        zoom0: f64,
        fixe: (f64, f64),
    },
}

/// Ce que la carte anime d'elle-même.
enum Animation {
    /// La carte lâchée continue sur sa lancée, et ralentit.
    Inertie { vx: f64, vy: f64 },
    /// Un déplacement vers une vue, selon une courbe — celle de la maison,
    /// sauf pour l'arrivée. `t0` peut être dans le futur : le vol attend.
    Vol {
        de: Vue,
        vers: Vue,
        t0: Option<f64>,
        duree_ms: f64,
        courbe: fn(f64) -> f64,
    },
    /// Le zoom à la molette ou aux boutons, lissé vers sa cible.
    Zoom { cible: f64, ancre: (f64, f64) },
    /// L'arrivée, en attente que la carte soit dévoilée : un vol joué
    /// derrière le volet ou sous le masque du dévoilement serait perdu.
    /// `guet` est le bloc révélé par le moteur (`data-mvt`) qui la contient.
    Arrivee {
        guet: web_sys::Element,
        t0: Option<f64>,
    },
}

/// Un toucher en attente : est-ce le premier d'un double toucher ?
struct Toucher {
    x: f64,
    y: f64,
    t: f64,
    _minuteur: Timeout,
}

/// Tout ce que la carte tient entre deux événements.
struct Interne {
    p: Rc<Proprietes>,
    rendu: State<Rendu>,
    ouvert: State<Option<Objet>>,
    moi: Weak<RefCell<Interne>>,
    noeud: Option<web_sys::Element>,
    vue: Vue,
    taille: (f64, f64),
    origine: (f64, f64),
    zoome: bool,
    niveau_prec: Option<u32>,
    pointeurs: Vec<(i32, f64, f64)>,
    geste: Geste,
    echantillons: VecDeque<(f64, f64, f64)>,
    animation: Option<Animation>,
    raf: Option<i32>,
    boucle: Option<Closure<dyn FnMut(f64)>>,
    image_prec: Option<f64>,
    fin_zoom: Option<Timeout>,
    fin_niveau: Option<Timeout>,
    toucher: Option<Toucher>,
    observateur: Option<(ResizeObserver, Closure<dyn FnMut()>)>,
    cadrage_vu: u32,
    cadrage_en_attente: bool,
    mouvement: bool,
    eteinte: bool,
}

/// La France entière, comme l'ancienne carte au démarrage.
const DEPART: Coord = Coord {
    lat: 46.6,
    lon: 2.5,
};
const ZOOM_DEPART: f64 = 5.0;
/// De combien de niveaux la carte descend en arrivant, et en combien de
/// temps.
const ZOOM_ARRIVEE: f64 = 2.5;
const DELAI_ARRIVEE_MS: f64 = 350.0;
const DUREE_ARRIVEE_MS: f64 = 1900.0;
/// L'arrivée n'attend pas le dévoilement plus longtemps : passé ce délai,
/// la carte se pose directement sur sa vue.
const ATTENTE_ARRIVEE_MS: f64 = 4000.0;

/// Lente au départ, lente à l'arrivée : la courbe de l'arrivée.
fn en_douceur(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (2.0 - 2.0 * t).powi(3) / 2.0
    }
}

/// Agit sur la carte, publie ce qui a changé, puis transmet au parent ce
/// que la carte a à lui dire — une fois tout relâché.
fn agit(interne: &Rc<RefCell<Interne>>, f: impl FnOnce(&mut Interne) -> Option<Sortie>) {
    let (sortie, p) = {
        let Ok(mut i) = interne.try_borrow_mut() else {
            return;
        };
        if i.eteinte {
            return;
        }
        let sortie = f(&mut i);
        i.publie();
        (sortie, i.p.clone())
    };
    match sortie {
        Some(Sortie::Clic(c)) => (p.on_clic)(c),
        Some(Sortie::Vote(id, oui)) => (p.on_vote)(id, oui),
        Some(Sortie::Echap) => (p.on_echap)(),
        None => {}
    }
}

/// Comme [`agit`], depuis un minuteur ou un rappel qui a pu survivre à la
/// carte.
fn agit_faible(faible: &Weak<RefCell<Interne>>, f: impl FnOnce(&mut Interne) -> Option<Sortie>) {
    if let Some(i) = faible.upgrade() {
        agit(&i, f);
    }
}

/// L'événement de pointeur derrière un événement d'`active`.
fn pointeur(e: &Event) -> Option<PointerEvent> {
    brut(e)?.dyn_into().ok()
}

/// Le contenu d'une bulle ouverte : ce qui, s'il change, la redessine.
#[derive(Clone, Debug, PartialEq)]
enum Contenu {
    Position {
        rang: usize,
        label: String,
        coord: Coord,
    },
    Voiture(String),
    Destination(String),
    Alerte {
        id: i64,
        titre: String,
        confirms: i32,
        denies: i32,
    },
}

/// Où se trouve l'objet ouvert, et ce que sa bulle dit. `None` s'il a
/// disparu entre-temps.
fn coord_objet(o: Objet, p: &Proprietes) -> Option<Coord> {
    match o {
        Objet::Position(id) => p
            .positions
            .with(|v| v.iter().find(|q| q.id == id).map(Position::coord)),
        Objet::Voiture(id) => p
            .voitures
            .with(|v| v.iter().find(|x| x.id == id).map(Voiture::coord)),
        Objet::Alerte(id) => p
            .alertes
            .with(|v| v.iter().find(|a| a.id == id).map(Alerte::coord)),
        Objet::Destination => p.destination.with(|d| d.as_ref().map(|d| d.coord)),
    }
}

fn contenu_objet(o: Objet, p: &Proprietes) -> Option<Contenu> {
    match o {
        Objet::Position(id) => p.positions.with(|v| {
            v.iter()
                .enumerate()
                .find(|(_, q)| q.id == id)
                .map(|(i, q)| Contenu::Position {
                    rang: i + 1,
                    label: q.label.clone(),
                    coord: q.coord(),
                })
        }),
        Objet::Voiture(id) => p.voitures.with(|v| {
            v.iter()
                .find(|x| x.id == id)
                .map(|x| Contenu::Voiture(x.label.clone()))
        }),
        Objet::Destination => p
            .destination
            .with(|d| d.as_ref().map(|d| Contenu::Destination(d.nom_court()))),
        Objet::Alerte(id) => p.alertes.with(|v| {
            v.iter().find(|a| a.id == id).map(|a| {
                let c = categorie(&a.category);
                let titre = if a.label.is_empty() {
                    c.nom.to_string()
                } else {
                    a.label.clone()
                };
                Contenu::Alerte {
                    id,
                    titre: format!("{} {titre}", c.emoji),
                    confirms: a.confirms,
                    denies: a.denies,
                }
            })
        }),
    }
}

/// La carte.
pub fn carte(p: Proprietes) -> Node {
    let p = Rc::new(p);
    let vue = Vue::sur(DEPART, ZOOM_DEPART);
    let rendu = use_state(Rendu {
        vue,
        origine: vue.centre,
        taille: (0.0, 0.0),
        zoome: false,
        niveau_prec: None,
    });
    let ouvert = use_state(None::<Objet>);
    let interne = Rc::new_cyclic(|moi| {
        RefCell::new(Interne {
            p: p.clone(),
            rendu,
            ouvert,
            moi: moi.clone(),
            noeud: None,
            vue,
            taille: (0.0, 0.0),
            origine: vue.centre,
            zoome: false,
            niveau_prec: None,
            pointeurs: Vec::new(),
            geste: Geste::Aucun,
            echantillons: VecDeque::new(),
            animation: None,
            raf: None,
            boucle: None,
            image_prec: None,
            fin_zoom: None,
            fin_niveau: None,
            toucher: None,
            observateur: None,
            cadrage_vu: 0,
            cadrage_en_attente: false,
            mouvement: navigateur::mouvement_permis(),
            eteinte: false,
        })
    });

    // Partie avec la page : tout ce qui tourne encore s'arrête.
    {
        let interne = interne.clone();
        on_cleanup(move || {
            if let Ok(mut i) = interne.try_borrow_mut() {
                i.eteins();
            }
        });
    }

    // Le cadrage demandé par le parent. Un premier cadrage demandé avant
    // que la carte ait une taille attend qu'elle en ait une.
    {
        let interne = interne.clone();
        let cadrage = p.cadrage;
        effect(move || {
            let numero = cadrage.with(|c| c.numero);
            untrack(|| {
                agit(&interne, |i| {
                    if numero != 0 && numero != i.cadrage_vu {
                        i.cadrage_en_attente = true;
                        i.applique_cadrage();
                    }
                    None
                })
            });
        });
    }

    let faible = Rc::downgrade(&interne);
    // Une bulle vient de s'ouvrir : on vérifie qu'elle tient dans la carte,
    // et l'on décale la carte sinon — ce que faisait l'`autoPan` de Leaflet.
    let verifie_bulle = {
        let faible = faible.clone();
        move |_: &Dom| {
            let faible = faible.clone();
            Timeout::new(0, move || {
                agit_faible(&faible, |i| {
                    i.garde_la_bulle_visible();
                    None
                })
            })
            .forget();
        }
    };

    let a_moi = memo({
        let moi = p.moi;
        move || moi.with(Option::is_some)
    });

    // --- Le plan ------------------------------------------------------------
    let tuiles_avant = keyed(
        move || {
            rendu.with(|r| {
                r.niveau_prec
                    .filter(|z| z.abs_diff(niveau(r.vue.zoom)) <= 2)
                    .map(|z| r.tuiles(z))
                    .unwrap_or_default()
            })
        },
        |t| *t,
        move |t| tuile(rendu, *t),
    );
    let tuiles = keyed(
        move || rendu.with(|r| r.tuiles(niveau(r.vue.zoom))),
        |t| *t,
        move |t| tuile(rendu, *t),
    );

    let plan = div()
        .class("carte-plan")
        .attr_dyn("style", move || {
            let (tx, ty) = rendu.with(Rendu::translation_plan);
            format!("transform:translate3d({tx}px,{ty}px,0)")
        })
        .child(div().class("tuiles tuiles-avant").child(tuiles_avant))
        .child(div().class("tuiles").child(tuiles))
        .child(traces(&p, rendu))
        .child(reperes(&p, rendu, ouvert, a_moi))
        .child(bulle(&p, rendu, ouvert, &interne, verifie_bulle.clone()))
        .child(brouillon(&p, rendu, verifie_bulle));

    // --- Les commandes -------------------------------------------------------
    let bouton_zoom = |libelle: &'static str, texte: &'static str, d: f64| {
        let interne = interne.clone();
        button()
            .attr("type", "button")
            .attr("aria-label", libelle)
            .on_click(move |_| {
                agit(&interne, |i| {
                    i.bouton(d);
                    None
                })
            })
            .text(texte)
    };
    let commandes = div()
        .class("carte-commandes")
        .attr("data-carte-ui", "")
        .child(bouton_zoom("Zoomer", "+", 1.0))
        .child(bouton_zoom("Dézoomer", "−", -1.0))
        .children_dyn({
            let interne = interne.clone();
            move || {
                if !a_moi.get() {
                    return Vec::new();
                }
                let interne = interne.clone();
                vec![button()
                    .attr("type", "button")
                    .class("recentrer")
                    .attr("aria-label", "Recentrer sur ma position")
                    .on_click(move |_| {
                        agit(&interne, |i| {
                            i.recentre();
                            None
                        })
                    })
                    .child(span().text("➤"))
                    .into()]
            }
        });

    // L'attribution que demande la licence ODbL, avec son lien.
    let credit = p_()
        .class("carte-credit")
        .attr("data-carte-ui", "")
        .text("© ")
        .child(
            a().href("https://www.openstreetmap.org/copyright")
                .attr("target", "_blank")
                .attr("rel", "noopener")
                .text("les contributeurs d'OpenStreetMap"),
        );

    // --- Les gestes ----------------------------------------------------------
    let sur = |interne: &Rc<RefCell<Interne>>,
               f: fn(&mut Interne, PointerEvent) -> Option<Sortie>| {
        let interne = interne.clone();
        move |e: Event| {
            if let Some(e) = pointeur(&e) {
                agit(&interne, |i| f(i, e));
            }
        }
    };

    div()
        .class("carte")
        .class_if("zoome", move || rendu.with(|r| r.zoome))
        .attr("tabindex", "0")
        .attr("role", "application")
        .attr(
            "aria-label",
            "Carte. Flèches pour la déplacer, plus et moins pour zoomer, Entrée pour poser une position au centre.",
        )
        .attr("data-defile-libre", "")
        .attr("data-mvt-calme", "")
        .on("pointerdown", sur(&interne, |i, e| i.bas(e)))
        .on("pointermove", sur(&interne, |i, e| i.bouge(e)))
        .on("pointerup", sur(&interne, |i, e| i.haut(e, true)))
        .on("pointercancel", sur(&interne, |i, e| i.haut(e, false)))
        .on("wheel", {
            let interne = interne.clone();
            move |e: Event| {
                if let Some(e) = brut(&e).and_then(|e| e.dyn_into::<WheelEvent>().ok()) {
                    agit(&interne, |i| {
                        i.molette(e);
                        None
                    });
                }
            }
        })
        .on("keydown", {
            let interne = interne.clone();
            move |e: Event| {
                if let Some(e) = brut(&e).and_then(|e| e.dyn_into::<KeyboardEvent>().ok()) {
                    agit(&interne, |i| i.clavier(e));
                }
            }
        })
        .on_mount({
            let interne = interne.clone();
            move |dom: &Dom| {
                if let Some(el) = navigateur::element(dom) {
                    branche(&interne, el);
                }
            }
        })
        .child(plan)
        .child(commandes)
        .child(credit)
        .into()
}

/// `p` est pris par le paramètre des propriétés : l'élément `<p>` passe par
/// ici.
fn p_() -> Element {
    el("p")
}

/// Branche la carte sur son élément : sa taille, et ce qui la suit.
fn branche(interne: &Rc<RefCell<Interne>>, el: web_sys::Element) {
    let faible = Rc::downgrade(interne);
    let rappel = Closure::<dyn FnMut()>::new(move || {
        agit_faible(&faible, |i| {
            i.taille();
            None
        })
    });
    let Ok(mut i) = interne.try_borrow_mut() else {
        return;
    };
    if let Ok(ro) = ResizeObserver::new(rappel.as_ref().unchecked_ref()) {
        ro.observe(&el);
        i.observateur = Some((ro, rappel));
    }
    i.noeud = Some(el);
    // L'observateur se manifeste à son premier tour ; la taille est prise
    // dès maintenant quand même, au cas où.
    let faible = Rc::downgrade(interne);
    Timeout::new(0, move || {
        agit_faible(&faible, |i| {
            i.taille();
            None
        })
    })
    .forget();
}

/// Une tuile : construite une fois, déplacée par son `style`.
fn tuile(rendu: State<Rendu>, t: Tuile) -> Node {
    img()
        .class("tuile")
        .attr("src", t.url())
        .attr("alt", "")
        .attr("draggable", "false")
        .attr("decoding", "async")
        .attr_dyn("style", move || rendu.with(|r| r.style_tuile(t)))
        .on("load", |e: Event| {
            if let Some(img) = brut(&e)
                .and_then(|e| e.target())
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            {
                let _ = img.set_attribute("data-chargee", "");
            }
        })
        .into()
}

fn traces(p: &Rc<Proprietes>, rendu: State<Rendu>) -> Node {
    let positions = p.positions;
    // La signature des points : un nouvel itinéraire remonte le tracé, qui
    // se dessine à nouveau ; un simple zoom le garde.
    let signature = memo(move || {
        positions.with(|v| {
            (v.len() >= 2).then(|| {
                v.iter()
                    .map(|q| format!("{:.5},{:.5}", q.lat, q.lon))
                    .collect::<Vec<_>>()
                    .join(";")
            })
        })
    });
    let a_trace = memo(move || signature.with(Option::is_some));
    let d_itineraire = move || {
        let r = rendu.get();
        positions.with(|v| {
            if v.len() >= 2 {
                r.chemin(v.iter().map(Position::coord))
            } else {
                String::new()
            }
        })
    };
    let (moi, destination) = (p.moi, p.destination);
    let vers_destination =
        memo(move || moi.with(Option::is_some) && destination.with(Option::is_some));

    svg()
        .class("traces")
        .attr("width", "1")
        .attr("height", "1")
        .attr("aria-hidden", "true")
        .attr("focusable", "false")
        .children_dyn(move || {
            if !a_trace.get() {
                return Vec::new();
            }
            vec![path()
                .class("trace-halo")
                .attr_dyn("d", d_itineraire)
                .into()]
        })
        .child(keyed(
            move || signature.get().into_iter().collect::<Vec<_>>(),
            |s| s.clone(),
            move |_| {
                path()
                    .class("trace")
                    .attr("pathLength", "1")
                    .attr_dyn("d", d_itineraire)
                    .into()
            },
        ))
        .children_dyn(move || {
            if !vers_destination.get() {
                return Vec::new();
            }
            vec![path()
                .class("trace-destination")
                .attr_dyn("d", move || {
                    let r = rendu.get();
                    match (moi.get(), destination.with(|d| d.as_ref().map(|d| d.coord))) {
                        (Some(m), Some(d)) => r.chemin([m, d].into_iter()),
                        _ => String::new(),
                    }
                })
                .into()]
        })
        .into()
}

fn reperes(
    p: &Rc<Proprietes>,
    rendu: State<Rendu>,
    ouvert: State<Option<Objet>>,
    a_moi: State<bool>,
) -> Node {
    let moi = p.moi;
    // Le point bleu passe dessous : un signalement posé là où l'on se
    // trouve doit rester lisible, l'onde du point dépasse de toute façon.
    let point_bleu = fragment_dyn(move || {
        if !a_moi.get() {
            return Vec::new();
        }
        vec![span()
            .class("moi")
            .attr_dyn("style", move || {
                moi.get()
                    .map(|c| rendu.with(|r| r.place(c)))
                    .unwrap_or_default()
            })
            .attr("aria-hidden", "true")
            .child(span())
            .into()]
    });

    let positions = p.positions;
    let liste_positions = keyed(
        move || positions.with(|v| v.iter().cloned().enumerate().collect::<Vec<_>>()),
        |(_, q)| q.id,
        move |(i, q)| {
            let (id, c, rang) = (q.id, q.coord(), i + 1);
            button()
                .attr("type", "button")
                .class("repere")
                .attr("data-carte-ui", "")
                .attr_dyn("style", move || rendu.with(|r| r.place(c)))
                .attr("aria-label", format!("Position {rang} : {}", q.label))
                .on_click(move |_| ouvert.set(Some(Objet::Position(id))))
                .child(span().class("goutte").child(span().text(rang.to_string())))
                .into()
        },
    );

    let alertes = p.alertes;
    let liste_alertes = keyed(
        move || alertes.with(|v| v.iter().cloned().collect::<Vec<_>>()),
        |a| a.id,
        move |a| {
            let (id, c) = (a.id, a.coord());
            let cat = categorie(&a.category);
            button()
                .attr("type", "button")
                .class("signal")
                .attr("data-carte-ui", "")
                .attr_dyn("style", move || rendu.with(|r| r.place(c)))
                .attr("aria-label", format!("Signalement : {}", cat.nom))
                .on_click(move |_| ouvert.set(Some(Objet::Alerte(id))))
                .child(span().class("signal-pastille").text(cat.emoji))
                .into()
        },
    );

    let destination = p.destination;
    let drapeau = fragment_dyn(move || {
        let Some(d) = destination.get() else {
            return Vec::new();
        };
        let c = d.coord;
        vec![button()
            .attr("type", "button")
            .class("drapeau")
            .attr("data-carte-ui", "")
            .attr_dyn("style", move || rendu.with(|r| r.place(c)))
            .attr("aria-label", format!("Destination : {}", d.nom_court()))
            .on_click(move |_| ouvert.set(Some(Objet::Destination)))
            .child(span().text("🎯"))
            .into()]
    });

    // Une voiture est gardée par son identifiant, et seul son `style` suit
    // ses relevés : l'élément reste, la transition CSS la fait rouler.
    let voitures = p.voitures;
    let res = p.ressources.clone();
    let liste_voitures = keyed(
        move || {
            voitures.with(|v| {
                v.iter()
                    .map(|x| (x.id, x.label.clone(), x.avatar.clone()))
                    .collect::<Vec<_>>()
            })
        },
        |(id, _, _)| *id,
        move |(id, label, avatar)| {
            let id = *id;
            let derniere = Cell::new(None::<Coord>);
            button()
                .attr("type", "button")
                .class("voiture")
                .attr("data-carte-ui", "")
                .attr_dyn("style", move || {
                    // Partie, elle garde sa dernière place le temps d'être
                    // retirée.
                    let c = voitures
                        .with(|v| v.iter().find(|x| x.id == id).map(Voiture::coord))
                        .or(derniere.get());
                    derniere.set(c);
                    c.map(|c| rendu.with(|r| r.place(c))).unwrap_or_default()
                })
                .attr("aria-label", format!("En direct : {label}"))
                .on_click(move |_| ouvert.set(Some(Objet::Voiture(id))))
                .child(
                    img()
                        .attr("src", res.avatar(avatar))
                        .attr("alt", "")
                        .attr("draggable", "false"),
                )
                .into()
        },
    );

    fragment([
        point_bleu,
        liste_positions,
        liste_alertes,
        drapeau,
        liste_voitures,
    ])
}

fn bulle(
    p: &Rc<Proprietes>,
    rendu: State<Rendu>,
    ouvert: State<Option<Objet>>,
    interne: &Rc<RefCell<Interne>>,
    verifie: impl Fn(&Dom) + Clone + 'static,
) -> Node {
    let contenu = memo({
        let p = p.clone();
        move || ouvert.get().and_then(|o| contenu_objet(o, &p))
    });
    let p = p.clone();
    let interne = interne.clone();
    fragment_dyn(move || {
        let Some(c) = contenu.get() else {
            return Vec::new();
        };
        untrack(|| {
            let corps: Vec<Node> = match c {
                Contenu::Position { rang, label, coord } => vec![
                    p_().class("bulle-titre")
                        .text(format!("{rang}. {label}"))
                        .into(),
                    p_().class("bulle-detail")
                        .text(format!("{:.4}, {:.4}", coord.lat, coord.lon))
                        .into(),
                ],
                Contenu::Voiture(label) => vec![p_().class("bulle-titre").text(label).into()],
                Contenu::Destination(nom) => vec![p_().class("bulle-titre").text(nom).into()],
                Contenu::Alerte {
                    id,
                    titre,
                    confirms,
                    denies,
                } => {
                    let vote = |oui: bool, classe: &'static str, texte: &'static str| {
                        let interne = interne.clone();
                        button()
                            .attr("type", "button")
                            .class(classe)
                            .on_click(move |_| agit(&interne, |_| Some(Sortie::Vote(id, oui))))
                            .text(texte)
                    };
                    vec![
                        p_().class("bulle-titre").text(titre).into(),
                        p_().class("bulle-detail")
                            .text(format!("👍 {confirms} · 👎 {denies}"))
                            .into(),
                        div()
                            .class("bulle-votes")
                            .child(vote(true, "petit", "👍 Toujours là"))
                            .child(vote(false, "petit secondaire", "👎 Plus là"))
                            .into(),
                    ]
                }
            };
            let p = p.clone();
            vec![div()
                .class("bulle")
                .attr("data-carte-ui", "")
                .attr_dyn("style", move || {
                    ouvert
                        .get()
                        .and_then(|o| coord_objet(o, &p))
                        .map(|c| rendu.with(|r| r.place(c)))
                        .unwrap_or_default()
                })
                .on_mount(verifie.clone())
                .child(
                    div()
                        .class("bulle-corps")
                        .attr("role", "dialog")
                        .child(
                            button()
                                .attr("type", "button")
                                .class("bulle-fermer")
                                .attr("aria-label", "Fermer")
                                .on_click(move |_| ouvert.set(None))
                                .text("✕"),
                        )
                        .children(corps),
                )
                .into()]
        })
    })
}

fn brouillon(
    p: &Rc<Proprietes>,
    rendu: State<Rendu>,
    verifie: impl Fn(&Dom) + Clone + 'static,
) -> Node {
    let brouillon = p.brouillon;
    let contenu = p.bulle_brouillon.clone();
    fragment_dyn(move || {
        let Some(c) = brouillon.get() else {
            return Vec::new();
        };
        untrack(|| {
            let place = move || rendu.with(|r| r.place(c));
            vec![
                span()
                    .class("repere brouillon")
                    .attr_dyn("style", place)
                    .attr("aria-hidden", "true")
                    .child(span().class("goutte").child(span().text("+")))
                    .into(),
                div()
                    .class("bulle")
                    .attr("data-carte-ui", "")
                    .attr_dyn("style", place)
                    .on_mount(verifie.clone())
                    .child(div().class("bulle-corps").child(contenu()))
                    .into(),
            ]
        })
    })
}

impl Interne {
    /// Rend visible ce qui a changé : un seul état, que suivent les tuiles
    /// et les repères.
    fn publie(&self) {
        let r = Rendu {
            vue: self.vue,
            origine: self.origine,
            taille: self.taille,
            zoome: self.zoome,
            niveau_prec: self.niveau_prec,
        };
        if self.rendu.is_alive() && untrack(|| self.rendu.with(|x| *x != r)) {
            self.rendu.set(r);
        }
    }

    fn eteins(&mut self) {
        self.eteinte = true;
        if let Some((ro, _)) = self.observateur.take() {
            ro.disconnect();
        }
        if let (Some(id), Some(f)) = (self.raf.take(), fenetre()) {
            let _ = f.cancel_animation_frame(id);
        }
        self.animation = None;
        self.fin_zoom = None;
        self.fin_niveau = None;
        self.toucher = None;
    }

    // --- Géométrie ------------------------------------------------------------

    fn position_locale(&self, client_x: f64, client_y: f64) -> (f64, f64) {
        match &self.noeud {
            Some(el) => {
                let r = el.get_bounding_client_rect();
                (client_x - r.left(), client_y - r.top())
            }
            None => (client_x, client_y),
        }
    }

    fn coord_ecran(&self, e: (f64, f64)) -> Coord {
        let (x, y) = self.vue.depuis_ecran(e, self.taille);
        deprojette(x.rem_euclid(1.0), y)
    }

    /// Change la vue, et tient à jour ce qui en dépend : le niveau de
    /// tuiles précédent, l'origine du plan, le drapeau de zoom.
    fn regle_vue(&mut self, v: Vue) {
        let ancien = niveau(self.vue.zoom);
        if niveau(v.zoom) != ancien {
            self.niveau_prec = Some(ancien);
            let faible = self.moi.clone();
            self.fin_niveau = Some(Timeout::new(700, move || {
                agit_faible(&faible, |i| {
                    i.niveau_prec = None;
                    i.fin_niveau = None;
                    None
                })
            }));
        }
        let s = echelle(v.zoom);
        let loin = ((v.centre.0 - self.origine.0) * s).abs() > 1e6
            || ((v.centre.1 - self.origine.1) * s).abs() > 1e6;
        if v.zoom != self.vue.zoom || loin {
            self.origine = v.centre;
            self.zoome = true;
            let faible = self.moi.clone();
            self.fin_zoom = Some(Timeout::new(220, move || {
                agit_faible(&faible, |i| {
                    i.zoome = false;
                    i.fin_zoom = None;
                    None
                })
            }));
        }
        self.vue = v;
    }

    /// Décale la carte pour qu'une bulle ouverte y tienne en entier.
    fn garde_la_bulle_visible(&mut self) {
        let Some(carte) = self.noeud.clone() else {
            return;
        };
        let Ok(Some(bulle)) = carte.query_selector(".bulle-corps") else {
            return;
        };
        let (c, b) = (
            carte.get_bounding_client_rect(),
            bulle.get_bounding_client_rect(),
        );
        // La bulle entre en grandissant depuis sa pointe : mesurée à cet
        // instant, elle est un peu plus petite qu'elle ne le sera. La marge
        // couvre l'écart.
        let marge = 18.0;
        let decale = |debut: f64, fin: f64, min: f64, max: f64| {
            if debut < min + marge {
                min + marge - debut
            } else if fin > max - marge {
                (max - marge - fin).max(min + marge - debut)
            } else {
                0.0
            }
        };
        let mut dx = decale(b.left(), b.right(), c.left(), c.right());
        let dy = decale(b.top(), b.bottom(), c.top(), c.bottom());
        // Les boutons de zoom flottent au-dessus du plan, dans le coin : une
        // bulle qui tomberait dessous serait inatteignable. On la pousse à
        // leur droite.
        if let Ok(Some(commandes)) = carte.query_selector(".carte-commandes") {
            let k = commandes.get_bounding_client_rect();
            let (gauche, haut, bas) = (b.left() + dx, b.top() + dy, b.bottom() + dy);
            let chevauche = gauche < k.right() + 8.0 && haut < k.bottom() + 8.0 && bas > k.top();
            if chevauche {
                dx += k.right() + 8.0 - gauche;
            }
        }
        if dx != 0.0 || dy != 0.0 {
            let vers = self.vue.glisse(dx, dy);
            self.vole(vers);
        }
    }

    fn taille(&mut self) {
        let Some(el) = self.noeud.clone() else {
            return;
        };
        let taille = (el.client_width() as f64, el.client_height() as f64);
        if taille == self.taille {
            return;
        }
        let premiere = self.taille.0 == 0.0;
        self.taille = taille;
        if self.cadrage_en_attente {
            self.applique_cadrage();
        } else if premiere && taille.0 > 0.0 && self.mouvement {
            self.prepare_arrivee(&el);
        }
    }

    // --- Gestes ---------------------------------------------------------------

    fn bas(&mut self, e: PointerEvent) -> Option<Sortie> {
        // Ce qui est posé sur la carte — repères, bulles, boutons — reçoit
        // ses propres clics : le geste de carte ne démarre pas dessus.
        let sur_ui = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|el| el.closest("[data-carte-ui]").ok().flatten())
            .is_some();
        if sur_ui || (e.pointer_type() == "mouse" && e.button() != 0) {
            return None;
        }
        if let Some(el) = self.noeud.as_ref().and_then(|n| n.dyn_ref::<HtmlElement>()) {
            let _ = el.set_pointer_capture(e.pointer_id());
            let _ = el.focus();
        }
        let (x, y) = self.position_locale(e.client_x() as f64, e.client_y() as f64);
        self.pointeurs.retain(|(id, _, _)| *id != e.pointer_id());
        self.pointeurs.push((e.pointer_id(), x, y));
        self.animation = None;
        self.commence_geste();
        None
    }

    fn commence_geste(&mut self) {
        let t = navigateur::horloge_ms();
        self.echantillons.clear();
        self.geste = match self.pointeurs.as_slice() {
            [(_, x, y)] => {
                self.echantillons.push_back((t, *x, *y));
                Geste::Glisse {
                    x: *x,
                    y: *y,
                    depart: (*x, *y),
                    t0: t,
                }
            }
            [(_, x1, y1), (_, x2, y2), ..] => {
                let milieu = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
                Geste::Pince {
                    ecart0: ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt().max(1.0),
                    zoom0: self.vue.zoom,
                    fixe: self.vue.depuis_ecran(milieu, self.taille),
                }
            }
            [] => Geste::Aucun,
        };
    }

    fn bouge(&mut self, e: PointerEvent) -> Option<Sortie> {
        let i = self
            .pointeurs
            .iter()
            .position(|(id, _, _)| *id == e.pointer_id())?;
        let (x, y) = self.position_locale(e.client_x() as f64, e.client_y() as f64);
        self.pointeurs[i] = (e.pointer_id(), x, y);
        match &mut self.geste {
            Geste::Glisse { x: gx, y: gy, .. } => {
                let (dx, dy) = (x - *gx, y - *gy);
                *gx = x;
                *gy = y;
                let t = navigateur::horloge_ms();
                self.echantillons.push_back((t, x, y));
                while self
                    .echantillons
                    .front()
                    .is_some_and(|(t0, _, _)| t - t0 > 120.0)
                {
                    self.echantillons.pop_front();
                }
                self.vue = self.vue.glisse(dx, dy);
            }
            Geste::Pince {
                ecart0,
                zoom0,
                fixe,
            } => {
                let [(_, x1, y1), (_, x2, y2), ..] = self.pointeurs.as_slice() else {
                    return None;
                };
                let ecart = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt().max(1.0);
                let milieu = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
                let zoom = (*zoom0 + (ecart / *ecart0).log2())
                    .clamp(projection::ZOOM_MIN, projection::ZOOM_MAX);
                let fixe = *fixe;
                // Le point pris entre les doigts reste entre les doigts :
                // on zoome et l'on glisse dans le même geste.
                let s = echelle(zoom);
                let v = Vue {
                    centre: (
                        fixe.0 - (milieu.0 - self.taille.0 / 2.0) / s,
                        fixe.1 - (milieu.1 - self.taille.1 / 2.0) / s,
                    ),
                    zoom,
                }
                .bornee();
                self.regle_vue(v);
            }
            Geste::Aucun => {}
        }
        None
    }

    fn haut(&mut self, e: PointerEvent, valide: bool) -> Option<Sortie> {
        let i = self
            .pointeurs
            .iter()
            .position(|(id, _, _)| *id == e.pointer_id())?;
        let (_, x, y) = self.pointeurs.remove(i);
        let t = navigateur::horloge_ms();

        if let (Geste::Glisse { depart, t0, .. }, true) = (&self.geste, valide) {
            let parcouru = ((x - depart.0).powi(2) + (y - depart.1).powi(2)).sqrt();
            if parcouru < 6.0 && t - t0 < 600.0 {
                self.touche(x, y, t);
            } else if self.mouvement {
                self.lance_inertie(t);
            }
        }
        // Il reste un doigt après un pincement : il reprend la main.
        if self.pointeurs.is_empty() {
            self.geste = Geste::Aucun;
        } else {
            self.commence_geste();
        }
        None
    }

    fn touche(&mut self, x: f64, y: f64, t: f64) {
        if let Some(prec) = self.toucher.take() {
            let proche = ((x - prec.x).powi(2) + (y - prec.y).powi(2)).sqrt() < 30.0;
            if proche && t - prec.t < 320.0 {
                // Double toucher : on zoome là, et le premier toucher — dont
                // le minuteur est lâché avec lui — ne pose rien.
                let cible = (self.vue.zoom + 1.0).round();
                self.zoome_vers(cible, (x, y));
                return;
            }
        }
        let faible = self.moi.clone();
        self.toucher = Some(Toucher {
            x,
            y,
            t,
            _minuteur: Timeout::new(260, move || {
                agit_faible(&faible, |i| i.toucher_confirme(x, y))
            }),
        });
    }

    /// Un toucher simple, confirmé : pas de second toucher dans les temps.
    fn toucher_confirme(&mut self, x: f64, y: f64) -> Option<Sortie> {
        self.toucher = None;
        // Une bulle ouverte se referme d'abord : le toucher qui la ferme ne
        // doit pas, en plus, poser un point sous elle.
        if untrack(|| self.ouvert.get()).is_some() {
            self.ouvert.set(None);
            return None;
        }
        Some(Sortie::Clic(self.coord_ecran((x, y))))
    }

    fn lance_inertie(&mut self, t: f64) {
        let (Some(premier), Some(dernier)) = (self.echantillons.front(), self.echantillons.back())
        else {
            return;
        };
        let dt = (dernier.0 - premier.0) / 1000.0;
        // Un doigt resté immobile avant de se lever ne lance rien.
        if dt <= 0.0 || t - dernier.0 > 80.0 {
            return;
        }
        let (vx, vy) = ((dernier.1 - premier.1) / dt, (dernier.2 - premier.2) / dt);
        if (vx * vx + vy * vy).sqrt() < 180.0 {
            return;
        }
        self.animation = Some(Animation::Inertie { vx, vy });
        self.demande_image();
    }

    fn molette(&mut self, e: WheelEvent) {
        e.prevent_default();
        let ecran = self.taille.1;
        let dy = calcul::delta_pixels(e.delta_y(), e.delta_mode(), ecran);
        // Ctrl + molette, c'est le pincement d'un pavé tactile : des deltas
        // fins et nombreux. Une vraie molette envoie des crans d'une
        // centaine de pixels — un demi-niveau chacun.
        let dz = if e.ctrl_key() {
            -dy * 0.012
        } else {
            -dy / 240.0
        };
        let ancre = self.position_locale(e.client_x() as f64, e.client_y() as f64);
        let base = match &self.animation {
            Some(Animation::Zoom { cible, .. }) => *cible,
            _ => self.vue.zoom,
        };
        self.zoome_vers(base + dz, ancre);
    }

    fn bouton(&mut self, d: f64) {
        let base = match &self.animation {
            Some(Animation::Zoom { cible, .. }) => *cible,
            _ => self.vue.zoom,
        };
        let centre = (self.taille.0 / 2.0, self.taille.1 / 2.0);
        self.zoome_vers((base + d).round(), centre);
    }

    fn recentre(&mut self) {
        if let Some(moi) = untrack(|| self.p.moi.get()) {
            let zoom = self.vue.zoom.max(14.0);
            self.vole(Vue::sur(moi, zoom));
        }
    }

    fn clavier(&mut self, e: KeyboardEvent) -> Option<Sortie> {
        let pas = 90.0;
        let centre = (self.taille.0 / 2.0, self.taille.1 / 2.0);
        let mut sortie = None;
        match e.key().as_str() {
            "ArrowLeft" => self.vue = self.vue.glisse(pas, 0.0),
            "ArrowRight" => self.vue = self.vue.glisse(-pas, 0.0),
            "ArrowUp" => self.vue = self.vue.glisse(0.0, pas),
            "ArrowDown" => self.vue = self.vue.glisse(0.0, -pas),
            "+" | "=" => self.zoome_vers((self.vue.zoom + 1.0).round(), centre),
            "-" | "_" => self.zoome_vers((self.vue.zoom - 1.0).round(), centre),
            "Enter" => {
                // Poser une position sans souris : au centre de la carte.
                let cible = e
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
                let sur_la_carte = cible.is_some() && cible == self.noeud;
                return sur_la_carte.then(|| Sortie::Clic(self.coord_ecran(centre)));
            }
            "Escape" => {
                self.ouvert.set(None);
                sortie = Some(Sortie::Echap);
            }
            _ => return None,
        }
        e.prevent_default();
        sortie
    }

    // --- Animations ----------------------------------------------------------

    fn zoome_vers(&mut self, cible: f64, ancre: (f64, f64)) {
        let cible = cible.clamp(projection::ZOOM_MIN, projection::ZOOM_MAX);
        if !self.mouvement {
            let v = self.vue.zoome_autour(cible, ancre, self.taille);
            self.regle_vue(v);
            return;
        }
        self.animation = Some(Animation::Zoom { cible, ancre });
        self.demande_image();
    }

    fn vole(&mut self, vers: Vue) {
        if !self.mouvement || self.taille.0 == 0.0 {
            self.regle_vue(vers);
            return;
        }
        // Plus le saut est grand, plus le vol dure — dans des bornes : un
        // recadrage de rue n'a pas à prendre une seconde, ni un saut de pays
        // à en prendre trois.
        let s = echelle(self.vue.zoom.min(vers.zoom));
        let px = ((vers.centre.0 - self.vue.centre.0).powi(2)
            + (vers.centre.1 - self.vue.centre.1).powi(2))
        .sqrt()
            * s;
        let duree_ms = (500.0 + px * 0.25 + (vers.zoom - self.vue.zoom).abs() * 90.0).min(1400.0);
        self.animation = Some(Animation::Vol {
            de: self.vue,
            vers,
            t0: None,
            duree_ms,
            courbe: calcul::expo_sortie,
        });
        self.demande_image();
    }

    /// L'arrivée : la carte part de plus haut et descend sur la France, le
    /// geste de l'app quand elle s'ouvre — une fois la carte dévoilée.
    ///
    /// Un cadrage en attente fait déjà voler la carte, vers les positions :
    /// plus utile qu'un vol pour le décor, il passe devant.
    fn prepare_arrivee(&mut self, el: &web_sys::Element) {
        let Some(guet) = el.closest("[data-mvt]").ok().flatten() else {
            return;
        };
        self.regle_vue(Vue::sur(DEPART, ZOOM_DEPART - ZOOM_ARRIVEE));
        self.animation = Some(Animation::Arrivee { guet, t0: None });
        self.demande_image();
    }

    fn applique_cadrage(&mut self) {
        if self.taille.0 == 0.0 {
            return;
        }
        let c = untrack(|| self.p.cadrage.get());
        self.cadrage_en_attente = false;
        self.cadrage_vu = c.numero;
        let marge = if self.taille.0 < 500.0 { 36.0 } else { 56.0 };
        if let Some(v) = cadre(&c.points, self.taille, marge, c.zoom_max) {
            self.vole(v);
        }
    }

    fn demande_image(&mut self) {
        if self.raf.is_some() || self.eteinte {
            return;
        }
        let Some(f) = fenetre() else {
            return;
        };
        if self.boucle.is_none() {
            let faible = self.moi.clone();
            self.boucle = Some(Closure::new(move |t: f64| {
                agit_faible(&faible, |i| {
                    i.image(t);
                    None
                })
            }));
        }
        if let Some(b) = &self.boucle {
            self.raf = f.request_animation_frame(b.as_ref().unchecked_ref()).ok();
        }
    }

    fn image(&mut self, t: f64) {
        self.raf = None;
        let dt = match self.image_prec.replace(t) {
            Some(p) => ((t - p) / 1000.0).clamp(0.0, 0.1),
            None => 1.0 / 60.0,
        };
        let Some(anim) = self.animation.take() else {
            self.image_prec = None;
            return;
        };
        let suite = match anim {
            Animation::Inertie { vx, vy } => {
                self.vue = self.vue.glisse(vx * dt, vy * dt);
                // Une décroissance exponentielle : un quart de seconde pour
                // perdre les deux tiers de l'élan.
                let k = (-dt * 4.0).exp();
                let (vx, vy) = (vx * k, vy * k);
                ((vx * vx + vy * vy).sqrt() > 20.0).then_some(Animation::Inertie { vx, vy })
            }
            Animation::Vol {
                de,
                vers,
                t0,
                duree_ms,
                courbe,
            } => {
                let t0 = t0.unwrap_or(t);
                let avance = ((t - t0) / duree_ms).clamp(0.0, 1.0);
                let v = de.vers(&vers, courbe(avance));
                self.regle_vue(v);
                (avance < 1.0).then_some(Animation::Vol {
                    de,
                    vers,
                    t0: Some(t0),
                    duree_ms,
                    courbe,
                })
            }
            Animation::Zoom { cible, ancre } => {
                let z = calcul::lerp(self.vue.zoom, cible, calcul::amorti(0.22, dt));
                let fini = (cible - z).abs() < 0.002;
                let z = if fini { cible } else { z };
                let v = self.vue.zoome_autour(z, ancre, self.taille);
                self.regle_vue(v);
                (!fini).then_some(Animation::Zoom { cible, ancre })
            }
            Animation::Arrivee { guet, t0 } => {
                let t0 = t0.unwrap_or(t);
                let vers = Vue::sur(DEPART, ZOOM_DEPART);
                if guet.has_attribute("data-vu") {
                    // Un temps, que le dévoilement ait commencé, puis une
                    // descente qui accélère et se pose : un vol qui part
                    // d'un coup se jouerait encore sous le masque.
                    Some(Animation::Vol {
                        de: self.vue,
                        vers,
                        t0: Some(t + DELAI_ARRIVEE_MS),
                        duree_ms: DUREE_ARRIVEE_MS,
                        courbe: en_douceur,
                    })
                } else if t - t0 > ATTENTE_ARRIVEE_MS {
                    self.regle_vue(vers);
                    None
                } else {
                    Some(Animation::Arrivee { guet, t0: Some(t0) })
                }
            }
        };
        self.animation = suite;
        if self.animation.is_some() {
            self.demande_image();
        } else {
            self.image_prec = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_arrivee_part_et_se_pose_sans_a_coup() {
        assert_eq!(en_douceur(0.0), 0.0);
        assert_eq!(en_douceur(0.5), 0.5);
        assert_eq!(en_douceur(1.0), 1.0);
        // Lente aux deux bouts, croissante entre les deux.
        assert!(en_douceur(0.1) < 0.01);
        assert!(en_douceur(0.9) > 0.99);
        let mut avant = 0.0;
        for i in 1..=100 {
            let v = en_douceur(i as f64 / 100.0);
            assert!(v >= avant);
            avant = v;
        }
    }

    fn proprietes(positions: Vec<Position>) -> Proprietes {
        Proprietes {
            positions: use_state(Rc::new(positions)),
            voitures: use_state(Rc::new(Vec::new())),
            alertes: use_state(Rc::new(Vec::new())),
            moi: use_state(None),
            destination: use_state(None),
            brouillon: use_state(None),
            bulle_brouillon: Rc::new(|| Node::Empty),
            cadrage: use_state(Cadrage::default()),
            on_clic: Rc::new(|_| {}),
            on_vote: Rc::new(|_, _| {}),
            on_echap: Rc::new(|| {}),
            ressources: Rc::new(Ressources::default()),
        }
    }

    /// Les repères sont des boutons, leurs libellés du texte : un nom de
    /// position qui ressemble à du HTML reste du texte.
    #[test]
    fn un_libelle_reste_du_texte() {
        let html = carte(proprietes(vec![Position {
            id: 1,
            lat: 44.8,
            lon: -0.57,
            label: "<b>Maison</b>".into(),
        }]))
        .render();
        assert!(
            html.contains("Position 1 : &lt;b&gt;Maison&lt;/b&gt;"),
            "{html}"
        );
        assert!(!html.contains("<b>Maison"));
        assert!(html.contains(r#"class="carte""#));
        assert!(html.contains("les contributeurs d&#x27;OpenStreetMap"));
    }

    #[test]
    fn deux_positions_tracent_un_itineraire() {
        let q = |id, lat| Position {
            id,
            lat,
            lon: 1.0,
            label: format!("P{id}"),
        };
        let html = carte(proprietes(vec![q(1, 45.0), q(2, 46.0)])).render();
        assert!(html.contains(r#"class="trace-halo""#), "{html}");
        assert!(html.contains(r#"class="trace" pathLength="1""#), "{html}");
        let seule = carte(proprietes(vec![q(1, 45.0)])).render();
        assert!(!seule.contains("trace-halo"));
    }
}
