//! Le navigateur, vu d'un code qui se compile aussi en natif.
//!
//! Le même composant tourne dans le navigateur et dans les tests, où il se
//! rend en HTML. Hors WebAssembly, appeler une fonction JavaScript fait
//! paniquer : chaque accès au navigateur passe donc par ici, et n'y trouve
//! rien en natif.

use std::cell::RefCell;

use paquets::active::{effect, untrack, Dom, Event};

/// La fenêtre, absente hors du navigateur.
pub fn fenetre() -> Option<web_sys::Window> {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// L'heure, en millisecondes depuis l'époque Unix (0 hors navigateur).
pub fn maintenant_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}

/// L'horloge de la page, pour mesurer des durées (0 hors navigateur).
pub fn horloge_ms() -> f64 {
    fenetre()
        .and_then(|f| f.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

/// Le système accepte-t-il le mouvement ? Faux hors navigateur : le rendu
/// de test est celui d'une page immobile.
pub fn mouvement_permis() -> bool {
    cfg!(target_arch = "wasm32") && mouvement::mouvement_permis()
}

/// L'événement du navigateur derrière celui d'`active`.
pub fn brut(e: &Event) -> Option<web_sys::Event> {
    #[cfg(target_arch = "wasm32")]
    {
        Some(e.raw().clone())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = e;
        None
    }
}

/// L'élément du DOM qu'`on_mount` reçoit.
pub fn element(d: &Dom) -> Option<web_sys::Element> {
    #[cfg(target_arch = "wasm32")]
    {
        Some(d.clone())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = d;
        None
    }
}

/// Lance une tâche asynchrone — dans le navigateur seulement.
pub fn lance(f: impl std::future::Future<Output = ()> + 'static) {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(f);
    #[cfg(not(target_arch = "wasm32"))]
    drop(f);
}

/// Exécute `f` une fois, au montage du composant, sans le lier à aucun
/// état : l'équivalent d'un `use_effect_with((), …)`. Ne tourne que dans le
/// navigateur — en natif, les effets d'`active` ne s'exécutent pas.
pub fn au_montage(f: impl FnOnce() + 'static) {
    let f = RefCell::new(Some(f));
    effect(move || {
        let f = f.borrow_mut().take();
        if let Some(f) = f {
            untrack(f);
        }
    });
}
