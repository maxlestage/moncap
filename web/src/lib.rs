//! Le front web de MonCap GPS, en Rust de bout en bout.
//!
//! Deux cibles, une base de code, comme la vitrine :
//!   - `app` compile en WebAssembly et tourne dans le navigateur ;
//!   - `construit` compile en natif et fabrique `dist/`.
//!
//! L'interface est écrite avec `active`, le front de la famille Paquets :
//! des fonctions qui construisent des nœuds, et des états fins qui ne
//! mettent à jour que ce qui les lit. Les règles — géographie, carte, temps
//! réel, état — sont des fonctions pures testées en natif ; les composants
//! ne font que les brancher, et se rendent en HTML dans les tests.

pub mod api;
pub mod app;
pub mod carte;
pub mod catalogue;
pub mod compte;
pub mod espace;
pub mod etat;
pub mod geo;
pub mod localisation;
pub mod navigateur;
pub mod nominatim;
pub mod porte;
pub mod session;
pub mod tableau;
pub mod temps_reel;
pub mod types;
pub mod ui;

pub use app::app;
