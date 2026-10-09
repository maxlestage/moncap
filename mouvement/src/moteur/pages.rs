//! Le volet de sortie : la transition de page.
//!
//! Sur zamocorp.com, quitter une page ne la fait pas disparaître d'un coup :
//! un panneau monte, couvre l'écran, et la page suivante se découvre
//! derrière. Ici, le volet monte avant de suivre un lien interne ; la page
//! d'arrivée a son propre rideau, de la même couleur, qui se lève quand
//! elle est prête. Les deux se répondent : on ne voit jamais de blanc.
//!
//! La règle de ce qui mérite un volet est dans `calcul::volet_de_sortie` :
//! une vraie navigation interne, rien d'autre. Un nouvel onglet, un
//! téléchargement, une ancre ou un site extérieur gardent le comportement
//! du navigateur, sans délai.

use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Element, EventTarget, HtmlElement, MouseEvent, PageTransitionEvent, Url};

use super::Interne;
use crate::calcul::{self, Clic};

/// Le temps que met le volet à couvrir l'écran — celui de `moteur.css`.
const MONTEE_MS: i32 = 560;
/// Le filet de sécurité : si la navigation n'a pas eu lieu (annulée,
/// bloquée), le volet redescend plutôt que de laisser la page couverte.
const SECOURS_MS: i32 = 4000;

impl Interne {
    pub(super) fn branche_pages(moi: &Rc<Self>) {
        let doc: EventTarget = moi.document.clone().into();
        let f = Rc::downgrade(moi);
        // Non passif : c'est la condition pour retenir la navigation le
        // temps que le volet monte.
        moi.ecoute(&doc, "click", false, move |e| {
            if let (Some(i), Ok(e)) = (f.upgrade(), e.dyn_into::<MouseEvent>()) {
                i.clic(&e);
            }
        });
        // Revenir en arrière ressort la page du cache du navigateur telle
        // qu'on l'a quittée — volet compris. On le rouvre.
        let fenetre: EventTarget = moi.fenetre.clone().into();
        let f = Rc::downgrade(moi);
        moi.ecoute(&fenetre, "pageshow", true, move |e| {
            let du_cache = e
                .dyn_ref::<PageTransitionEvent>()
                .is_some_and(|p| p.persisted());
            if let (true, Some(i)) = (du_cache, f.upgrade()) {
                i.rouvre();
            }
        });
    }

    fn clic(&self, e: &MouseEvent) {
        let Some(lien) = e
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|el| el.closest("a[href]").ok().flatten())
        else {
            return;
        };
        let lieu = self.fenetre.location();
        let (Ok(ici), Ok(origine), Ok(chemin)) = (lieu.href(), lieu.origin(), lieu.pathname())
        else {
            return;
        };
        let Some(Ok(url)) = lien
            .get_attribute("href")
            .map(|h| Url::new_with_base(&h, &ici))
        else {
            return;
        };
        let cible = lien.get_attribute("target").unwrap_or_default();
        let clic = Clic {
            bouton: e.button(),
            modificateur: e.ctrl_key() || e.meta_key() || e.shift_key() || e.alt_key(),
            deja_pris: e.default_prevented(),
            cible: &cible,
            telechargement: lien.has_attribute("download"),
            meme_origine: url.origin() == origine,
            chemin_actuel: &chemin,
            chemin_cible: &url.pathname(),
        };
        if !calcul::volet_de_sortie(&clic) {
            return;
        }
        e.prevent_default();
        let Some(volet) = self.volet() else {
            // Sans volet, on navigue tout de suite plutôt que pas du tout.
            let _ = lieu.assign(&url.href());
            return;
        };
        let _ = volet.class_list().add_1("part");

        let adresse = url.href();
        let part = Closure::once_into_js(move || {
            if let Some(f) = web_sys::window() {
                let _ = f.location().assign(&adresse);
            }
        });
        let _ = self
            .fenetre
            .set_timeout_with_callback_and_timeout_and_arguments_0(part.unchecked_ref(), MONTEE_MS);
        let secours = Closure::once_into_js(move || {
            let _ = volet.class_list().remove_1("part");
        });
        let _ = self
            .fenetre
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                secours.unchecked_ref(),
                SECOURS_MS,
            );
    }

    /// Le volet, créé à la première sortie.
    fn volet(&self) -> Option<HtmlElement> {
        if let Some(v) = self.volet.borrow().as_ref() {
            return Some(v.clone());
        }
        let v: HtmlElement = self.document.create_element("div").ok()?.dyn_into().ok()?;
        v.set_class_name("volet-sortie");
        let _ = v.set_attribute("aria-hidden", "true");
        let anneau = self.document.create_element("span").ok()?;
        anneau.set_class_name("volet-sortie-anneau");
        let _ = v.append_child(&anneau);
        self.document.body()?.append_child(&v).ok()?;
        // Une image pour que l'état de départ soit peint avant d'ajouter la
        // classe : sans elle, le navigateur fusionne les deux états et le
        // volet apparaît d'un coup au lieu de monter.
        let _ = v.offset_height();
        *self.volet.borrow_mut() = Some(v.clone());
        Some(v)
    }

    fn rouvre(&self) {
        if let Some(v) = self.volet.borrow().as_ref() {
            let _ = v.class_list().remove_1("part");
        }
    }
}
