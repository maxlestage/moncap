//! La vitrine de MonCap GPS, en Rust de bout en bout.
//!
//! Deux cibles, une base de code :
//!   - `app` compile en WebAssembly et rend la page dans le navigateur ;
//!   - `construit` compile en natif et fabrique les coquilles HTML (titres,
//!     Open Graph, JSON-LD, repli sans script) à la construction.
//!
//! Le texte des trois langues n'existe donc qu'une fois, et le compilateur
//! garantit que les deux usages voient exactement le même.

pub mod app;
pub mod appstrings;
pub mod contenu;
pub mod fleche;
pub mod seo;
pub mod traduire;

pub use app::App;
pub use contenu::{Contenu, Langue};
