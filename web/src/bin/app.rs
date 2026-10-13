//! Le point d'entrée du WebAssembly.
//!
//! La coquille HTML transmet sur le point de montage, en JSON, les adresses
//! des fichiers — que seule la construction connaît, puisqu'elles portent
//! une empreinte.

#[cfg(target_arch = "wasm32")]
fn main() {
    use std::rc::Rc;

    use mouvement::{Moteur, Reglages};
    use web::catalogue::Ressources;

    let Some(racine) = web_sys::window()
        .and_then(|f| f.document())
        .and_then(|d| d.get_element_by_id("racine"))
    else {
        // Sans point de montage, rien à faire : la coquille garde son
        // message `<noscript>`.
        return;
    };
    let ressources: Ressources = racine
        .get_attribute("data-ressources")
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    let vue = web::app(Rc::new(ressources), web::session::session());
    paquets::mount_to("#racine", vue);

    // Le moteur démarre sur la page montée, et tourne aussi longtemps
    // qu'elle : il suit ensuite seul ce qui y entre. Puis le rideau se lève.
    std::mem::forget(Moteur::demarre(Reglages::default()));
    mouvement::leve_rideau();
}

/// En natif, ce binaire n'a rien à faire : il n'existe que pour le
/// navigateur. Il se compile quand même, pour les vérifications.
#[cfg(not(target_arch = "wasm32"))]
fn main() {}
