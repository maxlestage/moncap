//! Le temps réel : le WebSocket `/ws` du serveur.
//!
//! Deux événements que l'ancien front ignorait sont enfin lus :
//!
//! - `hello` porte l'identifiant de notre propre connexion. Le serveur
//!   diffuse notre position à tout le monde, nous compris : sans cet
//!   identifiant, on s'affichait deux fois, son point bleu et l'écho de sa
//!   voiture juste dessous.
//! - `alert_gone` annonce un signalement infirmé par les votes. Il restait
//!   affiché jusqu'à son expiration, une demi-heure plus tard, alors que
//!   tout le monde l'avait déclaré disparu.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gloo_timers::callback::Timeout;
use serde::{Deserialize, Serialize};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};
use yew::Callback;

use crate::types::Alerte;

/// Ce que le serveur diffuse (`ServerEvent` dans `src/main.rs`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Evenement {
    Hello {
        id: u64,
    },
    PositionsChanged,
    Live {
        id: u64,
        lat: f64,
        lon: f64,
        #[serde(default)]
        label: String,
        #[serde(default)]
        avatar: String,
    },
    LiveGone {
        id: u64,
    },
    Alert(Alerte),
    AlertGone {
        id: i64,
    },
    Alerts {
        alerts: Vec<Alerte>,
    },
    /// Un événement d'une version plus récente du serveur : ignoré, plutôt
    /// que de faire échouer la lecture de tous les autres.
    #[serde(other)]
    Inconnu,
}

/// Lit un message du serveur. `None` pour ce qui n'est pas du JSON valide.
pub fn lis(texte: &str) -> Option<Evenement> {
    serde_json::from_str(texte).ok()
}

/// Ce que le client envoie (`ClientEvent` dans `src/main.rs`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Envoi {
    Live {
        lat: f64,
        lon: f64,
        label: String,
        avatar: String,
    },
    Alert {
        category: String,
        lat: f64,
        lon: f64,
        label: String,
    },
}

/// Le délai avant la reconnexion numéro `tentative` : il double à chaque
/// échec, d'une seconde à quinze au plus. Un serveur qui redémarre est
/// retrouvé vite ; un serveur en panne n'est pas harcelé.
pub fn delai_reconnexion(tentative: u32) -> u32 {
    1000u32.saturating_mul(1 << tentative.min(4)).min(15_000)
}

type Rappel = Closure<dyn FnMut(web_sys::Event)>;

/// Une connexion qui se rétablit seule. La lâcher la ferme pour de bon.
pub struct Connexion {
    interne: Rc<Interne>,
}

struct Interne {
    url: String,
    ws: RefCell<Option<WebSocket>>,
    tentative: Cell<u32>,
    fermee: Cell<bool>,
    minuteur: RefCell<Option<Timeout>>,
    sur_evenement: Callback<Evenement>,
    sur_etat: Callback<bool>,
    rappels: RefCell<Vec<Rappel>>,
}

impl Connexion {
    pub fn ouvre(
        url: String,
        sur_evenement: Callback<Evenement>,
        sur_etat: Callback<bool>,
    ) -> Self {
        let interne = Rc::new(Interne {
            url,
            ws: RefCell::new(None),
            tentative: Cell::new(0),
            fermee: Cell::new(false),
            minuteur: RefCell::new(None),
            sur_evenement,
            sur_etat,
            rappels: RefCell::new(Vec::new()),
        });
        Interne::connecte(&interne);
        Connexion { interne }
    }

    /// Envoie un message, s'il y a une connexion ouverte pour le porter.
    pub fn envoie(&self, e: &Envoi) -> bool {
        let ws = self.interne.ws.borrow();
        let Some(ws) = ws.as_ref() else {
            return false;
        };
        if ws.ready_state() != WebSocket::OPEN {
            return false;
        }
        match serde_json::to_string(e) {
            Ok(texte) => ws.send_with_str(&texte).is_ok(),
            Err(_) => false,
        }
    }
}

impl Drop for Connexion {
    fn drop(&mut self) {
        let i = &self.interne;
        i.fermee.set(true);
        i.minuteur.borrow_mut().take();
        if let Some(ws) = i.ws.borrow_mut().take() {
            ws.set_onopen(None);
            ws.set_onmessage(None);
            ws.set_onerror(None);
            ws.set_onclose(None);
            let _ = ws.close();
        }
        i.rappels.borrow_mut().clear();
    }
}

impl Interne {
    fn connecte(moi: &Rc<Self>) {
        if moi.fermee.get() {
            return;
        }
        let Ok(ws) = WebSocket::new(&moi.url) else {
            Self::reessaie(moi);
            return;
        };
        let faible: Weak<Self> = Rc::downgrade(moi);

        let f = faible.clone();
        let ouvert = Rappel::new(move |_| {
            if let Some(i) = f.upgrade() {
                i.tentative.set(0);
                i.sur_etat.emit(true);
            }
        });
        let f = faible.clone();
        let message = Rappel::new(move |e: web_sys::Event| {
            let Some(i) = f.upgrade() else { return };
            let Some(texte) = e
                .dyn_ref::<MessageEvent>()
                .and_then(|m| m.data().as_string())
            else {
                return;
            };
            if let Some(ev) = lis(&texte) {
                i.sur_evenement.emit(ev);
            }
        });
        let ws_erreur = ws.clone();
        let erreur = Rappel::new(move |_| {
            let _ = ws_erreur.close();
        });
        let f = faible;
        let ferme = Rappel::new(move |_| {
            if let Some(i) = f.upgrade() {
                i.sur_etat.emit(false);
                Self::reessaie(&i);
            }
        });

        ws.set_onopen(Some(ouvert.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
        ws.set_onerror(Some(erreur.as_ref().unchecked_ref()));
        ws.set_onclose(Some(ferme.as_ref().unchecked_ref()));
        // Les rappels de la connexion précédente partent avec elle.
        *moi.rappels.borrow_mut() = vec![ouvert, message, erreur, ferme];
        *moi.ws.borrow_mut() = Some(ws);
    }

    fn reessaie(moi: &Rc<Self>) {
        if moi.fermee.get() {
            return;
        }
        let n = moi.tentative.get();
        moi.tentative.set(n.saturating_add(1));
        let faible = Rc::downgrade(moi);
        let minuteur = Timeout::new(delai_reconnexion(n), move || {
            if let Some(i) = faible.upgrade() {
                Self::connecte(&i);
            }
        });
        *moi.minuteur.borrow_mut() = Some(minuteur);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_reconnexion_ralentit_puis_plafonne() {
        assert_eq!(delai_reconnexion(0), 1000);
        assert_eq!(delai_reconnexion(1), 2000);
        assert_eq!(delai_reconnexion(3), 8000);
        assert_eq!(delai_reconnexion(4), 15_000);
        assert_eq!(delai_reconnexion(40), 15_000);
    }

    /// Les formes exactes que `serde_json` produit côté serveur pour
    /// `ServerEvent` — un écart de nom ici serait un événement perdu.
    #[test]
    fn les_evenements_du_serveur_se_lisent() {
        assert_eq!(
            lis(r#"{"kind":"hello","id":7}"#),
            Some(Evenement::Hello { id: 7 })
        );
        assert_eq!(
            lis(r#"{"kind":"positions_changed"}"#),
            Some(Evenement::PositionsChanged)
        );
        assert_eq!(
            lis(r#"{"kind":"live","id":2,"lat":45.0,"lon":1.5,"label":"a@b.fr","avatar":"mint"}"#),
            Some(Evenement::Live {
                id: 2,
                lat: 45.0,
                lon: 1.5,
                label: "a@b.fr".into(),
                avatar: "mint".into()
            })
        );
        assert_eq!(
            lis(r#"{"kind":"live_gone","id":2}"#),
            Some(Evenement::LiveGone { id: 2 })
        );
        assert_eq!(
            lis(r#"{"kind":"alert_gone","id":9}"#),
            Some(Evenement::AlertGone { id: 9 })
        );
    }

    /// `Alert(Alert)` est une variante « nouveau type » : serde aplatit les
    /// champs du signalement à côté de `kind`.
    #[test]
    fn un_signalement_arrive_a_plat() {
        let ev = lis(
            r#"{"kind":"alert","id":4,"category":"police","lat":44.8,"lon":-0.57,"label":"","ts":1700000000000,"confirms":2,"denies":0}"#,
        );
        match ev {
            Some(Evenement::Alert(a)) => {
                assert_eq!((a.id, a.category.as_str(), a.confirms), (4, "police", 2));
            }
            autre => panic!("{autre:?}"),
        }
        let ev = lis(r#"{"kind":"alerts","alerts":[]}"#);
        assert_eq!(ev, Some(Evenement::Alerts { alerts: vec![] }));
    }

    #[test]
    fn un_evenement_inconnu_ne_casse_rien() {
        assert_eq!(
            lis(r#"{"kind":"meteo","pluie":true}"#),
            Some(Evenement::Inconnu)
        );
        assert_eq!(lis("pas du json"), None);
    }

    /// Et dans l'autre sens, la forme que `ClientEvent` attend.
    #[test]
    fn les_envois_ont_la_forme_attendue() {
        let e = Envoi::Alert {
            category: "bouchon".into(),
            lat: 1.0,
            lon: 2.0,
            label: "Bouchon".into(),
        };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"kind":"alert","category":"bouchon","lat":1.0,"lon":2.0,"label":"Bouchon"}"#
        );
    }
}
