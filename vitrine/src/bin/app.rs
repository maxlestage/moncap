//! Le point d'entrée du WebAssembly.
//!
//! La page est déjà là : `construit` l'a rendue en HTML, en entier. Il ne
//! reste au navigateur que trois gestes — réhydrater les îlots (la flèche de
//! défilement, l'année du copyright), démarrer le moteur de mouvement, et
//! lever le rideau de chargement.
//!
//! Un seul WebAssembly sert les neuf pages, et il n'embarque aucun texte de
//! la page : il ne sait que ce que les îlots lui disent dans le HTML.

use mouvement::{Moteur, Reglages};

fn main() {
    // Sur une page sans îlot (une coquille réduite, une page d'erreur), rien
    // n'est trouvé et rien n'est cassé : le HTML reste tel qu'il est venu.
    paquets::hydrate(vitrine::ILOTS);

    // Le moteur ne démarre pas quand le système demande moins de
    // mouvement : la page est alors simplement immobile. Démarré, il vit
    // autant que la page — le lâcher l'arrêterait.
    if let Some(moteur) = Moteur::demarre(Reglages::default()) {
        std::mem::forget(moteur);
    }
    mouvement::leve_rideau();
}
