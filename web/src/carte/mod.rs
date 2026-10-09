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
//! Les bulles sont maintenant des composants comme les autres : un libellé
//! y est du texte, jamais du balisage, et l'échappement cesse d'être une
//! chose à ne pas oublier.
//!
//! La géométrie vit dans [`projection`], testée sans navigateur. Ce module
//! ne fait que la brancher sur l'écran et sur les doigts.
//!
//! **Le plan.** Tout ce qui est posé sur la carte — tuiles, traces, repères
//! — vit dans un calque unique, déplacé d'un seul `transform` quand on fait
//! glisser la carte. Les repères gardent donc leur position dans le calque
//! pendant un glissement, et la transition CSS qui fait rouler les voitures
//! d'un relevé à l'autre ne se déclenche que lorsqu'elles bougent vraiment.

pub mod projection;

use std::collections::VecDeque;
use std::rc::Rc;

use gloo_timers::callback::Timeout;
use mouvement::calcul;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, KeyboardEvent, PointerEvent, ResizeObserver, WheelEvent};
use yew::prelude::*;

use crate::catalogue::{categorie, Ressources};
use crate::types::{Alerte, Coord, Lieu, Position, Voiture};
use projection::{cadre, deprojette, echelle, niveau, projette, tuiles_visibles, Vue};

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

#[derive(Properties, PartialEq)]
pub struct Props {
    pub positions: Rc<Vec<Position>>,
    pub voitures: Rc<Vec<Voiture>>,
    pub alertes: Rc<Vec<Alerte>>,
    #[prop_or_default]
    pub moi: Option<Coord>,
    #[prop_or_default]
    pub destination: Option<Lieu>,
    /// Le point qu'on est en train de nommer, et sa bulle.
    #[prop_or_default]
    pub brouillon: Option<Coord>,
    #[prop_or_default]
    pub bulle_brouillon: Html,
    pub cadrage: Cadrage,
    pub on_clic: Callback<Coord>,
    pub on_vote: Callback<(i64, bool)>,
    #[prop_or_default]
    pub on_echap: Callback<()>,
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
    /// Un déplacement vers une vue, avec la courbe de la maison.
    Vol {
        de: Vue,
        vers: Vue,
        t0: Option<f64>,
        duree_ms: f64,
    },
    /// Le zoom à la molette ou aux boutons, lissé vers sa cible.
    Zoom { cible: f64, ancre: (f64, f64) },
}

/// Un toucher en attente : est-ce le premier d'un double toucher ?
struct Toucher {
    x: f64,
    y: f64,
    t: f64,
    _minuteur: Timeout,
}

pub enum Msg {
    Bas(PointerEvent),
    Bouge(PointerEvent),
    Haut(PointerEvent),
    Perdu(PointerEvent),
    Molette(WheelEvent),
    Clavier(KeyboardEvent),
    Bouton(f64),
    Recentre,
    Image(f64),
    Taille,
    Ouvre(Option<Objet>),
    Vote(i64, bool),
    FinZoom,
    FinNiveau,
    Toucher(f64, f64),
}

pub struct Carte {
    noeud: NodeRef,
    vue: Vue,
    taille: (f64, f64),
    /// L'origine du plan, en Mercator normalisé. Elle ne bouge qu'au zoom :
    /// tant qu'elle reste fixe, glisser la carte ne déplace que le plan.
    origine: (f64, f64),
    pointeurs: Vec<(i32, f64, f64)>,
    geste: Geste,
    echantillons: VecDeque<(f64, f64, f64)>,
    animation: Option<Animation>,
    raf: Option<i32>,
    boucle: Closure<dyn FnMut(f64)>,
    image_prec: Option<f64>,
    ouvert: Option<Objet>,
    /// Vrai pendant un zoom : les transitions des repères sont coupées, sans
    /// quoi chaque repère glisserait vers sa nouvelle place au lieu d'y être.
    zoome: bool,
    fin_zoom: Option<Timeout>,
    /// Le niveau de tuiles précédent, gardé un instant sous le nouveau pour
    /// qu'un changement de niveau ne montre jamais de trou.
    niveau_prec: Option<u32>,
    fin_niveau: Option<Timeout>,
    toucher: Option<Toucher>,
    observateur: Option<(ResizeObserver, Closure<dyn FnMut()>)>,
    cadrage_vu: u32,
    cadrage_en_attente: bool,
    mouvement: bool,
    ressources: Rc<Ressources>,
    /// Une bulle vient de s'ouvrir : au rendu suivant, on vérifie qu'elle
    /// tient dans la carte, et l'on décale la carte sinon — ce que faisait
    /// l'`autoPan` de Leaflet.
    verifie_bulle: bool,
}

/// La France entière, comme l'ancienne carte au démarrage.
const DEPART: Coord = Coord {
    lat: 46.6,
    lon: 2.5,
};
const ZOOM_DEPART: f64 = 5.0;

fn maintenant() -> f64 {
    web_sys::window()
        .and_then(|f| f.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

impl Component for Carte {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let lien = ctx.link().clone();
        let boucle = Closure::new(move |t: f64| lien.send_message(Msg::Image(t)));
        let vue = Vue::sur(DEPART, ZOOM_DEPART);
        Carte {
            noeud: NodeRef::default(),
            vue,
            taille: (0.0, 0.0),
            origine: vue.centre,
            pointeurs: Vec::new(),
            geste: Geste::Aucun,
            echantillons: VecDeque::new(),
            animation: None,
            raf: None,
            boucle,
            image_prec: None,
            ouvert: None,
            zoome: false,
            fin_zoom: None,
            niveau_prec: None,
            fin_niveau: None,
            toucher: None,
            observateur: None,
            // Un premier cadrage demandé avant que la carte ait une taille
            // attend qu'elle en ait une.
            cadrage_vu: 0,
            cadrage_en_attente: ctx.props().cadrage.numero != 0,
            mouvement: mouvement::mouvement_permis(),
            // Les adresses des avatars ne changent pas pendant la vie de la
            // page : lues une fois, sans abonnement.
            ressources: ctx
                .link()
                .context::<Rc<Ressources>>(Callback::noop())
                .map(|(r, _)| r)
                .unwrap_or_default(),
            verifie_bulle: false,
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, ancien: &Props) -> bool {
        let p = ctx.props();
        if p.cadrage.numero != ancien.cadrage.numero && p.cadrage.numero != self.cadrage_vu {
            self.cadrage_en_attente = true;
            self.applique_cadrage(ctx);
        }
        if p.brouillon.is_some() && p.brouillon != ancien.brouillon {
            self.verifie_bulle = true;
        }
        true
    }

    fn rendered(&mut self, ctx: &Context<Self>, premier: bool) {
        if self.verifie_bulle {
            self.verifie_bulle = false;
            self.garde_la_bulle_visible(ctx);
        }
        if !premier {
            return;
        }
        let Some(el) = self.noeud.cast::<Element>() else {
            return;
        };
        let lien = ctx.link().clone();
        let rappel = Closure::<dyn FnMut()>::new(move || lien.send_message(Msg::Taille));
        if let Ok(ro) = ResizeObserver::new(rappel.as_ref().unchecked_ref()) {
            ro.observe(&el);
            self.observateur = Some((ro, rappel));
        }
        ctx.link().send_message(Msg::Taille);
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        if let Some((ro, _)) = self.observateur.take() {
            ro.disconnect();
        }
        if let (Some(id), Some(f)) = (self.raf.take(), web_sys::window()) {
            let _ = f.cancel_animation_frame(id);
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Msg) -> bool {
        match msg {
            Msg::Taille => {
                let Some(el) = self.noeud.cast::<Element>() else {
                    return false;
                };
                let taille = (el.client_width() as f64, el.client_height() as f64);
                if taille == self.taille {
                    return false;
                }
                self.taille = taille;
                if self.cadrage_en_attente {
                    self.applique_cadrage(ctx);
                }
                true
            }
            Msg::Bas(e) => self.bas(e),
            Msg::Bouge(e) => self.bouge(ctx, e),
            Msg::Haut(e) => self.haut(ctx, e, true),
            Msg::Perdu(e) => self.haut(ctx, e, false),
            Msg::Molette(e) => {
                e.prevent_default();
                let ecran = self.taille.1;
                let dy = calcul::delta_pixels(e.delta_y(), e.delta_mode(), ecran);
                // Ctrl + molette, c'est le pincement d'un pavé tactile :
                // des deltas fins et nombreux. Une vraie molette envoie des
                // crans d'une centaine de pixels — un demi-niveau chacun.
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
                self.zoome_vers(ctx, base + dz, ancre);
                false
            }
            Msg::Clavier(e) => self.clavier(ctx, e),
            Msg::Bouton(d) => {
                let base = match &self.animation {
                    Some(Animation::Zoom { cible, .. }) => *cible,
                    _ => self.vue.zoom,
                };
                let centre = (self.taille.0 / 2.0, self.taille.1 / 2.0);
                self.zoome_vers(ctx, (base + d).round(), centre);
                false
            }
            Msg::Recentre => {
                if let Some(moi) = ctx.props().moi {
                    let zoom = self.vue.zoom.max(14.0);
                    self.vole(ctx, Vue::sur(moi, zoom));
                }
                false
            }
            Msg::Image(t) => self.image(ctx, t),
            Msg::Ouvre(o) => {
                self.verifie_bulle = o.is_some();
                self.ouvert = o;
                true
            }
            Msg::Vote(id, oui) => {
                ctx.props().on_vote.emit((id, oui));
                false
            }
            Msg::FinZoom => {
                self.zoome = false;
                self.fin_zoom = None;
                true
            }
            Msg::FinNiveau => {
                self.niveau_prec = None;
                self.fin_niveau = None;
                true
            }
            Msg::Toucher(x, y) => {
                self.toucher = None;
                // Une bulle ouverte se referme d'abord : le toucher qui la
                // ferme ne doit pas, en plus, poser un point sous elle.
                if self.ouvert.take().is_some() {
                    return true;
                }
                let coord = self.coord_ecran((x, y));
                ctx.props().on_clic.emit(coord);
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let p = ctx.props();
        let lien = ctx.link();
        let (tx, ty) = self.translation_plan();
        let classes = classes!("carte", self.zoome.then_some("zoome"));

        html! {
            <div class={classes} ref={self.noeud.clone()}
                 tabindex="0" role="application"
                 aria-label="Carte. Flèches pour la déplacer, plus et moins pour zoomer, Entrée pour poser une position au centre."
                 data-defile-libre="" data-mvt-calme=""
                 onpointerdown={lien.callback(Msg::Bas)}
                 onpointermove={lien.callback(Msg::Bouge)}
                 onpointerup={lien.callback(Msg::Haut)}
                 onpointercancel={lien.callback(Msg::Perdu)}
                 onwheel={lien.callback(Msg::Molette)}
                 onkeydown={lien.callback(Msg::Clavier)}>
                <div class="carte-plan"
                     style={format!("transform:translate3d({tx}px,{ty}px,0)")}>
                    <div class="tuiles tuiles-avant">
                        { self.niveau_prec
                            .filter(|z| z.abs_diff(niveau(self.vue.zoom)) <= 2)
                            .map(|z| self.tuiles(z))
                            .unwrap_or_default() }
                    </div>
                    <div class="tuiles">
                        { self.tuiles(niveau(self.vue.zoom)) }
                    </div>
                    { self.traces(p) }
                    { self.reperes(ctx) }
                    { self.bulle(ctx) }
                    { self.brouillon(p) }
                </div>

                <div class="carte-commandes" data-carte-ui="">
                    <button type="button" aria-label="Zoomer"
                            onclick={lien.callback(|_| Msg::Bouton(1.0))}>{ "+" }</button>
                    <button type="button" aria-label="Dézoomer"
                            onclick={lien.callback(|_| Msg::Bouton(-1.0))}>{ "−" }</button>
                    if p.moi.is_some() {
                        <button type="button" class="recentrer"
                                aria-label="Recentrer sur ma position"
                                onclick={lien.callback(|_| Msg::Recentre)}>
                            <span>{ "➤" }</span>
                        </button>
                    }
                </div>
                // L'attribution que demande la licence ODbL, avec son lien.
                <p class="carte-credit" data-carte-ui="">
                    { "© " }
                    <a href="https://www.openstreetmap.org/copyright"
                       target="_blank" rel="noopener">{ "les contributeurs d'OpenStreetMap" }</a>
                </p>
            </div>
        }
    }
}

impl Carte {
    // --- Repères et géométrie ----------------------------------------------

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

    fn position_locale(&self, client_x: f64, client_y: f64) -> (f64, f64) {
        match self.noeud.cast::<Element>() {
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
    fn regle_vue(&mut self, ctx: &Context<Self>, v: Vue) {
        let ancien = niveau(self.vue.zoom);
        if niveau(v.zoom) != ancien {
            self.niveau_prec = Some(ancien);
            let lien = ctx.link().clone();
            self.fin_niveau = Some(Timeout::new(700, move || lien.send_message(Msg::FinNiveau)));
        }
        let s = echelle(v.zoom);
        let loin = ((v.centre.0 - self.origine.0) * s).abs() > 1e6
            || ((v.centre.1 - self.origine.1) * s).abs() > 1e6;
        if v.zoom != self.vue.zoom || loin {
            self.origine = v.centre;
            self.zoome = true;
            let lien = ctx.link().clone();
            self.fin_zoom = Some(Timeout::new(220, move || lien.send_message(Msg::FinZoom)));
        }
        self.vue = v;
    }

    fn tuiles(&self, z: u32) -> Html {
        let s = echelle(self.vue.zoom);
        let n = (1u64 << z) as f64;
        let liste = tuiles_visibles(&self.vue, self.taille, z);
        // Les bords sont arrondis à partir des positions, et non la taille
        // à part : deux tuiles voisines partagent exactement le même bord,
        // et aucun liseré clair ne passe entre elles aux zooms fractionnaires.
        liste
            .into_iter()
            .map(|t| {
                let (cx, cy) = t.coin();
                let x0 = ((cx - self.origine.0) * s).round();
                let y0 = ((cy - self.origine.1) * s).round();
                let x1 = ((cx + 1.0 / n - self.origine.0) * s).round();
                let y1 = ((cy + 1.0 / n - self.origine.1) * s).round();
                html! {
                    <img key={t.cle()} class="tuile" src={t.url()} alt="" draggable="false"
                         decoding="async"
                         style={format!(
                             "transform:translate3d({x0}px,{y0}px,0);width:{}px;height:{}px",
                             x1 - x0, y1 - y0)}
                         onload={Callback::from(|e: Event| {
                             if let Some(img) = e.target_dyn_into::<Element>() {
                                 let _ = img.set_attribute("data-chargee", "");
                             }
                         })} />
                }
            })
            .collect()
    }

    fn traces(&self, p: &Props) -> Html {
        let chemin = |points: &mut dyn Iterator<Item = Coord>| -> String {
            let mut d = String::new();
            for (i, c) in points.enumerate() {
                let (x, y) = self.dans_plan(c);
                d.push_str(&format!("{}{x:.1} {y:.1}", if i == 0 { "M" } else { "L" }));
            }
            d
        };
        let itineraire = (p.positions.len() >= 2).then(|| {
            let d = chemin(&mut p.positions.iter().map(Position::coord));
            // La signature des points : un nouvel itinéraire remonte le
            // tracé, qui se dessine à nouveau ; un simple zoom le garde.
            let signature = p
                .positions
                .iter()
                .map(|q| format!("{:.5},{:.5}", q.lat, q.lon))
                .collect::<Vec<_>>()
                .join(";");
            (d, signature)
        });
        let vers_destination = match (p.moi, &p.destination) {
            (Some(moi), Some(dest)) => Some(chemin(&mut [moi, dest.coord].into_iter())),
            _ => None,
        };
        html! {
            <svg class="traces" width="1" height="1" aria-hidden="true" focusable="false">
                if let Some((d, _)) = &itineraire {
                    <path class="trace-halo" d={d.clone()} />
                }
                { for itineraire.iter().map(|(d, signature)| html! {
                    <path key={signature.clone()} class="trace" pathLength="1" d={d.clone()} />
                }) }
                if let Some(d) = vers_destination {
                    <path class="trace-destination" d={d} />
                }
            </svg>
        }
    }

    fn reperes(&self, ctx: &Context<Self>) -> Html {
        let p = ctx.props();
        let lien = ctx.link();
        let res = &self.ressources;
        let place = |c: Coord| {
            let (x, y) = self.dans_plan(c);
            format!("transform:translate3d({x:.1}px,{y:.1}px,0)")
        };

        let positions = p.positions.iter().enumerate().map(|(i, q)| {
            let id = q.id;
            html! {
                <button key={format!("p{id}")} type="button" class="repere" data-carte-ui=""
                        style={place(q.coord())}
                        aria-label={format!("Position {} : {}", i + 1, q.label)}
                        onclick={lien.callback(move |_| Msg::Ouvre(Some(Objet::Position(id))))}>
                    <span class="goutte"><span>{ i + 1 }</span></span>
                </button>
            }
        });

        let alertes = p.alertes.iter().map(|a| {
            let id = a.id;
            let c = categorie(&a.category);
            html! {
                <button key={format!("a{id}")} type="button" class="signal" data-carte-ui=""
                        style={place(a.coord())}
                        aria-label={format!("Signalement : {}", c.nom)}
                        onclick={lien.callback(move |_| Msg::Ouvre(Some(Objet::Alerte(id))))}>
                    <span class="signal-pastille">{ c.emoji }</span>
                </button>
            }
        });

        let voitures = p.voitures.iter().map(|v| {
            let id = v.id;
            html! {
                <button key={format!("v{id}")} type="button" class="voiture" data-carte-ui=""
                        style={place(v.coord())}
                        aria-label={format!("En direct : {}", v.label)}
                        onclick={lien.callback(move |_| Msg::Ouvre(Some(Objet::Voiture(id))))}>
                    <img src={res.avatar(&v.avatar)} alt="" draggable="false" />
                </button>
            }
        });

        // Le point bleu passe dessous : un signalement posé là où l'on se
        // trouve doit rester lisible, l'onde du point dépasse de toute façon.
        html! {
            <>
                if let Some(moi) = p.moi {
                    <span class="moi" style={place(moi)} aria-hidden="true"><span /></span>
                }
                { for positions }
                { for alertes }
                if let Some(d) = &p.destination {
                    <button type="button" class="drapeau" data-carte-ui=""
                            style={place(d.coord)}
                            aria-label={format!("Destination : {}", d.nom_court())}
                            onclick={lien.callback(|_| Msg::Ouvre(Some(Objet::Destination)))}>
                        <span>{ "🎯" }</span>
                    </button>
                }
                { for voitures }
            </>
        }
    }

    fn bulle(&self, ctx: &Context<Self>) -> Html {
        let p = ctx.props();
        let lien = ctx.link();
        let Some(objet) = self.ouvert else {
            return Html::default();
        };
        let (coord, contenu) = match objet {
            Objet::Position(id) => {
                let Some((i, q)) = p.positions.iter().enumerate().find(|(_, q)| q.id == id) else {
                    return Html::default();
                };
                (
                    q.coord(),
                    html! {
                        <>
                            <p class="bulle-titre">{ format!("{}. {}", i + 1, q.label) }</p>
                            <p class="bulle-detail">{ format!("{:.4}, {:.4}", q.lat, q.lon) }</p>
                        </>
                    },
                )
            }
            Objet::Voiture(id) => {
                let Some(v) = p.voitures.iter().find(|v| v.id == id) else {
                    return Html::default();
                };
                (v.coord(), html! { <p class="bulle-titre">{ &v.label }</p> })
            }
            Objet::Destination => {
                let Some(d) = &p.destination else {
                    return Html::default();
                };
                (
                    d.coord,
                    html! { <p class="bulle-titre">{ d.nom_court() }</p> },
                )
            }
            Objet::Alerte(id) => {
                let Some(a) = p.alertes.iter().find(|a| a.id == id) else {
                    return Html::default();
                };
                let c = categorie(&a.category);
                let titre = if a.label.is_empty() {
                    c.nom
                } else {
                    a.label.as_str()
                };
                (
                    a.coord(),
                    html! {
                        <>
                            <p class="bulle-titre">{ format!("{} {titre}", c.emoji) }</p>
                            <p class="bulle-detail">
                                { format!("👍 {} · 👎 {}", a.confirms, a.denies) }
                            </p>
                            <div class="bulle-votes">
                                <button type="button" class="petit"
                                        onclick={lien.callback(move |_| Msg::Vote(id, true))}>
                                    { "👍 Toujours là" }
                                </button>
                                <button type="button" class="petit secondaire"
                                        onclick={lien.callback(move |_| Msg::Vote(id, false))}>
                                    { "👎 Plus là" }
                                </button>
                            </div>
                        </>
                    },
                )
            }
        };
        let (x, y) = self.dans_plan(coord);
        html! {
            <div class="bulle" data-carte-ui=""
                 style={format!("transform:translate3d({x:.1}px,{y:.1}px,0)")}>
                <div class="bulle-corps" role="dialog">
                    <button type="button" class="bulle-fermer" aria-label="Fermer"
                            onclick={lien.callback(|_| Msg::Ouvre(None))}>{ "✕" }</button>
                    { contenu }
                </div>
            </div>
        }
    }

    fn brouillon(&self, p: &Props) -> Html {
        let Some(c) = p.brouillon else {
            return Html::default();
        };
        let (x, y) = self.dans_plan(c);
        let place = format!("transform:translate3d({x:.1}px,{y:.1}px,0)");
        html! {
            <>
                <span class="repere brouillon" style={place.clone()} aria-hidden="true">
                    <span class="goutte"><span>{ "+" }</span></span>
                </span>
                <div class="bulle" data-carte-ui="" style={place}>
                    <div class="bulle-corps">{ p.bulle_brouillon.clone() }</div>
                </div>
            </>
        }
    }

    /// Décale la carte pour qu'une bulle ouverte y tienne en entier.
    fn garde_la_bulle_visible(&mut self, ctx: &Context<Self>) {
        let Some(carte) = self.noeud.cast::<Element>() else {
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
            self.vole(ctx, vers);
        }
    }

    // --- Gestes ---------------------------------------------------------------

    fn bas(&mut self, e: PointerEvent) -> bool {
        // Ce qui est posé sur la carte — repères, bulles, boutons — reçoit
        // ses propres clics : le geste de carte ne démarre pas dessus.
        let sur_ui = e
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|el| el.closest("[data-carte-ui]").ok().flatten())
            .is_some();
        if sur_ui || (e.pointer_type() == "mouse" && e.button() != 0) {
            return false;
        }
        if let Some(el) = self.noeud.cast::<HtmlElement>() {
            let _ = el.set_pointer_capture(e.pointer_id());
            let _ = el.focus();
        }
        let (x, y) = self.position_locale(e.client_x() as f64, e.client_y() as f64);
        self.pointeurs.retain(|(id, _, _)| *id != e.pointer_id());
        self.pointeurs.push((e.pointer_id(), x, y));
        self.animation = None;
        self.commence_geste();
        false
    }

    fn commence_geste(&mut self) {
        let t = maintenant();
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

    fn bouge(&mut self, ctx: &Context<Self>, e: PointerEvent) -> bool {
        let Some(i) = self
            .pointeurs
            .iter()
            .position(|(id, _, _)| *id == e.pointer_id())
        else {
            return false;
        };
        let (x, y) = self.position_locale(e.client_x() as f64, e.client_y() as f64);
        self.pointeurs[i] = (e.pointer_id(), x, y);
        match &mut self.geste {
            Geste::Glisse { x: gx, y: gy, .. } => {
                let (dx, dy) = (x - *gx, y - *gy);
                *gx = x;
                *gy = y;
                let t = maintenant();
                self.echantillons.push_back((t, x, y));
                while self
                    .echantillons
                    .front()
                    .is_some_and(|(t0, _, _)| t - t0 > 120.0)
                {
                    self.echantillons.pop_front();
                }
                self.vue = self.vue.glisse(dx, dy);
                true
            }
            Geste::Pince {
                ecart0,
                zoom0,
                fixe,
            } => {
                let [(_, x1, y1), (_, x2, y2), ..] = self.pointeurs.as_slice() else {
                    return false;
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
                self.regle_vue(ctx, v);
                true
            }
            Geste::Aucun => false,
        }
    }

    fn haut(&mut self, ctx: &Context<Self>, e: PointerEvent, valide: bool) -> bool {
        let Some(i) = self
            .pointeurs
            .iter()
            .position(|(id, _, _)| *id == e.pointer_id())
        else {
            return false;
        };
        let (_, x, y) = self.pointeurs.remove(i);
        let t = maintenant();

        match &self.geste {
            Geste::Glisse { depart, t0, .. } if valide => {
                let parcouru = ((x - depart.0).powi(2) + (y - depart.1).powi(2)).sqrt();
                if parcouru < 6.0 && t - t0 < 600.0 {
                    self.touche(ctx, x, y, t);
                } else if self.mouvement {
                    self.lance_inertie(t);
                }
            }
            _ => {}
        }
        // Il reste un doigt après un pincement : il reprend la main.
        if self.pointeurs.is_empty() {
            self.geste = Geste::Aucun;
        } else {
            self.commence_geste();
        }
        false
    }

    fn touche(&mut self, ctx: &Context<Self>, x: f64, y: f64, t: f64) {
        if let Some(prec) = self.toucher.take() {
            let proche = ((x - prec.x).powi(2) + (y - prec.y).powi(2)).sqrt() < 30.0;
            if proche && t - prec.t < 320.0 {
                // Double toucher : on zoome là, et le premier toucher — dont
                // le minuteur est lâché avec lui — ne pose rien.
                let cible = (self.vue.zoom + 1.0).round();
                self.zoome_vers(ctx, cible, (x, y));
                return;
            }
        }
        let lien = ctx.link().clone();
        self.toucher = Some(Toucher {
            x,
            y,
            t,
            _minuteur: Timeout::new(260, move || lien.send_message(Msg::Toucher(x, y))),
        });
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

    fn clavier(&mut self, ctx: &Context<Self>, e: KeyboardEvent) -> bool {
        let pas = 90.0;
        let centre = (self.taille.0 / 2.0, self.taille.1 / 2.0);
        match e.key().as_str() {
            "ArrowLeft" => self.vue = self.vue.glisse(pas, 0.0),
            "ArrowRight" => self.vue = self.vue.glisse(-pas, 0.0),
            "ArrowUp" => self.vue = self.vue.glisse(0.0, pas),
            "ArrowDown" => self.vue = self.vue.glisse(0.0, -pas),
            "+" | "=" => {
                self.zoome_vers(ctx, (self.vue.zoom + 1.0).round(), centre);
            }
            "-" | "_" => {
                self.zoome_vers(ctx, (self.vue.zoom - 1.0).round(), centre);
            }
            "Enter" => {
                // Poser une position sans souris : au centre de la carte.
                let sur_la_carte = e.target().and_then(|t| t.dyn_into::<Element>().ok())
                    == self.noeud.cast::<Element>();
                if sur_la_carte {
                    ctx.props().on_clic.emit(self.coord_ecran(centre));
                }
                return false;
            }
            "Escape" => {
                self.ouvert = None;
                ctx.props().on_echap.emit(());
            }
            _ => return false,
        }
        e.prevent_default();
        true
    }

    // --- Animations ----------------------------------------------------------

    fn zoome_vers(&mut self, ctx: &Context<Self>, cible: f64, ancre: (f64, f64)) {
        let cible = cible.clamp(projection::ZOOM_MIN, projection::ZOOM_MAX);
        if !self.mouvement {
            let v = self.vue.zoome_autour(cible, ancre, self.taille);
            self.regle_vue(ctx, v);
            self.redessine(ctx);
            return;
        }
        self.animation = Some(Animation::Zoom { cible, ancre });
        self.demande_image();
    }

    fn vole(&mut self, ctx: &Context<Self>, vers: Vue) {
        if !self.mouvement || self.taille.0 == 0.0 {
            self.regle_vue(ctx, vers);
            self.redessine(ctx);
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
        });
        self.demande_image();
    }

    fn applique_cadrage(&mut self, ctx: &Context<Self>) {
        if self.taille.0 == 0.0 {
            return;
        }
        let c = &ctx.props().cadrage;
        self.cadrage_en_attente = false;
        self.cadrage_vu = c.numero;
        let marge = if self.taille.0 < 500.0 { 36.0 } else { 56.0 };
        if let Some(v) = cadre(&c.points, self.taille, marge, c.zoom_max) {
            self.vole(ctx, v);
        }
    }

    fn redessine(&self, ctx: &Context<Self>) {
        ctx.link().send_message(Msg::Image(-1.0));
    }

    fn demande_image(&mut self) {
        if self.raf.is_some() {
            return;
        }
        if let Some(f) = web_sys::window() {
            self.raf = f
                .request_animation_frame(self.boucle.as_ref().unchecked_ref())
                .ok();
        }
    }

    fn image(&mut self, ctx: &Context<Self>, t: f64) -> bool {
        // `t` négatif : un simple rafraîchissement demandé hors boucle.
        if t < 0.0 {
            return true;
        }
        self.raf = None;
        let dt = match self.image_prec.replace(t) {
            Some(p) => ((t - p) / 1000.0).clamp(0.0, 0.1),
            None => 1.0 / 60.0,
        };
        let Some(anim) = self.animation.take() else {
            self.image_prec = None;
            return false;
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
            } => {
                let t0 = t0.unwrap_or(t);
                let avance = ((t - t0) / duree_ms).clamp(0.0, 1.0);
                let v = de.vers(&vers, calcul::expo_sortie(avance));
                self.regle_vue(ctx, v);
                (avance < 1.0).then_some(Animation::Vol {
                    de,
                    vers,
                    t0: Some(t0),
                    duree_ms,
                })
            }
            Animation::Zoom { cible, ancre } => {
                let z = calcul::lerp(self.vue.zoom, cible, calcul::amorti(0.22, dt));
                let fini = (cible - z).abs() < 0.002;
                let z = if fini { cible } else { z };
                let v = self.vue.zoome_autour(z, ancre, self.taille);
                self.regle_vue(ctx, v);
                (!fini).then_some(Animation::Zoom { cible, ancre })
            }
        };
        self.animation = suite;
        if self.animation.is_some() {
            self.demande_image();
        } else {
            self.image_prec = None;
        }
        true
    }
}
