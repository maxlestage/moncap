//! La racine : la porte ou le tableau de bord, et le volet qui passe de
//! l'un à l'autre.

use std::rc::Rc;

use gloo_timers::callback::Timeout;
use mouvement::{Moteur, Reglages};
use yew::prelude::*;

use crate::catalogue::Ressources;
use crate::porte::Porte;
use crate::session::{self, Session};
use crate::tableau::Tableau;

#[derive(Properties, PartialEq)]
pub struct Props {
    pub ressources: Rc<Ressources>,
    /// La session de départ. Lue dans le stockage par le point d'entrée,
    /// fournie ici pour que le rendu de test n'ait pas besoin de navigateur.
    #[prop_or_default]
    pub session: Option<Session>,
}

/// Le volet qui couvre l'écran pendant qu'on change de vue — la transition
/// de page des sites qui bougent, plutôt qu'un écran qui en remplace un
/// autre d'un coup.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Volet {
    Repos,
    Monte,
    Part,
}

/// La durée de chaque moitié du volet, en millisecondes. Celle de la
/// feuille de style : les deux doivent bouger ensemble.
const MONTEE_MS: u32 = 560;
const DEPART_MS: u32 = 720;

#[function_component]
pub fn App(p: &Props) -> Html {
    let session = use_state(|| p.session.clone());
    let volet = use_state_eq(|| Volet::Repos);
    let minuteurs = use_mut_ref(Vec::<Timeout>::new);
    let moteur = use_mut_ref(|| None::<Moteur>);

    {
        let moteur = moteur.clone();
        use_effect_with((), move |_| {
            *moteur.borrow_mut() = Moteur::demarre(Reglages::default());
            mouvement::leve_rideau();
            move || {
                moteur.borrow_mut().take();
            }
        });
    }

    let change = {
        let session = session.clone();
        let volet = volet.clone();
        let minuteurs = minuteurs.clone();
        Callback::from(move |nouvelle: Option<Session>| {
            if !mouvement::mouvement_permis() {
                session.set(nouvelle);
                return;
            }
            volet.set(Volet::Monte);
            let s = session.clone();
            let v = volet.clone();
            let bascule = Timeout::new(MONTEE_MS, move || {
                s.set(nouvelle);
                v.set(Volet::Part);
                if let Some(f) = web_sys::window() {
                    f.scroll_to_with_x_and_y(0.0, 0.0);
                }
            });
            let v = volet.clone();
            let repos = Timeout::new(MONTEE_MS + DEPART_MS, move || v.set(Volet::Repos));
            *minuteurs.borrow_mut() = vec![bascule, repos];
        })
    };

    let on_session = {
        let change = change.clone();
        Callback::from(move |s: Session| change.emit(Some(s)))
    };
    let on_deconnexion = Callback::from(move |_| {
        session::ferme();
        change.emit(None);
    });

    let etat_volet = match *volet {
        Volet::Repos => "repos",
        Volet::Monte => "monte",
        Volet::Part => "part",
    };

    html! {
        <ContextProvider<Rc<Ressources>> context={p.ressources.clone()}>
            if let Some(s) = (*session).clone() {
                <Tableau key="tableau" session={s} on_deconnexion={on_deconnexion} />
            } else {
                <Porte key="porte" on_session={on_session} />
            }
            <div class="volet" data-etat={etat_volet} aria-hidden="true">
                <img src={p.ressources.logo.clone()} width="72" height="72" alt="" />
            </div>
        </ContextProvider<Rc<Ressources>>>
    }
}
