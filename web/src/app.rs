//! La racine : la porte ou le tableau de bord, le volet qui passe de l'un
//! à l'autre, et la session qui décide lequel montrer.
//!
//! La session est gardée ici, et seulement ici. Elle s'ouvre à la porte,
//! se renouvelle quand le mot de passe change, et se referme de trois
//! façons : à la demande (le serveur est prévenu d'abord), quand le serveur
//! ne la reconnaît plus (« session expirée »), ou partout à la fois.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gloo_timers::callback::Timeout;
use paquets::active::prelude::*;

use crate::api::{self, Erreur};
use crate::catalogue::Ressources;
use crate::navigateur::{self, au_montage, fenetre, lance};
use crate::porte::{self, porte};
use crate::session::{self, Session};
use crate::tableau::{self, tableau};

/// Comment une session se referme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinDeSession {
    /// Le bouton « Déconnexion » : le serveur est prévenu, puis la porte.
    Volontaire,
    /// Le serveur ne reconnaît plus le jeton — expiré, révoqué ailleurs.
    Expiree,
    /// « Déconnecter partout » : le serveur a déjà tout refermé.
    Partout,
}

impl FinDeSession {
    /// Ce que la porte dit en se rouvrant, et si c'est une erreur.
    pub fn avis(self) -> Option<(String, bool)> {
        match self {
            FinDeSession::Volontaire => None,
            FinDeSession::Expiree => Some((api::SESSION_EXPIREE.into(), true)),
            FinDeSession::Partout => Some(("Déconnecté de tous les appareils.".into(), false)),
        }
    }
}

/// Le volet qui couvre l'écran pendant qu'on change de vue — la transition
/// de page des sites qui bougent, plutôt qu'un écran qui en remplace un
/// autre d'un coup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Volet {
    Repos,
    Monte,
    Part,
}

/// La durée de chaque moitié du volet, en millisecondes. Celle de la
/// feuille de style : les deux doivent bouger ensemble.
const MONTEE_MS: u32 = 560;
const DEPART_MS: u32 = 720;
/// Le serveur a ce temps pour accuser la déconnexion ; passé ce délai, la
/// porte se rouvre quand même — une déconnexion ne reste jamais coincée.
const ATTENTE_DECONNEXION_MS: u32 = 2500;

/// L'application. `depart` est la session lue dans le stockage par le point
/// d'entrée — fournie ici pour que le rendu de test n'ait pas besoin de
/// navigateur.
pub fn app(ressources: Rc<Ressources>, depart: Option<Session>) -> Node {
    let session = use_state(depart.clone());
    let avis = use_state(None::<(String, bool)>);
    let volet = use_state(Volet::Repos);
    let minuteurs: Rc<RefCell<Vec<Timeout>>> = Rc::default();
    // Une fermeture déjà en route : les suivantes — plusieurs requêtes
    // refusées d'un coup — n'en rajoutent pas.
    let en_sortie = Rc::new(Cell::new(false));

    let change: Rc<dyn Fn(Option<Session>)> = Rc::new(move |nouvelle: Option<Session>| {
        if !navigateur::mouvement_permis() {
            session.set(nouvelle);
            return;
        }
        volet.set(Volet::Monte);
        let bascule = Timeout::new(MONTEE_MS, move || {
            session.set(nouvelle);
            volet.set(Volet::Part);
            if let Some(f) = fenetre() {
                f.scroll_to_with_x_and_y(0.0, 0.0);
            }
        });
        let repos = Timeout::new(MONTEE_MS + DEPART_MS, move || volet.set(Volet::Repos));
        *minuteurs.borrow_mut() = vec![bascule, repos];
    });

    let fin: Rc<dyn Fn(FinDeSession)> = {
        let change = change.clone();
        let en_sortie = en_sortie.clone();
        Rc::new(move |raison: FinDeSession| {
            let Some(courante) = untrack(|| session.get()) else {
                return;
            };
            if en_sortie.replace(true) {
                return;
            }
            let change = change.clone();
            let termine = move || {
                session::ferme();
                avis.set(raison.avis());
                change(None);
            };
            if raison != FinDeSession::Volontaire {
                termine();
                return;
            }
            // Le serveur révoque la session avant qu'on l'oublie ici — au
            // mieux : injoignable ou lent, il n'empêche pas de sortir.
            let fait = Rc::new(Cell::new(false));
            let termine = Rc::new(termine);
            {
                let (fait, termine) = (fait.clone(), termine.clone());
                lance(async move {
                    let _ = api::deconnexion(&courante.jeton).await;
                    if !fait.replace(true) {
                        termine();
                    }
                });
            }
            Timeout::new(ATTENTE_DECONNEXION_MS, move || {
                if !fait.replace(true) {
                    termine();
                }
            })
            .forget();
        })
    };

    let on_session: Rc<dyn Fn(Session)> = {
        let change = change.clone();
        let en_sortie = en_sortie.clone();
        Rc::new(move |s: Session| {
            session::ouvre(&s);
            en_sortie.set(false);
            avis.set(None);
            change(Some(s));
        })
    };

    let renouvelle: Rc<dyn Fn(Session)> = Rc::new(move |s: Session| {
        session::ouvre(&s);
        session.set(Some(s));
    });

    // La session retrouvée dans le stockage est vérifiée auprès du
    // serveur : refusée, la porte se rouvre en le disant. Un serveur
    // injoignable ne déconnecte personne — l'écran reste utilisable, et la
    // prochaine requête refusée refermera la session s'il le faut.
    if let Some(s) = depart {
        let fin = fin.clone();
        au_montage(move || {
            lance(async move {
                if let Err(Erreur::NonAutorise) = api::moi(&s.jeton).await {
                    // Toujours la même session ? Entre-temps, on a pu se
                    // déconnecter et se reconnecter.
                    let meme = untrack(|| session.with(|c| c.as_ref() == Some(&s)));
                    if meme {
                        fin(FinDeSession::Expiree);
                    }
                }
            });
        });
    }

    let connecte = memo(move || session.with(Option::is_some));
    let vue = fragment_dyn({
        let ressources = ressources.clone();
        move || {
            let c = connecte.get();
            untrack(|| {
                vec![if c {
                    tableau(tableau::Proprietes {
                        session,
                        ressources: ressources.clone(),
                        fin: fin.clone(),
                        renouvelle: renouvelle.clone(),
                    })
                } else {
                    let avis = avis.get();
                    porte(porte::Proprietes {
                        on_session: on_session.clone(),
                        avis,
                    })
                }]
            })
        }
    });

    fragment([
        vue,
        div()
            .class("volet")
            .attr_dyn("data-etat", move || {
                match volet.get() {
                    Volet::Repos => "repos",
                    Volet::Monte => "monte",
                    Volet::Part => "part",
                }
                .to_string()
            })
            .attr("aria-hidden", "true")
            .child(
                img()
                    .attr("src", ressources.logo.clone())
                    .attr("width", "72")
                    .attr("height", "72")
                    .attr("alt", ""),
            )
            .into(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ressources() -> Rc<Ressources> {
        Rc::new(
            serde_json::from_str(r#"{"logo":"/web/logo.webp","avatars":{"green":"/web/g.webp"}}"#)
                .unwrap(),
        )
    }

    #[test]
    fn sans_session_la_porte() {
        let html = app(ressources(), None).render();
        assert!(html.contains(r#"class="porte""#), "{html}");
        assert!(!html.contains(r#"class="tableau""#));
        assert!(html.contains(
            r#"<div class="volet" data-etat="repos" aria-hidden="true"><img src="/web/logo.webp""#
        ));
    }

    #[test]
    fn avec_une_session_le_tableau() {
        let s = Session {
            jeton: "j.w.t".into(),
            utilisateur: "toi@exemple.fr".into(),
        };
        let html = app(ressources(), Some(s)).render();
        assert!(html.contains(r#"class="tableau""#), "{html}");
        assert!(html.contains("toi@exemple.fr"));
        assert!(html.contains("Déconnexion"));
        assert!(html.contains("Mon compte"));
        assert!(html.contains("/positions.gpx?token=j.w.t"));
    }

    #[test]
    fn chaque_fin_a_son_mot() {
        assert_eq!(FinDeSession::Volontaire.avis(), None);
        assert_eq!(
            FinDeSession::Expiree.avis(),
            Some((api::SESSION_EXPIREE.to_string(), true))
        );
        assert!(!FinDeSession::Partout.avis().unwrap().1);
    }
}
