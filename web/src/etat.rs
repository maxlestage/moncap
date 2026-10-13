//! L'état du tableau de bord, et les règles qui le font évoluer.
//!
//! Un réducteur : chaque changement est une [`Action`], et la fonction qui
//! l'applique est pure. Le tableau le tient dans un `State` d'`active`. Les règles du temps réel — ignorer son propre écho,
//! oublier une voiture muette, retirer un signalement infirmé — se testent
//! donc sans serveur ni navigateur.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::temps_reel::Evenement;
use crate::types::{Alerte, Coord, Itineraire, Position, Stats, Voiture};

/// Une voiture sans nouvelles depuis quinze secondes a quitté la route —
/// ou le réseau. Elle disparaît de la carte.
pub const VOITURE_MUETTE_MS: f64 = 15_000.0;

/// Le serveur garde un signalement deux heures au plus, même très
/// confirmé (`ALERT_MAX_SECS`). Il le purge sans prévenir personne : passé
/// ce délai, le client l'oublie de lui-même.
pub const ALERTE_MAX_MS: f64 = 2.0 * 60.0 * 60.0 * 1000.0;

/// Un message passager : une erreur, ou la confirmation d'une action.
#[derive(Clone, Debug, PartialEq)]
pub struct Annonce {
    pub numero: u32,
    pub texte: String,
    pub erreur: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Etat {
    pub positions: Rc<Vec<Position>>,
    pub stats: Option<Stats>,
    pub voitures: BTreeMap<u64, Voiture>,
    pub alertes: Rc<Vec<Alerte>>,
    pub connecte: bool,
    /// L'identifiant de notre connexion, donné par `hello`.
    pub mon_id: Option<u64>,
    pub moi: Option<Coord>,
    pub partage: bool,
    pub itineraire: Option<(Itineraire, f64)>,
    /// La dernière annonce, numérotée : le même texte deux fois de suite
    /// doit pouvoir se montrer deux fois.
    pub annonce: Option<Annonce>,
    numero_annonce: u32,
    /// Combien de fois les positions ont changé de place : la carte se
    /// recadre quand ce numéro bouge.
    pub cadrages: u32,
}

pub enum Action {
    Donnees {
        positions: Vec<Position>,
        stats: Stats,
    },
    Recu {
        evenement: Evenement,
        maintenant_ms: f64,
    },
    Connexion(bool),
    Moi(Coord),
    Partage(bool),
    Alertes(Vec<Alerte>),
    Itineraire(Option<(Itineraire, f64)>),
    Erreur(String),
    Info(String),
    /// Efface l'annonce de ce numéro — et seulement elle : une annonce
    /// plus récente ne doit pas disparaître avec la précédente.
    EffaceAnnonce(u32),
    Purge {
        maintenant_ms: f64,
    },
}

impl Etat {
    /// Les voitures des autres, dans un ordre stable.
    pub fn voitures(&self) -> Vec<Voiture> {
        self.voitures.values().cloned().collect()
    }
}

/// Deux listes de positions aux mêmes endroits, dans le même ordre ? Un
/// renommage ne déplace rien : la carte n'a pas à se recadrer pour lui.
fn memes_lieux(a: &[Position], b: &[Position]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.id == y.id && x.lat == y.lat && x.lon == y.lon)
}

impl Etat {
    /// Applique une action. Rien de changé : l'état rendu est l'ancien, le
    /// même `Rc` — et rien ne se met à jour à l'écran.
    pub fn reduce(self: Rc<Self>, action: Action) -> Rc<Self> {
        let mut e = (*self).clone();
        match action {
            Action::Donnees { positions, stats } => {
                if !memes_lieux(&e.positions, &positions) {
                    e.cadrages += 1;
                    // L'itinéraire calculé portait sur l'ancienne liste.
                    e.itineraire = None;
                }
                e.positions = Rc::new(positions);
                e.stats = Some(stats);
            }
            Action::Recu {
                evenement,
                maintenant_ms,
            } => applique(&mut e, evenement, maintenant_ms),
            Action::Connexion(c) => e.connecte = c,
            Action::Moi(c) => e.moi = Some(c),
            Action::Partage(p) => e.partage = p,
            Action::Alertes(a) => e.alertes = Rc::new(a),
            Action::Itineraire(i) => e.itineraire = i,
            Action::Erreur(texte) => annonce(&mut e, texte, true),
            Action::Info(texte) => annonce(&mut e, texte, false),
            Action::EffaceAnnonce(n) => {
                if e.annonce.as_ref().is_some_and(|a| a.numero == n) {
                    e.annonce = None;
                }
            }
            Action::Purge { maintenant_ms } => {
                e.voitures
                    .retain(|_, v| maintenant_ms - v.vu_ms < VOITURE_MUETTE_MS);
                if e.alertes
                    .iter()
                    .any(|a| maintenant_ms - a.ts >= ALERTE_MAX_MS)
                {
                    let gardees = e
                        .alertes
                        .iter()
                        .filter(|a| maintenant_ms - a.ts < ALERTE_MAX_MS)
                        .cloned()
                        .collect();
                    e.alertes = Rc::new(gardees);
                }
            }
        }
        // Rien n'a changé : on rend l'ancien état, et rien ne se redessine.
        if e == *self {
            self
        } else {
            Rc::new(e)
        }
    }
}

fn annonce(e: &mut Etat, texte: String, erreur: bool) {
    let numero = e.numero_annonce + 1;
    e.numero_annonce = numero;
    e.annonce = Some(Annonce {
        numero,
        texte,
        erreur,
    });
}

fn applique(e: &mut Etat, ev: Evenement, maintenant_ms: f64) {
    match ev {
        Evenement::Hello { id } => {
            e.mon_id = Some(id);
            e.voitures.remove(&id);
        }
        Evenement::Live {
            id,
            lat,
            lon,
            label,
            avatar,
        } => {
            // Notre propre position nous revient de la diffusion : le point
            // bleu la montre déjà.
            if e.mon_id == Some(id) {
                return;
            }
            e.voitures.insert(
                id,
                Voiture {
                    id,
                    lat,
                    lon,
                    label,
                    avatar,
                    vu_ms: maintenant_ms,
                },
            );
        }
        Evenement::LiveGone { id } => {
            e.voitures.remove(&id);
        }
        Evenement::Alert(a) => {
            let mut liste: Vec<Alerte> =
                e.alertes.iter().filter(|x| x.id != a.id).cloned().collect();
            liste.push(a);
            e.alertes = Rc::new(liste);
        }
        Evenement::AlertGone { id } => {
            if e.alertes.iter().any(|a| a.id == id) {
                e.alertes = Rc::new(e.alertes.iter().filter(|a| a.id != id).cloned().collect());
            }
        }
        Evenement::Alerts { alerts } => e.alertes = Rc::new(alerts),
        // Le rechargement des positions est l'affaire du composant : il
        // demande l'API, ce qu'un réducteur ne fait pas.
        Evenement::PositionsChanged | Evenement::Inconnu => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn etat() -> Rc<Etat> {
        Rc::new(Etat::default())
    }

    fn recu(ev: Evenement, t: f64) -> Action {
        Action::Recu {
            evenement: ev,
            maintenant_ms: t,
        }
    }

    fn live(id: u64) -> Evenement {
        Evenement::Live {
            id,
            lat: 45.0,
            lon: 1.0,
            label: format!("conducteur {id}"),
            avatar: "mint".into(),
        }
    }

    fn alerte(id: i64, ts: f64) -> Alerte {
        Alerte {
            id,
            category: "police".into(),
            lat: 45.0,
            lon: 1.0,
            label: String::new(),
            ts,
            confirms: 0,
            denies: 0,
        }
    }

    fn position(id: i64, lat: f64, label: &str) -> Position {
        Position {
            id,
            lat,
            lon: 1.0,
            label: label.into(),
        }
    }

    fn stats() -> Stats {
        Stats {
            count: 0,
            total_km: 0.0,
            bbox: None,
            centroid: None,
        }
    }

    #[test]
    fn on_ne_se_voit_pas_en_double() {
        let e = etat()
            .reduce(recu(Evenement::Hello { id: 7 }, 0.0))
            .reduce(recu(live(7), 10.0))
            .reduce(recu(live(8), 10.0));
        assert_eq!(e.voitures.keys().copied().collect::<Vec<_>>(), [8]);
    }

    #[test]
    fn une_voiture_muette_disparait() {
        let e = etat().reduce(recu(live(1), 1000.0));
        let e = e.reduce(Action::Purge {
            maintenant_ms: 1000.0 + VOITURE_MUETTE_MS - 1.0,
        });
        assert_eq!(e.voitures.len(), 1, "encore dans les temps");
        let e = e.reduce(Action::Purge {
            maintenant_ms: 1000.0 + VOITURE_MUETTE_MS,
        });
        assert!(e.voitures.is_empty());
    }

    #[test]
    fn une_voiture_partie_est_retiree() {
        let e = etat()
            .reduce(recu(live(1), 0.0))
            .reduce(recu(Evenement::LiveGone { id: 1 }, 0.0));
        assert!(e.voitures.is_empty());
    }

    #[test]
    fn un_signalement_se_met_a_jour_puis_s_en_va() {
        let mut a = alerte(5, 0.0);
        let e = etat().reduce(recu(Evenement::Alert(a.clone()), 0.0));
        a.confirms = 3;
        let e = e.reduce(recu(Evenement::Alert(a), 0.0));
        assert_eq!(e.alertes.len(), 1, "mis à jour, pas dupliqué");
        assert_eq!(e.alertes[0].confirms, 3);
        let e = e.reduce(recu(Evenement::AlertGone { id: 5 }, 0.0));
        assert!(e.alertes.is_empty(), "infirmé : il disparaît tout de suite");
    }

    #[test]
    fn un_vieux_signalement_est_oublie() {
        let e = etat().reduce(Action::Alertes(vec![alerte(1, 0.0), alerte(2, 1000.0)]));
        let e = e.reduce(Action::Purge {
            maintenant_ms: ALERTE_MAX_MS + 500.0,
        });
        assert_eq!(e.alertes.iter().map(|a| a.id).collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn un_renommage_ne_recadre_pas_la_carte() {
        let e = etat().reduce(Action::Donnees {
            positions: vec![position(1, 45.0, "A"), position(2, 46.0, "B")],
            stats: stats(),
        });
        assert_eq!(e.cadrages, 1);
        let e = e.reduce(Action::Donnees {
            positions: vec![position(1, 45.0, "Maison"), position(2, 46.0, "B")],
            stats: stats(),
        });
        assert_eq!(e.cadrages, 1, "même lieux : pas de recadrage");
        let e = e.reduce(Action::Donnees {
            positions: vec![position(1, 45.0, "Maison")],
            stats: stats(),
        });
        assert_eq!(e.cadrages, 2);
    }

    #[test]
    fn rien_de_neuf_rien_a_redessiner() {
        let e = etat();
        let apres = e.clone().reduce(Action::Purge { maintenant_ms: 5.0 });
        assert!(Rc::ptr_eq(&e, &apres));
    }

    #[test]
    fn la_meme_erreur_se_montre_deux_fois() {
        let e = etat()
            .reduce(Action::Erreur("Vote impossible.".into()))
            .reduce(Action::Erreur("Vote impossible.".into()));
        let a = e.annonce.clone().unwrap();
        assert_eq!(
            (a.numero, a.texte.as_str(), a.erreur),
            (2, "Vote impossible.", true)
        );
    }

    #[test]
    fn effacer_une_annonce_ne_retire_pas_la_suivante() {
        let e = etat()
            .reduce(Action::Info("Signalement envoyé.".into()))
            .reduce(Action::Erreur("Vote impossible.".into()));
        // Le minuteur de la première annonce expire après l'arrivée de la
        // seconde : elle reste.
        let e = e.reduce(Action::EffaceAnnonce(1));
        assert_eq!(e.annonce.as_ref().map(|a| a.numero), Some(2));
        let e = e.reduce(Action::EffaceAnnonce(2));
        assert!(e.annonce.is_none());
    }
}
