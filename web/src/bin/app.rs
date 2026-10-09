//! Le point d'entrée du WebAssembly.
//!
//! La coquille HTML transmet sur le point de montage, en JSON, les adresses
//! des fichiers — que seule la construction connaît, puisqu'elles portent
//! une empreinte.

use std::rc::Rc;

use web::app::{App, Props};
use web::catalogue::Ressources;

fn main() {
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
    let props = Props {
        ressources: Rc::new(ressources),
        session: web::session::session(),
    };
    yew::Renderer::<App>::with_root_and_props(racine, props).render();
}
