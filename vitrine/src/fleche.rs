//! La flèche de défilement, qui lit le sens où l'on va.
//!
//! C'étaient cinquante lignes de JavaScript écrites à la main dans une
//! chaîne de caractères, qu'aucun outil ne vérifiait — ni compilateur, ni
//! typage, ni test. Elles deviennent du Rust, que le compilateur relit.
//! C'est, concrètement, ce que le passage au WebAssembly achète ici : le
//! seul script de l'ancienne page était aussi le seul code hors contrôle.
//!
//! Le comportement ne change pas d'un cheveu. En descendant, la flèche mène
//! au prochain titre de section ; en remontant, ou arrivé en bas, elle
//! ramène au bandeau. Sa teinte suit la section traversée.

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};
use yew::prelude::*;

use crate::contenu::Langue;

/// Les trois teintes de la maison, en jetons CSS : elles restent justes dans
/// les quatre thèmes.
const TEINTES: [&str; 3] = ["var(--accent)", "var(--cyan)", "var(--orange)"];

/// Le sens vers lequel la flèche pointe.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sens {
    Haut,
    Bas,
}

impl Sens {
    pub const fn glyphe(self) -> &'static str {
        match self {
            Sens::Haut => "↑",
            Sens::Bas => "⌄",
        }
    }
}

/// Où pointe la flèche, à partir de l'état du défilement.
///
/// Fonction pure, et c'est tout l'intérêt : la règle se teste sans
/// navigateur, là où l'ancien JavaScript ne se testait pas du tout.
///
/// En bas de page, remonter est la seule chose qui reste à faire. Tout en
/// haut, descendre est la seule qui ait un sens — même si l'on vient de
/// remonter, sans quoi la flèche proposerait d'aller là où l'on est déjà.
pub fn sens(y: f64, hauteur_ecran: f64, hauteur_page: f64, monte: bool) -> Sens {
    let en_bas = hauteur_ecran + y >= hauteur_page - 60.0;
    let en_haut = y < 60.0;
    if en_bas || (monte && !en_haut) {
        Sens::Haut
    } else {
        Sens::Bas
    }
}

/// L'indice de la section traversée : le dernier titre passé au-dessus du
/// milieu de l'écran. Zéro tant qu'on n'en a croisé aucun.
pub fn section_courante(hauts: &[f64], hauteur_ecran: f64) -> usize {
    hauts.iter().filter(|h| **h < hauteur_ecran * 0.5).count()
}

/// La teinte d'une section, en boucle sur les trois couleurs.
pub fn teinte(section: usize) -> &'static str {
    TEINTES[section % TEINTES.len()]
}

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub langue: Langue,
}

#[function_component]
pub fn FlecheNav(p: &Props) -> Html {
    let c = p.langue.contenu();
    let noeud = use_node_ref();

    {
        let noeud = noeud.clone();
        let descendre = c.chevrons.descendre;
        let remonter = c.chevrons.remonter;

        use_effect_with((), move |_| {
            let fenetre = web_sys::window().expect("une page a toujours une fenêtre");
            let dernier = std::rc::Rc::new(std::cell::Cell::new(fenetre.scroll_y().unwrap_or(0.0)));
            let monte = std::rc::Rc::new(std::cell::Cell::new(false));

            let maj = {
                let noeud = noeud.clone();
                let dernier = dernier.clone();
                let monte = monte.clone();
                move || {
                    let Some(f) = web_sys::window() else { return };
                    let Some(fleche) = noeud.cast::<HtmlElement>() else {
                        return;
                    };
                    let y = f.scroll_y().unwrap_or(0.0);
                    // Un seuil de quatre pixels : sans lui, le tremblement
                    // d'un défilement par inertie inverserait le sens sans
                    // qu'on ait changé d'avis.
                    if (y - dernier.get()).abs() > 4.0 {
                        monte.set(y < dernier.get());
                        dernier.set(y);
                    }
                    let hauteur_ecran = f
                        .inner_height()
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let hauteur_page = f
                        .document()
                        .and_then(|d| d.document_element())
                        .map(|e| e.scroll_height() as f64)
                        .unwrap_or(0.0);

                    let s = sens(y, hauteur_ecran, hauteur_page, monte.get());
                    fleche.set_text_content(Some(s.glyphe()));
                    let etiquette = match s {
                        Sens::Haut => remonter,
                        Sens::Bas => descendre,
                    };
                    let _ = fleche.set_attribute("aria-label", etiquette);
                    let _ = fleche.set_attribute(
                        "data-sens",
                        match s {
                            Sens::Haut => "haut",
                            Sens::Bas => "bas",
                        },
                    );

                    let hauts = hauts_des_titres(&f);
                    let _ = fleche
                        .style()
                        .set_property("--teinte", teinte(section_courante(&hauts, hauteur_ecran)));
                }
            };

            maj();
            let rappel = Closure::<dyn Fn()>::new(maj);
            let _ =
                fenetre.add_event_listener_with_callback("scroll", rappel.as_ref().unchecked_ref());

            move || {
                if let Some(f) = web_sys::window() {
                    let _ = f.remove_event_listener_with_callback(
                        "scroll",
                        rappel.as_ref().unchecked_ref(),
                    );
                }
                drop(rappel);
            }
        });
    }

    let au_clic = Callback::from(|e: MouseEvent| {
        e.prevent_default();
        let Some(f) = web_sys::window() else { return };
        let cible = e
            .current_target()
            .and_then(|t| t.dyn_into::<HtmlElement>().ok());
        let vers_le_haut = cible
            .and_then(|el| el.get_attribute("data-sens"))
            .map(|s| s == "haut")
            .unwrap_or(true);

        let doux = f
            .match_media("(prefers-reduced-motion: reduce)")
            .ok()
            .flatten()
            .map(|m| m.matches())
            .unwrap_or(false);
        let comportement = if doux {
            web_sys::ScrollBehavior::Auto
        } else {
            web_sys::ScrollBehavior::Smooth
        };

        if vers_le_haut {
            let o = web_sys::ScrollToOptions::new();
            o.set_top(0.0);
            o.set_behavior(comportement);
            f.scroll_to_with_scroll_to_options(&o);
            return;
        }

        // Le prochain titre encore sous la barre des 80 pixels ; à défaut,
        // le dernier — on est alors tout en bas.
        let titres = titres(&f);
        let suivant = titres
            .iter()
            .find(|t| t.get_bounding_client_rect().top() > 80.0)
            .cloned()
            .or_else(|| titres.last().cloned());
        if let Some(t) = suivant {
            let o = web_sys::ScrollIntoViewOptions::new();
            o.set_behavior(comportement);
            t.scroll_into_view_with_scroll_into_view_options(&o);
        }
    });

    html! {
        <a class="fleche-nav" id="fleche" href="#haut" ref={noeud} data-aimant="" data-defile=""
           onclick={au_clic} aria-label={c.chevrons.remonter} data-sens="bas">
            { Sens::Bas.glyphe() }
        </a>
    }
}

/// Les titres de section de la page, dans l'ordre du document.
fn titres(f: &web_sys::Window) -> Vec<Element> {
    let Some(d) = f.document() else {
        return Vec::new();
    };
    let Ok(liste) = d.query_selector_all("main h2[id]") else {
        return Vec::new();
    };
    (0..liste.length())
        .filter_map(|i| liste.item(i).and_then(|n| n.dyn_into::<Element>().ok()))
        .collect()
}

/// Le haut de chaque titre, relativement à l'écran.
fn hauts_des_titres(f: &web_sys::Window) -> Vec<f64> {
    titres(f)
        .iter()
        .map(|t| t.get_bounding_client_rect().top())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ECRAN: f64 = 900.0;
    const PAGE: f64 = 10_000.0;

    #[test]
    fn tout_en_haut_la_fleche_descend() {
        assert_eq!(sens(0.0, ECRAN, PAGE, false), Sens::Bas);
        // Même si l'on vient de remonter : proposer de monter alors qu'on
        // est déjà en haut n'offre rien.
        assert_eq!(sens(0.0, ECRAN, PAGE, true), Sens::Bas);
    }

    #[test]
    fn tout_en_bas_la_fleche_remonte() {
        // Arrivé au pied, descendre n'a plus de sens, quel que soit le geste.
        assert_eq!(sens(PAGE - ECRAN, ECRAN, PAGE, false), Sens::Haut);
        assert_eq!(sens(PAGE - ECRAN, ECRAN, PAGE, true), Sens::Haut);
    }

    #[test]
    fn au_milieu_la_fleche_suit_le_geste() {
        assert_eq!(sens(4000.0, ECRAN, PAGE, true), Sens::Haut);
        assert_eq!(sens(4000.0, ECRAN, PAGE, false), Sens::Bas);
    }

    #[test]
    fn la_section_courante_compte_les_titres_depasses() {
        // Trois titres : deux au-dessus du milieu de l'écran, un en dessous.
        let hauts = [-200.0, 100.0, 800.0];
        assert_eq!(section_courante(&hauts, ECRAN), 2);
        assert_eq!(section_courante(&[], ECRAN), 0);
    }

    #[test]
    fn les_teintes_bouclent_sur_trois() {
        assert_eq!(teinte(0), teinte(3));
        assert_eq!(teinte(1), teinte(4));
        assert_ne!(teinte(0), teinte(1));
    }

    #[test]
    fn les_glyphes_disent_le_sens() {
        assert_eq!(Sens::Haut.glyphe(), "↑");
        assert_eq!(Sens::Bas.glyphe(), "⌄");
    }
}
