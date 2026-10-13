//! Les petites pièces d'animation de l'interface.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mouvement::calcul;
use paquets::active::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use crate::navigateur::{self, fenetre};

/// Un texte découpé en mots, chacun derrière son masque — le « split text »
/// des sites qui bougent. Le parent porte `data-mvt="mots"` ; le moteur
/// le révèle et la feuille de style fait monter les mots un à un.
/// `depart` est le rang du premier mot dans la cascade, pour enchaîner deux
/// lignes.
///
/// Le découpage se fait au rendu, pas en retouchant le DOM après coup : le
/// rendu reste maître de ses nœuds, et le test côté serveur voit le même
/// HTML que le navigateur.
pub fn mots(texte: &str, depart: usize) -> Node {
    let mots: Vec<&str> = calcul::mots(texte).collect();
    let n = mots.len();
    let mut rendu = Vec::with_capacity(n * 2);
    for (i, m) in mots.into_iter().enumerate() {
        rendu.push(Node::from(
            span().class("mot").child(
                span()
                    .style(format!("--i:{}", depart + i))
                    .text(m.to_string()),
            ),
        ));
        if i + 1 < n {
            rendu.push(" ".into());
        }
    }
    fragment(rendu)
}

/// Un texte découpé en lettres, chacune derrière le masque de son mot —
/// plus démonstratif que les mots, réservé aux grands titres. Le parent
/// porte `data-mvt="lettres"`, et son nom accessible (`aria-label`) : épelé
/// lettre à lettre, le titre se lirait mal au lecteur d'écran.
pub fn lettres(texte: &str) -> Node {
    let mots: Vec<&str> = calcul::mots(texte).collect();
    let n = mots.len();
    let mut rang = 0usize;
    let mut rendu = Vec::with_capacity(n * 2);
    for (i, m) in mots.into_iter().enumerate() {
        let lettres = m.chars().map(|ch| {
            let j = rang;
            rang += 1;
            span()
                .class("lettre")
                .style(format!("--j:{j}"))
                .text(ch.to_string())
        });
        rendu.push(Node::from(
            span()
                .class("mot")
                .attr("aria-hidden", "true")
                .children(lettres.collect::<Vec<_>>()),
        ));
        if i + 1 < n {
            rendu.push(" ".into());
        }
    }
    fragment(rendu)
}

/// Un libellé qui roule au survol de son lien ou de son bouton : le texte
/// monte et son double le remplace. Le double est caché aux lecteurs
/// d'écran, qui liraient sinon chaque libellé deux fois.
pub fn roule(texte: &'static str) -> Node {
    span()
        .class("roule")
        .child(span().text(texte))
        .child(
            span()
                .class("roule-double")
                .attr("aria-hidden", "true")
                .text(texte),
        )
        .into()
}

type Boucle = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

/// Une animation en cours, arrêtée quand on la lâche.
pub struct Anime {
    raf: Rc<RefCell<Option<i32>>>,
    _boucle: Boucle,
}

impl Drop for Anime {
    fn drop(&mut self) {
        if let (Some(id), Some(f)) = (self.raf.borrow_mut().take(), fenetre()) {
            let _ = f.cancel_animation_frame(id);
        }
    }
}

/// Appelle `pas` à chaque image avec l'avancement (0 → 1), pendant
/// `duree_ms`. La dernière image reçoit toujours exactement 1.
pub fn anime(duree_ms: f64, mut pas: impl FnMut(f64) + 'static) -> Anime {
    let raf: Rc<RefCell<Option<i32>>> = Rc::new(RefCell::new(None));
    let boucle: Boucle = Rc::new(RefCell::new(None));
    let debut: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));

    let raf_c = raf.clone();
    let boucle_c = boucle.clone();
    *boucle.borrow_mut() = Some(Closure::new(move |t: f64| {
        let t0 = *debut.borrow_mut().get_or_insert(t);
        let avance = if duree_ms <= 0.0 {
            1.0
        } else {
            ((t - t0) / duree_ms).clamp(0.0, 1.0)
        };
        pas(avance);
        *raf_c.borrow_mut() = None;
        if avance < 1.0 {
            if let (Some(f), Some(b)) = (fenetre(), boucle_c.borrow().as_ref()) {
                *raf_c.borrow_mut() = f.request_animation_frame(b.as_ref().unchecked_ref()).ok();
            }
        }
    }));
    if let (Some(f), Some(b)) = (fenetre(), boucle.borrow().as_ref()) {
        *raf.borrow_mut() = f.request_animation_frame(b.as_ref().unchecked_ref()).ok();
    }
    Anime {
        raf,
        _boucle: boucle,
    }
}

/// Un nombre qui compte jusqu'à sa valeur, et recompte quand elle change —
/// depuis l'ancienne, pas depuis zéro. `valeur` lit les états dont le
/// nombre dépend : le compteur les suit.
pub fn compteur(
    valeur: impl Fn() -> f64 + 'static,
    decimales: usize,
    suffixe: &'static str,
) -> Node {
    let affiche = use_state(0.0f64);
    let courant = Rc::new(Cell::new(0.0f64));
    let animation: Rc<RefCell<Option<Anime>>> = Rc::default();
    {
        let animation = animation.clone();
        on_cleanup(move || drop(animation.borrow_mut().take()));
    }
    effect(move || {
        let cible = valeur();
        let de = courant.get();
        if !navigateur::mouvement_permis() || de == cible {
            courant.set(cible);
            untrack(|| affiche.set(cible));
            return;
        }
        let courant = courant.clone();
        // L'ancienne animation part avec son remplacement.
        *animation.borrow_mut() = Some(anime(1200.0, move |t| {
            let v = calcul::lerp(de, cible, calcul::expo_sortie(t));
            courant.set(v);
            if affiche.is_alive() {
                affiche.set(v);
            }
        }));
    });
    span()
        .class("compteur")
        .text_dyn(move || format!("{:.*}{}", decimales, affiche.get(), suffixe))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_mots_montent_chacun_derriere_son_masque() {
        assert_eq!(
            mots("Bienvenue à bord.", 2).render(),
            r#"<span class="mot"><span style="--i:2">Bienvenue</span></span> <span class="mot"><span style="--i:3">à</span></span> <span class="mot"><span style="--i:4">bord.</span></span>"#
        );
    }

    #[test]
    fn les_lettres_se_comptent_d_un_mot_a_l_autre() {
        let html = lettres("Où va").render();
        assert!(html.starts_with(r#"<span class="mot" aria-hidden="true"><span class="lettre" style="--j:0">O</span>"#), "{html}");
        assert!(
            html.contains(r#"<span class="lettre" style="--j:3">a</span>"#),
            "{html}"
        );
    }

    #[test]
    fn le_libelle_qui_roule_est_lu_une_fois() {
        assert_eq!(
            roule("Le site").render(),
            r#"<span class="roule"><span>Le site</span><span class="roule-double" aria-hidden="true">Le site</span></span>"#
        );
    }

    #[test]
    fn le_compteur_part_de_zero() {
        let html = compteur(|| 12.5, 1, " km").render();
        assert_eq!(html, r#"<span class="compteur"><!--#-->0.0 km</span>"#);
    }
}
