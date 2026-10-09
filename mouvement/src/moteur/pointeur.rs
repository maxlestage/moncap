//! Le pointeur : le curseur qui suit, la parallaxe, l'inclinaison, le
//! magnétisme — à la souris seulement — et les ondes des clics, au doigt
//! comme à la souris.
//!
//! Au doigt, il n'y a pas de survol : un curseur suivrait un pouce qui
//! cache déjà ce qu'il touche, une carte s'inclinerait sous un appui qui
//! veut simplement faire défiler, un bouton sauterait. Seule l'onde a un
//! sens partout — elle répond à un geste, elle n'en devance aucun.

use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Element, EventTarget, HtmlElement, MouseEvent, PointerEvent};

use super::{media, Interne};
use crate::calcul;

/// Ce qui, sous le curseur, l'invite à grossir : ce qu'on peut activer.
const INTERACTIF: &str =
    "a[href], button, summary, label, input, select, textarea, [role=button], [data-curseur]";

/// Le curseur qui suit la souris : un point, exact, et un anneau qui le
/// rattrape avec un temps de retard — le geste des sites qui bougent.
///
/// Il s'ajoute au curseur du système, il ne le remplace pas : cacher la
/// flèche serait rendre la page moins lisible pour décorer.
pub(super) struct Curseur {
    racine: HtmlElement,
    point: HtmlElement,
    anneau: HtmlElement,
    texte: HtmlElement,
    etiquette: Cell<bool>,
}

impl Curseur {
    pub(super) fn retire(self) {
        self.racine.remove();
    }
}

fn element(i: &Interne, classe: &str) -> Option<HtmlElement> {
    let el = i.document.create_element("span").ok()?;
    el.set_class_name(classe);
    el.dyn_into().ok()
}

impl Interne {
    pub(super) fn branche_pointeur(moi: &Rc<Self>) {
        let doc: EventTarget = moi.document.clone().into();

        let f = Rc::downgrade(moi);
        moi.ecoute(&doc, "pointerdown", true, move |e| {
            if let (Some(i), Ok(e)) = (f.upgrade(), e.dyn_into::<PointerEvent>()) {
                i.onde(&e);
                i.appui(true);
            }
        });
        let f = Rc::downgrade(moi);
        moi.ecoute(&doc, "pointerup", true, move |_| {
            if let Some(i) = f.upgrade() {
                i.appui(false);
            }
        });

        if !media("(hover: hover) and (pointer: fine)").unwrap_or(false) {
            return;
        }
        moi.cree_curseur();

        let f = Rc::downgrade(moi);
        moi.ecoute(&doc, "pointermove", true, move |e| {
            if let (Some(i), Ok(e)) = (f.upgrade(), e.dyn_into::<PointerEvent>()) {
                i.bouge(&e);
            }
        });
        // La souris quitte la fenêtre : `mouseout` sans élément d'arrivée.
        let f = Rc::downgrade(moi);
        moi.ecoute(&doc, "mouseout", true, move |e| {
            let Some(i) = f.upgrade() else { return };
            let sortie = e
                .dyn_ref::<MouseEvent>()
                .is_some_and(|m| m.related_target().is_none());
            if sortie {
                i.quitte();
            }
        });
    }

    fn cree_curseur(&self) {
        let (Some(racine), Some(point), Some(anneau), Some(texte), Some(corps)) = (
            element(self, "curseur cache"),
            element(self, "curseur-point"),
            element(self, "curseur-anneau"),
            element(self, "curseur-texte"),
            self.document.body(),
        ) else {
            return;
        };
        let _ = racine.set_attribute("aria-hidden", "true");
        let _ = anneau.append_child(&texte);
        let _ = racine.append_child(&anneau);
        let _ = racine.append_child(&point);
        let _ = corps.append_child(&racine);
        *self.curseur.borrow_mut() = Some(Curseur {
            racine,
            point,
            anneau,
            texte,
            etiquette: Cell::new(false),
        });
    }

    fn bouge(&self, e: &PointerEvent) {
        let (x, y) = (e.client_x() as f64, e.client_y() as f64);
        if self.pointeur.replace(Some((x, y))).is_none() {
            // Première apparition : l'anneau part d'ici, pas du coin.
            self.pointeur_lisse.set((x, y));
        }
        let sous = e.target().and_then(|t| t.dyn_into::<Element>().ok());
        self.survole(x, y, sous.as_ref());
        self.demande_image();
    }

    /// Ce qui est sous la souris, relu après un défilement : sans cela, le
    /// curseur garderait l'étiquette d'un bouton parti plus haut, et une
    /// carte resterait penchée vers une souris qui ne la touche plus.
    pub(super) fn resurvole(&self) {
        let Some((x, y)) = self.pointeur.get() else {
            return;
        };
        let sous = self.document.element_from_point(x as f32, y as f32);
        self.survole(x, y, sous.as_ref());
    }

    fn survole(&self, x: f64, y: f64, sous: Option<&Element>) {
        if let Some(c) = self.curseur.borrow().as_ref() {
            let _ = c.racine.class_list().remove_1("cache");
            let _ = c
                .point
                .style()
                .set_property("transform", &format!("translate3d({x:.1}px,{y:.1}px,0)"));
            let interactif = sous.and_then(|el| el.closest(INTERACTIF).ok().flatten());
            let _ = c
                .racine
                .class_list()
                .toggle_with_force("survol", interactif.is_some());
            let texte = interactif
                .as_ref()
                .and_then(|el| el.closest("[data-curseur]").ok().flatten())
                .and_then(|el| el.get_attribute("data-curseur"))
                .filter(|t| !t.is_empty());
            let a_texte = texte.is_some();
            if a_texte || c.etiquette.get() {
                c.texte.set_text_content(texte.as_deref());
                let _ = c
                    .racine
                    .class_list()
                    .toggle_with_force("etiquette", a_texte);
                c.etiquette.set(a_texte);
            }
        }

        self.aimante((x, y), sous);
        self.incline((x, y), sous);
    }

    fn quitte(&self) {
        self.pointeur.set(None);
        if let Some(c) = self.curseur.borrow().as_ref() {
            let _ = c.racine.class_list().add_1("cache");
        }
        self.relache_aimant();
        self.relache_incline();
    }

    fn appui(&self, enfonce: bool) {
        if let Some(c) = self.curseur.borrow().as_ref() {
            let _ = c.racine.class_list().toggle_with_force("appui", enfonce);
        }
    }

    /// Une image du pointeur : l'anneau rattrape le point, la parallaxe
    /// suit l'anneau. Vrai tant qu'il reste du chemin.
    pub(super) fn avance_pointeur(&self, dt: f64) -> bool {
        let Some(cible) = self.pointeur.get() else {
            return false;
        };
        let (x, y) = self.pointeur_lisse.get();
        let k = calcul::amorti(0.16, dt);
        let (nx, ny) = (calcul::lerp(x, cible.0, k), calcul::lerp(y, cible.1, k));
        let encore = (nx - cible.0).abs() > 0.1 || (ny - cible.1).abs() > 0.1;
        let (nx, ny) = if encore { (nx, ny) } else { cible };
        if (nx, ny) == (x, y) {
            return false;
        }
        self.pointeur_lisse.set((nx, ny));

        if let Some(c) = self.curseur.borrow().as_ref() {
            let _ = c
                .anneau
                .style()
                .set_property("transform", &format!("translate3d({nx:.1}px,{ny:.1}px,0)"));
        }
        let souris = self.souris.borrow();
        if !souris.is_empty() {
            let largeur = self
                .fenetre
                .inner_width()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let (sx, sy) = calcul::pointeur_normalise(nx, ny, largeur, self.hauteur_ecran());
            for s in souris.iter() {
                let _ = s.set_property("--sx", &format!("{sx:.4}"));
                let _ = s.set_property("--sy", &format!("{sy:.4}"));
            }
        }
        encore
    }

    // --- Le magnétisme -------------------------------------------------------

    fn aimante(&self, (x, y): (f64, f64), sous: Option<&Element>) {
        let sous = sous
            .and_then(|el| el.closest("[data-aimant]").ok().flatten())
            .and_then(|el| el.dyn_into::<HtmlElement>().ok());
        if *self.aimant_courant.borrow() != sous {
            self.relache_aimant();
        }
        let Some(el) = sous else { return };
        let r = el.get_bounding_client_rect();
        let centre = (r.left() + r.width() / 2.0, r.top() + r.height() / 2.0);
        let (x, y) = calcul::aimant((x, y), centre, 0.28, 10.0);
        let s = el.style();
        let _ = s.set_property("--ax", &format!("{x:.1}px"));
        let _ = s.set_property("--ay", &format!("{y:.1}px"));
        *self.aimant_courant.borrow_mut() = Some(el);
    }

    pub(super) fn relache_aimant(&self) {
        if let Some(el) = self.aimant_courant.borrow_mut().take() {
            let s = el.style();
            let _ = s.set_property("--ax", "0px");
            let _ = s.set_property("--ay", "0px");
        }
    }

    // --- L'inclinaison -------------------------------------------------------

    /// La carte sous la souris s'incline vers elle, et un halo la suit
    /// (`--mx`, `--my`, en pixels dans la carte). `data-incline` peut donner
    /// l'angle maximal, en degrés ; six par défaut.
    fn incline(&self, (x, y): (f64, f64), sous: Option<&Element>) {
        let sous = sous
            .and_then(|el| el.closest("[data-incline]").ok().flatten())
            .and_then(|el| el.dyn_into::<HtmlElement>().ok());
        if *self.incline_courant.borrow() != sous {
            self.relache_incline();
        }
        let Some(el) = sous else { return };
        let r = el.get_bounding_client_rect();
        let local = (x - r.left(), y - r.top());
        let max = el
            .get_attribute("data-incline")
            .and_then(|v| v.parse::<u8>().ok())
            .map(f64::from)
            .unwrap_or(6.0);
        let (rx, ry) = calcul::inclinaison(local, (r.width(), r.height()), max);
        let s = el.style();
        let _ = s.set_property("--rx", &format!("{rx:.2}deg"));
        let _ = s.set_property("--ry", &format!("{ry:.2}deg"));
        let _ = s.set_property("--mx", &format!("{:.0}px", local.0));
        let _ = s.set_property("--my", &format!("{:.0}px", local.1));
        let _ = el.set_attribute("data-survol", "");
        *self.incline_courant.borrow_mut() = Some(el);
    }

    pub(super) fn relache_incline(&self) {
        if let Some(el) = self.incline_courant.borrow_mut().take() {
            let s = el.style();
            let _ = s.set_property("--rx", "0deg");
            let _ = s.set_property("--ry", "0deg");
            let _ = el.remove_attribute("data-survol");
        }
    }

    // --- Les ondes -------------------------------------------------------------

    /// Une onde part du point touché, dans l'élément `data-onde` touché.
    ///
    /// Elle est ajoutée en dernier enfant puis retirée : le rendu ne la
    /// connaît pas, et ses propres nœuds restent à leur place.
    fn onde(&self, e: &PointerEvent) {
        if e.button() != 0 {
            return;
        }
        let Some(el) = e
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|el| el.closest("[data-onde]").ok().flatten())
        else {
            return;
        };
        let Some(onde) = element(self, "onde-clic") else {
            return;
        };
        let r = el.get_bounding_client_rect();
        let taille = r.width().max(r.height()) * 2.4;
        let s = onde.style();
        let _ = s.set_property("left", &format!("{:.0}px", e.client_x() as f64 - r.left()));
        let _ = s.set_property("top", &format!("{:.0}px", e.client_y() as f64 - r.top()));
        let _ = s.set_property("--taille", &format!("{taille:.0}px"));
        let _ = onde.set_attribute("aria-hidden", "true");
        let _ = el.append_child(&onde);
        let fin = Closure::once_into_js(move || onde.remove());
        let _ = self
            .fenetre
            .set_timeout_with_callback_and_timeout_and_arguments_0(fin.unchecked_ref(), 900);
    }
}
