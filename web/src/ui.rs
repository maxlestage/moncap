//! Les petites pièces d'animation de l'interface.

use std::cell::RefCell;
use std::rc::Rc;

use mouvement::calcul;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct MotsProps {
    pub texte: AttrValue,
    /// Le rang du premier mot dans la cascade, pour enchaîner deux lignes.
    #[prop_or(0)]
    pub depart: usize,
}

/// Un texte découpé en mots, chacun derrière son masque — le « split text »
/// des sites qui bougent. Le parent porte `data-mvt="mots"` ; le moteur
/// le révèle et la feuille de style fait monter les mots un à un.
///
/// Le découpage se fait au rendu, pas en retouchant le DOM après coup : le
/// rendu reste maître de ses nœuds, et le test côté serveur voit le même
/// HTML que le navigateur.
#[function_component]
pub fn Mots(p: &MotsProps) -> Html {
    let mots: Vec<String> = calcul::mots(&p.texte).map(str::to_string).collect();
    let n = mots.len();
    mots.into_iter()
        .enumerate()
        .map(|(i, m)| {
            html! {
                <>
                    <span class="mot"><span style={format!("--i:{}", p.depart + i)}>{ m }</span></span>
                    if i + 1 < n { { " " } }
                </>
            }
        })
        .collect()
}

type Boucle = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

/// Une animation en cours, arrêtée quand on la lâche.
pub struct Anime {
    raf: Rc<RefCell<Option<i32>>>,
    _boucle: Boucle,
}

impl Drop for Anime {
    fn drop(&mut self) {
        if let (Some(id), Some(f)) = (self.raf.borrow_mut().take(), web_sys::window()) {
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
            if let (Some(f), Some(b)) = (web_sys::window(), boucle_c.borrow().as_ref()) {
                *raf_c.borrow_mut() = f.request_animation_frame(b.as_ref().unchecked_ref()).ok();
            }
        }
    }));
    if let (Some(f), Some(b)) = (web_sys::window(), boucle.borrow().as_ref()) {
        *raf.borrow_mut() = f.request_animation_frame(b.as_ref().unchecked_ref()).ok();
    }
    Anime {
        raf,
        _boucle: boucle,
    }
}

#[derive(Properties, PartialEq)]
pub struct CompteurProps {
    pub valeur: f64,
    #[prop_or(0)]
    pub decimales: usize,
    #[prop_or_default]
    pub suffixe: AttrValue,
}

/// Un nombre qui compte jusqu'à sa valeur, et recompte quand elle change —
/// depuis l'ancienne, pas depuis zéro.
#[function_component]
pub fn Compteur(p: &CompteurProps) -> Html {
    let affiche = use_state(|| 0.0f64);
    let courant = use_mut_ref(|| 0.0f64);
    let animation = use_mut_ref(|| None::<Anime>);
    {
        let affiche = affiche.clone();
        let courant = courant.clone();
        let animation = animation.clone();
        let cible = p.valeur;
        use_effect_with(p.valeur.to_bits(), move |_| {
            let de = *courant.borrow();
            if !mouvement::mouvement_permis() || de == cible {
                *courant.borrow_mut() = cible;
                affiche.set(cible);
            } else {
                let courant = courant.clone();
                *animation.borrow_mut() = Some(anime(1200.0, move |t| {
                    let v = calcul::lerp(de, cible, calcul::expo_sortie(t));
                    *courant.borrow_mut() = v;
                    affiche.set(v);
                }));
            }
            move || {
                animation.borrow_mut().take();
            }
        });
    }
    html! {
        <span class="compteur">{ format!("{:.*}{}", p.decimales, *affiche, p.suffixe) }</span>
    }
}
