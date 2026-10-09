//! Le mouvement de MonCap GPS, partagé entre la vitrine et le front web.
//!
//! zamocorp.com doit son allure à trois bibliothèques JavaScript : GSAP pour
//! les cascades et les masques, ScrollTrigger pour lier l'animation au
//! défilement, Lenis pour la molette qui glisse. Cette crate en reprend les
//! gestes — et seulement ceux-là — en Rust, dans le WebAssembly que la page
//! charge déjà :
//!
//! - [`calcul`] : les courbes, l'amortissement, les cascades, les compteurs,
//!   en fonctions pures testées sans navigateur ;
//! - [`moteur`] : leur branchement sur le DOM — révélations à l'entrée dans
//!   l'écran, défilement inertiel, progression, magnétisme, rideau.
//!
//! Le moteur ne démarre pas quand le système demande moins de mouvement, et
//! rien ne reste masqué s'il s'arrête : l'état caché n'existe que sous la
//! classe qu'il pose lui-même.

pub mod calcul;
pub mod moteur;

pub use moteur::{leve_rideau, mouvement_permis, Moteur, Reglages};
