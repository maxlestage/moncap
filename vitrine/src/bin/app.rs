//! Le point d'entrée du WebAssembly.
//!
//! La coquille HTML dit quoi afficher par des attributs `data-` sur le point
//! de montage : la langue, la page, et le chemin du logo — que seule la
//! construction connaît, puisqu'il porte une empreinte.
//!
//! Un seul WebAssembly sert les neuf pages. L'alternative aurait été de
//! compiler un paquet par langue, plus petit de quelques kilooctets ; elle
//! triplait le temps de construction et interdisait au serveur de changer la
//! langue d'une adresse sans tout refaire.

use vitrine::app::{App, Page, Props, Ressources};
use vitrine::contenu::Langue;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

fn main() {
    let Some(racine) = point_de_montage() else {
        // Sans point de montage il n'y a rien à faire, et surtout rien à
        // casser : la coquille garde son repli `<noscript>`, qui reste
        // affiché puisque personne ne l'a remplacé.
        return;
    };

    let langue = lis(&racine, "langue")
        .and_then(|c| Langue::depuis_code(&c))
        .unwrap_or(Langue::Fr);
    let page = match lis(&racine, "page").as_deref() {
        Some("mentions") => Page::Mentions,
        Some("confidentialite") => Page::Confidentialite,
        _ => Page::Accueil,
    };
    let logo = lis(&racine, "logo").unwrap_or_default();

    let props = Props {
        langue,
        page,
        res: Ressources { logo: logo.into() },
        annee: annee_courante(),
    };

    yew::Renderer::<App>::with_root_and_props(racine.into(), props).render();
}

fn point_de_montage() -> Option<HtmlElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id("racine")?
        .dyn_into::<HtmlElement>()
        .ok()
}

fn lis(el: &HtmlElement, cle: &str) -> Option<String> {
    el.get_attribute(&format!("data-{cle}"))
        .filter(|v| !v.is_empty())
}

/// L'année du copyright, lue à l'exécution.
///
/// Lue et non figée à la construction : une année gravée dans le binaire
/// devient fausse le 1er janvier, et personne ne pense à reconstruire pour
/// ça. C'est la seule chose que cette page demande vraiment au navigateur.
fn annee_courante() -> i32 {
    js_sys::Date::new_0().get_full_year() as i32
}
