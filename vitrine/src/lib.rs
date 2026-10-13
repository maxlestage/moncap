//! La vitrine de MonCap GPS, en Rust de bout en bout, avec `active` — le
//! front de la famille Paquets.
//!
//! Deux cibles, une base de code :
//!   - `construit` compile en natif et **rend les neuf pages en HTML** à la
//!     construction : tête complète (titres, Open Graph, JSON-LD), corps
//!     complet (texte, maquettes, liens). Rien n'attend le WebAssembly pour
//!     être lu ;
//!   - `app` compile en WebAssembly et réhydrate les îlots ([`ILOTS`]), puis
//!     démarre le moteur de mouvement.
//!
//! Le texte des trois langues n'existe qu'une fois, dans `contenu.rs`, et ne
//! part plus au navigateur : il est déjà dans le HTML.

use paquets::active::IslandFn;

pub mod app;
pub mod appstrings;
pub mod contenu;
pub mod fleche;
pub mod seo;
pub mod traduire;

pub use app::{vitrine, Page, Props, Ressources};
pub use contenu::{Contenu, Langue};

/// Les îlots de la page — ses seules parties interactives — par nom. Le
/// rendu les marque, le navigateur les réhydrate (`paquets::hydrate`) : une
/// liste, partagée, pour que les deux côtés ne puissent pas diverger.
pub const ILOTS: &[(&str, IslandFn)] = &[
    (fleche::ILOT, fleche::fleche),
    (app::ILOT_ANNEE, app::annee),
];
