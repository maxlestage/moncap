//! Le moteur : ce que GSAP, ScrollTrigger et Lenis font sur zamocorp.com,
//! branché directement sur le DOM.
//!
//! Il ne connaît pas Yew. Il lit des attributs posés par le rendu et
//! n'écrit que des attributs et des propriétés CSS que le rendu, lui, ne
//! déclare pas — `data-vu`, `--retard`, `--p`, `--ax` —, si bien qu'un
//! nouveau rendu ne les efface jamais.
//!
//! | attribut           | effet                                                    |
//! |--------------------|----------------------------------------------------------|
//! | `data-mvt="…"`     | révélé à l'entrée dans l'écran, en cascade avec ses voisins |
//! | `data-compte`      | son nombre compte depuis zéro quand lui, ou le bloc qui le contient, est révélé |
//! | `data-scrub`       | reçoit `--p`, sa traversée de l'écran (0 → 1)             |
//! | `data-defile`      | reçoit `--defile`, `--defile-ecrans` et `--elan`         |
//! | `data-aimant`      | suit le pointeur, à la souris seulement                  |
//! | `data-defile-libre`| la molette y garde son comportement natif                |
//! | `data-mvt-calme`   | ses changements de contenu ne relancent pas d'inventaire  |
//!
//! **Le contenu reste visible sans le moteur.** L'état masqué n'existe que
//! sous `html.mvt`, classe que le moteur pose en démarrant et retire en
//! s'arrêtant. Mouvement refusé par le système, WebAssembly en panne,
//! moteur jamais lancé : la page est immobile, jamais vide.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::Array;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    AddEventListenerOptions, CssStyleDeclaration, Document, Element, Event, EventTarget,
    HtmlElement, IntersectionObserver, IntersectionObserverEntry, IntersectionObserverInit,
    MutationObserver, MutationObserverInit, MutationRecord, Node, PointerEvent, ScrollBehavior,
    ScrollToOptions, SvgElement, WheelEvent, Window,
};

use crate::calcul::{self, Chiffre};

/// Les réglages du moteur.
#[derive(Clone, Copy, Debug)]
pub struct Reglages {
    /// Lisse la molette à la manière de Lenis. Le toucher n'est jamais
    /// intercepté : le système le rend déjà bien, et Lenis le laisse aussi.
    pub molette_douce: bool,
    /// L'amortissement par image à 60 Hz — 0,1 chez Lenis.
    pub lerp: f64,
    /// Le pas d'une cascade, en secondes.
    pub pas_cascade: f64,
    /// Le retard de la toute première cascade : le temps que le rideau de
    /// chargement se lève.
    pub retard_initial: f64,
}

impl Default for Reglages {
    fn default() -> Self {
        Self {
            molette_douce: true,
            lerp: 0.1,
            pas_cascade: 0.08,
            retard_initial: 0.25,
        }
    }
}

/// Vrai si le système accepte le mouvement.
///
/// `prefers-reduced-motion: reduce` est un réglage d'accessibilité, pas un
/// goût : il protège notamment les personnes que le mouvement rend malades.
/// Le moteur ne démarre alors pas du tout.
pub fn mouvement_permis() -> bool {
    media("(prefers-reduced-motion: reduce)")
        .map(|r| !r)
        .unwrap_or(true)
}

fn media(requete: &str) -> Option<bool> {
    web_sys::window()?
        .match_media(requete)
        .ok()
        .flatten()
        .map(|m| m.matches())
}

/// Lève le rideau de chargement.
///
/// Le rideau est écrit dans la coquille HTML, avant le WebAssembly : c'est
/// lui qu'on voit pendant le téléchargement, à la place d'une page blanche.
/// Une fois l'application montée, il glisse vers le haut et disparaît.
pub fn leve_rideau() {
    let Some(d) = web_sys::window().and_then(|f| f.document()) else {
        return;
    };
    let Some(rideau) = d.get_element_by_id("rideau") else {
        return;
    };
    if !mouvement_permis() {
        rideau.remove();
        return;
    }
    let _ = rideau.class_list().add_1("leve");
    // Retiré après sa sortie : un calque plein écran, même transparent, n'a
    // rien à faire dans la page une fois sa course finie.
    let a_retirer = rideau.clone();
    let fin = Closure::once_into_js(move || a_retirer.remove());
    if let Some(f) = web_sys::window() {
        let _ = f.set_timeout_with_callback_and_timeout_and_arguments_0(fin.unchecked_ref(), 1400);
    }
}

/// Le moteur en marche. Le lâcher l'arrête et rend la page immobile.
pub struct Moteur {
    interne: Rc<Interne>,
}

/// Un nombre en train de compter.
struct Compteur {
    noeud: Node,
    chiffre: Chiffre,
    final_: String,
    debut_ms: f64,
}

type RappelIo = Closure<dyn FnMut(Array, IntersectionObserver)>;
type RappelMo = Closure<dyn FnMut(Array, MutationObserver)>;
type RappelImage = Closure<dyn FnMut(f64)>;

/// Un écouteur posé, gardé pour être retiré.
struct Ecouteur {
    cible: EventTarget,
    evenement: &'static str,
    rappel: Closure<dyn FnMut(Event)>,
}

struct Interne {
    fenetre: Window,
    document: Document,
    reglages: Reglages,

    // Le défilement inertiel.
    actuel: Cell<f64>,
    cible: Cell<f64>,
    glisse: Cell<bool>,
    dernier_pose: Cell<f64>,

    // L'élan, pour les effets qui réagissent à la vitesse.
    dernier_y: Cell<f64>,
    elan: Cell<f64>,

    // La boucle d'images.
    raf: Cell<Option<i32>>,
    image_prec: Cell<Option<f64>>,
    a_peindre: Cell<bool>,
    a_inventorier: Cell<bool>,
    premiere_cascade: Cell<bool>,

    defilants: RefCell<Vec<CssStyleDeclaration>>,
    scrubs: RefCell<Vec<(Element, CssStyleDeclaration)>>,
    compteurs: RefCell<Vec<Compteur>>,
    aimant_courant: RefCell<Option<HtmlElement>>,

    io: RefCell<Option<IntersectionObserver>>,
    mo: RefCell<Option<MutationObserver>>,
    rappel_io: RefCell<Option<RappelIo>>,
    rappel_mo: RefCell<Option<RappelMo>>,
    boucle: RefCell<Option<RappelImage>>,
    ecouteurs: RefCell<Vec<Ecouteur>>,
}

impl Moteur {
    /// Démarre le moteur sur le document courant.
    ///
    /// Rend `None` quand le mouvement est refusé : rien n'est alors posé, ni
    /// classe, ni écouteur, et la page reste telle que le rendu l'a écrite.
    pub fn demarre(reglages: Reglages) -> Option<Moteur> {
        if !mouvement_permis() {
            return None;
        }
        let fenetre = web_sys::window()?;
        let document = fenetre.document()?;
        let y = fenetre.scroll_y().unwrap_or(0.0);

        let interne = Rc::new(Interne {
            fenetre,
            document,
            reglages,
            actuel: Cell::new(y),
            cible: Cell::new(y),
            glisse: Cell::new(false),
            dernier_pose: Cell::new(y),
            dernier_y: Cell::new(y),
            elan: Cell::new(0.0),
            raf: Cell::new(None),
            image_prec: Cell::new(None),
            a_peindre: Cell::new(true),
            a_inventorier: Cell::new(true),
            premiere_cascade: Cell::new(true),
            defilants: RefCell::new(Vec::new()),
            scrubs: RefCell::new(Vec::new()),
            compteurs: RefCell::new(Vec::new()),
            aimant_courant: RefCell::new(None),
            io: RefCell::new(None),
            mo: RefCell::new(None),
            rappel_io: RefCell::new(None),
            rappel_mo: RefCell::new(None),
            boucle: RefCell::new(None),
            ecouteurs: RefCell::new(Vec::new()),
        });

        Interne::branche(&interne);
        if let Some(racine) = interne.document.document_element() {
            let _ = racine.class_list().add_1("mvt");
        }
        interne.inventorie();
        interne.demande_image();
        Some(Moteur { interne })
    }

    /// Fait glisser la page jusqu'à `y`, avec la courbe du moteur.
    pub fn defile_vers(&self, y: f64) {
        let i = &self.interne;
        if !i.glisse.get() {
            let ici = i.fenetre.scroll_y().unwrap_or(0.0);
            i.actuel.set(ici);
        }
        i.cible.set(calcul::borne(y, 0.0, i.defilement_max()));
        i.glisse.set(true);
        i.demande_image();
    }
}

impl Drop for Moteur {
    fn drop(&mut self) {
        self.interne.arrete();
    }
}

impl Interne {
    /// Pose les observateurs, les écouteurs et la boucle.
    fn branche(moi: &Rc<Self>) {
        let faible = Rc::downgrade(moi);

        // --- La boucle d'images ---------------------------------------------
        {
            let f = faible.clone();
            *moi.boucle.borrow_mut() = Some(Closure::new(move |t: f64| {
                if let Some(i) = f.upgrade() {
                    i.image(t);
                }
            }));
        }

        // --- Les révélations ------------------------------------------------
        {
            let f = faible.clone();
            let rappel = Closure::new(move |entrees: Array, _: IntersectionObserver| {
                if let Some(i) = f.upgrade() {
                    i.revele(entrees);
                }
            });
            let init = IntersectionObserverInit::new();
            // Un peu avant le bas de l'écran : l'élément entre déjà visible
            // dans sa course plutôt que d'apparaître collé au bord.
            init.set_root_margin("0px 0px -8% 0px");
            init.set_threshold(&JsValue::from_f64(0.0));
            if let Ok(io) =
                IntersectionObserver::new_with_options(rappel.as_ref().unchecked_ref(), &init)
            {
                *moi.io.borrow_mut() = Some(io);
            }
            *moi.rappel_io.borrow_mut() = Some(rappel);
        }

        // --- Le contenu qui arrive après coup --------------------------------
        // Une application ajoute des éléments au fil des données : chacun
        // doit entrer dans l'inventaire sans qu'on ait à le signaler.
        {
            let f = faible.clone();
            let rappel = Closure::new(move |mutations: Array, _: MutationObserver| {
                let Some(i) = f.upgrade() else { return };
                let utile = mutations.iter().any(|m| {
                    m.dyn_into::<MutationRecord>()
                        .ok()
                        .and_then(|r| r.target())
                        .and_then(|n| element_de(&n))
                        .map(|e| e.closest("[data-mvt-calme]").ok().flatten().is_none())
                        .unwrap_or(true)
                });
                if utile {
                    i.a_inventorier.set(true);
                    i.demande_image();
                }
            });
            if let Ok(mo) = MutationObserver::new(rappel.as_ref().unchecked_ref()) {
                let init = MutationObserverInit::new();
                init.set_child_list(true);
                init.set_subtree(true);
                if let Some(corps) = moi.document.body() {
                    let _ = mo.observe_with_options(&corps, &init);
                }
                *moi.mo.borrow_mut() = Some(mo);
            }
            *moi.rappel_mo.borrow_mut() = Some(rappel);
        }

        // --- Les écouteurs --------------------------------------------------
        let cible_fenetre: EventTarget = moi.fenetre.clone().into();

        let f = faible.clone();
        moi.ecoute(&cible_fenetre, "scroll", true, move |_| {
            if let Some(i) = f.upgrade() {
                i.au_defilement();
            }
        });

        let f = faible.clone();
        moi.ecoute(&cible_fenetre, "resize", true, move |_| {
            if let Some(i) = f.upgrade() {
                i.cible
                    .set(calcul::borne(i.cible.get(), 0.0, i.defilement_max()));
                i.a_peindre.set(true);
                i.demande_image();
            }
        });

        if moi.reglages.molette_douce {
            let f = faible.clone();
            // Non passif, et c'est le seul : c'est la condition pour pouvoir
            // reprendre la main sur la molette.
            moi.ecoute(&cible_fenetre, "wheel", false, move |e| {
                if let (Some(i), Ok(e)) = (f.upgrade(), e.dyn_into::<WheelEvent>()) {
                    i.molette(&e);
                }
            });
        }

        // Le magnétisme n'a de sens qu'avec un vrai pointeur : au doigt, il
        // n'y a pas de survol, seulement un bouton qui sauterait sous le
        // pouce.
        if media("(hover: hover) and (pointer: fine)").unwrap_or(false) {
            let cible_doc: EventTarget = moi.document.clone().into();
            let f = faible.clone();
            moi.ecoute(&cible_doc, "pointermove", true, move |e| {
                if let (Some(i), Ok(e)) = (f.upgrade(), e.dyn_into::<PointerEvent>()) {
                    i.aimante(&e);
                }
            });
            let f = faible;
            moi.ecoute(&cible_doc, "pointerleave", true, move |_| {
                if let Some(i) = f.upgrade() {
                    i.relache_aimant();
                }
            });
        }
    }

    fn ecoute(
        &self,
        cible: &EventTarget,
        evenement: &'static str,
        passif: bool,
        rappel: impl FnMut(Event) + 'static,
    ) {
        let rappel = Closure::<dyn FnMut(Event)>::new(rappel);
        let options = AddEventListenerOptions::new();
        options.set_passive(passif);
        let _ = cible.add_event_listener_with_callback_and_add_event_listener_options(
            evenement,
            rappel.as_ref().unchecked_ref(),
            &options,
        );
        self.ecouteurs.borrow_mut().push(Ecouteur {
            cible: cible.clone(),
            evenement,
            rappel,
        });
    }

    fn arrete(&self) {
        if let Some(id) = self.raf.take() {
            let _ = self.fenetre.cancel_animation_frame(id);
        }
        if let Some(io) = self.io.borrow_mut().take() {
            io.disconnect();
        }
        if let Some(mo) = self.mo.borrow_mut().take() {
            mo.disconnect();
        }
        for e in self.ecouteurs.borrow_mut().drain(..) {
            let _ = e.cible.remove_event_listener_with_callback(
                e.evenement,
                e.rappel.as_ref().unchecked_ref(),
            );
        }
        // Rien ne doit rester masqué derrière un moteur arrêté.
        if let Some(racine) = self.document.document_element() {
            let _ = racine.class_list().remove_1("mvt");
        }
        for c in self.compteurs.borrow_mut().drain(..) {
            c.noeud.set_node_value(Some(&c.final_));
        }
    }

    fn demande_image(&self) {
        if self.raf.get().is_some() {
            return;
        }
        let boucle = self.boucle.borrow();
        let Some(boucle) = boucle.as_ref() else {
            return;
        };
        if let Ok(id) = self
            .fenetre
            .request_animation_frame(boucle.as_ref().unchecked_ref())
        {
            self.raf.set(Some(id));
        }
    }

    fn defilement_max(&self) -> f64 {
        let page = self
            .document
            .document_element()
            .map(|e| e.scroll_height() as f64)
            .unwrap_or(0.0);
        (page - self.hauteur_ecran()).max(0.0)
    }

    fn hauteur_ecran(&self) -> f64 {
        self.fenetre
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    }

    // --- Le défilement -------------------------------------------------------

    fn au_defilement(&self) {
        let y = self.fenetre.scroll_y().unwrap_or(0.0);
        // Un défilement que nous n'avons pas demandé — clavier, barre,
        // ancre, recherche dans la page — reprend la main : on cesse de
        // glisser et l'on repart d'où la page se trouve.
        if !self.glisse.get() || (y - self.dernier_pose.get()).abs() > 2.0 {
            self.glisse.set(false);
            self.actuel.set(y);
            self.cible.set(y);
        }
        self.demande_image();
    }

    fn molette(&self, e: &WheelEvent) {
        // Ctrl + molette, c'est le pincement d'un pavé tactile : le zoom du
        // navigateur, pas un défilement.
        if e.ctrl_key() || e.default_prevented() {
            return;
        }
        let ecran = self.hauteur_ecran();
        let dy = calcul::delta_pixels(e.delta_y(), e.delta_mode(), ecran);
        let dx = calcul::delta_pixels(e.delta_x(), e.delta_mode(), ecran);
        if dx.abs() > dy.abs() || dy == 0.0 {
            return;
        }
        if let Some(cible) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) {
            if self.zone_libre(&cible, dy) {
                return;
            }
        }
        e.prevent_default();
        if !self.glisse.get() {
            let y = self.fenetre.scroll_y().unwrap_or(0.0);
            self.actuel.set(y);
            self.cible.set(y);
        }
        self.cible.set(calcul::borne(
            self.cible.get() + dy,
            0.0,
            self.defilement_max(),
        ));
        self.glisse.set(true);
        self.demande_image();
    }

    /// Vrai si la molette doit garder son comportement natif ici : une zone
    /// déclarée libre (une carte qui zoome), ou une liste qui défile pour son
    /// propre compte et peut encore aller dans ce sens.
    fn zone_libre(&self, depart: &Element, dy: f64) -> bool {
        let mut courant = Some(depart.clone());
        while let Some(el) = courant {
            if el.has_attribute("data-defile-libre") {
                return true;
            }
            if el.scroll_height() > el.client_height() {
                let debordement = self
                    .fenetre
                    .get_computed_style(&el)
                    .ok()
                    .flatten()
                    .and_then(|s| s.get_property_value("overflow-y").ok())
                    .unwrap_or_default();
                if debordement == "auto" || debordement == "scroll" {
                    let haut = el.scroll_top() as f64;
                    let peut = if dy < 0.0 {
                        haut > 0.0
                    } else {
                        haut + (el.client_height() as f64) < el.scroll_height() as f64 - 1.0
                    };
                    if peut {
                        return true;
                    }
                }
            }
            courant = el.parent_element();
        }
        false
    }

    // --- La boucle -----------------------------------------------------------

    fn image(&self, t: f64) {
        self.raf.set(None);
        let dt = match self.image_prec.replace(Some(t)) {
            Some(prec) => ((t - prec) / 1000.0).clamp(0.0, 0.1),
            None => 1.0 / 60.0,
        };
        let mut encore = false;

        if self.a_inventorier.replace(false) {
            self.inventorie();
        }

        if self.glisse.get() {
            let (y, fini) =
                calcul::pas_inertiel(self.actuel.get(), self.cible.get(), self.reglages.lerp, dt);
            self.actuel.set(y);
            self.dernier_pose.set(y);
            let o = ScrollToOptions::new();
            o.set_top(y);
            // `instant` et non `auto` : `auto` obéit au `scroll-behavior` de
            // la feuille, et une page en `smooth` lancerait sa propre
            // animation à chaque image — deux glisses qui se battent.
            o.set_behavior(ScrollBehavior::Instant);
            self.fenetre.scroll_to_with_scroll_to_options(&o);
            if fini {
                self.glisse.set(false);
            } else {
                encore = true;
            }
        }

        let y = self.fenetre.scroll_y().unwrap_or(0.0);
        let vitesse = if dt > 0.0 {
            (y - self.dernier_y.get()) / dt
        } else {
            0.0
        };
        let bouge = y != self.dernier_y.get();
        self.dernier_y.set(y);
        let elan = calcul::lerp(
            self.elan.get(),
            calcul::elan(vitesse),
            calcul::amorti(0.12, dt),
        );
        let elan = if elan.abs() < 0.001 { 0.0 } else { elan };
        let elan_change = elan != self.elan.get();
        self.elan.set(elan);
        if elan != 0.0 {
            encore = true;
        }

        if bouge || elan_change || self.a_peindre.replace(false) {
            self.peint(y, elan);
        }

        if self.avance_compteurs(t) {
            encore = true;
        }

        if encore {
            self.demande_image();
        } else {
            self.image_prec.set(None);
        }
    }

    /// Écrit les propriétés liées au défilement. Sur les éléments qui les
    /// demandent seulement : posée sur la racine, une propriété héritée
    /// ferait recalculer le style du document entier à chaque image.
    fn peint(&self, y: f64, elan: f64) {
        let ecran = self.hauteur_ecran();
        let page = self
            .document
            .document_element()
            .map(|e| e.scroll_height() as f64)
            .unwrap_or(0.0);
        let progression = calcul::progression_page(y, page, ecran);
        let ecrans = if ecran > 0.0 { y / ecran } else { 0.0 };

        for s in self.defilants.borrow().iter() {
            let _ = s.set_property("--defile", &format!("{progression:.4}"));
            let _ = s.set_property("--defile-ecrans", &format!("{ecrans:.4}"));
            let _ = s.set_property("--elan", &format!("{elan:.4}"));
        }
        for (el, s) in self.scrubs.borrow().iter() {
            let r = el.get_bounding_client_rect();
            let p = calcul::traversee(r.top(), r.height(), ecran);
            let _ = s.set_property("--p", &format!("{p:.4}"));
        }
    }

    /// Recense les éléments à animer. Appelé au démarrage puis à chaque
    /// arrivée de contenu.
    fn inventorie(&self) {
        if let Some(io) = self.io.borrow().as_ref() {
            for el in tous(&self.document, "[data-mvt]:not([data-mvt-suivi])") {
                let _ = el.set_attribute("data-mvt-suivi", "");
                io.observe(&el);
            }
        }
        *self.defilants.borrow_mut() = tous(&self.document, "[data-defile]")
            .iter()
            .filter_map(style_de)
            .collect();
        *self.scrubs.borrow_mut() = tous(&self.document, "[data-scrub]")
            .into_iter()
            .filter_map(|e| style_de(&e).map(|s| (e, s)))
            .collect();
        self.a_peindre.set(true);
    }

    // --- Les révélations -----------------------------------------------------

    fn revele(&self, entrees: Array) {
        let mut entrants: Vec<(f64, f64, HtmlElement)> = entrees
            .iter()
            .filter_map(|e| e.dyn_into::<IntersectionObserverEntry>().ok())
            .filter(|e| e.is_intersecting())
            .filter_map(|e| {
                let r = e.bounding_client_rect();
                e.target()
                    .dyn_into::<HtmlElement>()
                    .ok()
                    .map(|el| (r.top(), r.left(), el))
            })
            .collect();
        if entrants.is_empty() {
            return;
        }
        // L'ordre de lecture, pas celui de l'observateur : de haut en bas,
        // puis de gauche à droite.
        entrants.sort_by(|a, b| {
            (a.0.round(), a.1)
                .partial_cmp(&(b.0.round(), b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let base = if self.premiere_cascade.replace(false) {
            self.reglages.retard_initial
        } else {
            0.0
        };
        let maintenant = self.fenetre.performance().map(|p| p.now()).unwrap_or(0.0);
        let io = self.io.borrow();
        for (rang, (_, _, el)) in entrants.into_iter().enumerate() {
            let retard = base + calcul::retard(rang, self.reglages.pas_cascade, 6);
            let _ = el
                .style()
                .set_property("--retard", &format!("{retard:.2}s"));
            let _ = el.set_attribute("data-vu", "");
            if let Some(io) = io.as_ref() {
                io.unobserve(&el);
            }
            if el.has_attribute("data-compte") {
                self.lance_compteur(&el, maintenant + retard * 1000.0);
            }
            // Les nombres d'un bloc révélé comptent avec lui : un prix dans
            // une carte, une mesure dans une phrase. Un léger décalage de
            // l'un à l'autre, comme la cascade.
            for (k, n) in tous_dans(&el, "[data-compte]").into_iter().enumerate() {
                if let Ok(n) = n.dyn_into::<HtmlElement>() {
                    let decale = retard + 0.25 + calcul::retard(k, 0.12, 6);
                    self.lance_compteur(&n, maintenant + decale * 1000.0);
                }
            }
        }
        drop(io);
        self.demande_image();
    }

    /// Fait compter le nombre d'un élément.
    ///
    /// On écrit dans le nœud de texte existant, jamais à sa place : le rendu
    /// garde une référence vers ce nœud, et le remplacer le laisserait
    /// mettre à jour un nœud détaché de la page.
    fn lance_compteur(&self, el: &HtmlElement, debut_ms: f64) {
        let Some(noeud) = el.first_child() else {
            return;
        };
        if noeud.node_type() != Node::TEXT_NODE || noeud.next_sibling().is_some() {
            return;
        }
        let final_ = noeud.node_value().unwrap_or_default();
        let Some(chiffre) = calcul::decompose(&final_) else {
            return;
        };
        noeud.set_node_value(Some(&calcul::valeur_comptee(&chiffre, 0.0)));
        self.compteurs.borrow_mut().push(Compteur {
            noeud,
            chiffre,
            final_,
            debut_ms,
        });
    }

    /// Avance les compteurs ; vrai s'il en reste.
    fn avance_compteurs(&self, t: f64) -> bool {
        const DUREE_MS: f64 = 1600.0;
        let mut compteurs = self.compteurs.borrow_mut();
        compteurs.retain(|c| {
            let avance = ((t - c.debut_ms) / DUREE_MS).clamp(0.0, 1.0);
            if avance >= 1.0 {
                // La dernière valeur est le texte d'origine, à l'octet près :
                // un arrondi ne doit pas laisser « 3,98 € » à l'écran.
                c.noeud.set_node_value(Some(&c.final_));
                false
            } else {
                c.noeud
                    .set_node_value(Some(&calcul::valeur_comptee(&c.chiffre, avance)));
                true
            }
        });
        !compteurs.is_empty()
    }

    // --- Le magnétisme -------------------------------------------------------

    fn aimante(&self, e: &PointerEvent) {
        let sous = e
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|el| el.closest("[data-aimant]").ok().flatten())
            .and_then(|el| el.dyn_into::<HtmlElement>().ok());
        let courant = self.aimant_courant.borrow().clone();
        if courant != sous {
            self.relache_aimant();
        }
        let Some(el) = sous else { return };
        let r = el.get_bounding_client_rect();
        let centre = (r.left() + r.width() / 2.0, r.top() + r.height() / 2.0);
        let (x, y) = calcul::aimant(
            (e.client_x() as f64, e.client_y() as f64),
            centre,
            0.28,
            10.0,
        );
        let s = el.style();
        let _ = s.set_property("--ax", &format!("{x:.1}px"));
        let _ = s.set_property("--ay", &format!("{y:.1}px"));
        *self.aimant_courant.borrow_mut() = Some(el);
    }

    fn relache_aimant(&self) {
        if let Some(el) = self.aimant_courant.borrow_mut().take() {
            let s = el.style();
            let _ = s.set_property("--ax", "0px");
            let _ = s.set_property("--ay", "0px");
        }
    }
}

/// Les éléments qui répondent à un sélecteur.
fn tous(d: &Document, selecteur: &str) -> Vec<Element> {
    let Ok(liste) = d.query_selector_all(selecteur) else {
        return Vec::new();
    };
    (0..liste.length())
        .filter_map(|i| liste.item(i).and_then(|n| n.dyn_into::<Element>().ok()))
        .collect()
}

/// Le style en ligne d'un élément HTML **ou SVG**.
///
/// Un `<svg>` n'est pas un `HtmlElement` : filtré comme tel, le fil de
/// l'itinéraire de la vitrine ne recevait jamais sa progression et restait
/// effacé — constaté en mesurant son `stroke-dashoffset`, figé à 1.
fn style_de(el: &Element) -> Option<CssStyleDeclaration> {
    if let Some(h) = el.dyn_ref::<HtmlElement>() {
        return Some(h.style());
    }
    el.dyn_ref::<SvgElement>().map(SvgElement::style)
}

/// Les descendants d'un élément qui répondent à un sélecteur.
fn tous_dans(el: &Element, selecteur: &str) -> Vec<Element> {
    let Ok(liste) = el.query_selector_all(selecteur) else {
        return Vec::new();
    };
    (0..liste.length())
        .filter_map(|i| liste.item(i).and_then(|n| n.dyn_into::<Element>().ok()))
        .collect()
}

/// L'élément d'un nœud : lui-même, ou son parent pour un nœud de texte.
fn element_de(n: &Node) -> Option<Element> {
    n.clone()
        .dyn_into::<Element>()
        .ok()
        .or_else(|| n.parent_element())
}
