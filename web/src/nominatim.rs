//! La recherche d'adresse, par Nominatim (OpenStreetMap).
//!
//! **Une recherche par validation, plus une par frappe.** L'ancien front
//! interrogeait Nominatim à chaque pause de 350 ms dans la saisie : une
//! auto-complétion, que la politique d'usage du serveur public interdit en
//! toutes lettres (« auto-complete search … you must not implement such a
//! service on the client side using the API »), en plus de plafonner à une
//! requête par seconde. Le risque n'était pas théorique : la sanction
//! prévue est le blocage.
//!
//! La recherche part donc sur Entrée ou sur le bouton, jamais plus d'une
//! fois par seconde, et une requête déjà posée n'est pas reposée : la
//! politique demande aussi de garder les résultats.

use std::cell::RefCell;
use std::collections::HashMap;

use gloo_net::http::Request;
use serde::Deserialize;

use crate::api::encode_composant;
use crate::types::{Coord, Lieu};

/// L'intervalle minimal entre deux requêtes, imposé par la politique.
pub const INTERVALLE_MS: f64 = 1000.0;

#[derive(Deserialize)]
struct Resultat {
    display_name: String,
    lat: String,
    lon: String,
}

thread_local! {
    static MEMOIRE: RefCell<HashMap<String, Vec<Lieu>>> = RefCell::new(HashMap::new());
    static DERNIERE: RefCell<Option<f64>> = const { RefCell::new(None) };
}

/// Combien attendre avant d'avoir le droit d'interroger le serveur.
pub fn attente(derniere_ms: Option<f64>, maintenant_ms: f64) -> f64 {
    match derniere_ms {
        Some(t) => (INTERVALLE_MS - (maintenant_ms - t)).max(0.0),
        None => 0.0,
    }
}

/// La clé de mémoire d'une requête : deux saisies qui ne diffèrent que par
/// la casse ou les espaces sont la même question.
pub fn cle(requete: &str) -> String {
    requete
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn url(requete: &str) -> String {
    format!(
        "https://nominatim.openstreetmap.org/search?format=jsonv2&limit=5\
         &accept-language=fr&countrycodes=fr&q={}",
        encode_composant(requete.trim())
    )
}

fn maintenant() -> f64 {
    web_sys::window()
        .and_then(|f| f.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

/// Cherche un lieu, priorité à la France.
pub async fn cherche(requete: &str) -> Result<Vec<Lieu>, String> {
    let k = cle(requete);
    if let Some(deja) = MEMOIRE.with(|m| m.borrow().get(&k).cloned()) {
        return Ok(deja);
    }
    let attendre = attente(DERNIERE.with(|d| *d.borrow()), maintenant());
    if attendre > 0.0 {
        gloo_timers::future::TimeoutFuture::new(attendre.ceil() as u32).await;
    }
    DERNIERE.with(|d| *d.borrow_mut() = Some(maintenant()));

    let reponse = Request::get(&url(requete))
        .send()
        .await
        .map_err(|_| "Recherche impossible : pas de réseau ?".to_string())?;
    if reponse.status() == 429 {
        return Err("Trop de recherches d'un coup — patiente un instant.".into());
    }
    if !reponse.ok() {
        return Err(format!("La recherche a répondu {}.", reponse.status()));
    }
    let bruts: Vec<Resultat> = reponse
        .json()
        .await
        .map_err(|_| "Réponse de recherche illisible.".to_string())?;
    let lieux: Vec<Lieu> = bruts
        .into_iter()
        .filter_map(|r| {
            Some(Lieu {
                nom: r.display_name,
                coord: Coord {
                    lat: r.lat.parse().ok()?,
                    lon: r.lon.parse().ok()?,
                },
            })
        })
        .collect();
    MEMOIRE.with(|m| m.borrow_mut().insert(k, lieux.clone()));
    Ok(lieux)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_requete_par_seconde_au_plus() {
        assert_eq!(attente(None, 5000.0), 0.0);
        assert_eq!(attente(Some(5000.0), 5400.0), 600.0);
        assert_eq!(attente(Some(5000.0), 7000.0), 0.0);
    }

    #[test]
    fn la_meme_question_n_est_pas_reposee() {
        assert_eq!(cle("  Place de la  Bourse "), cle("place de la bourse"));
    }

    #[test]
    fn l_adresse_est_encodee() {
        let u = url("12 rue d'Ornano, Bordeaux");
        assert!(u.ends_with("&q=12%20rue%20d'Ornano%2C%20Bordeaux"), "{u}");
        assert!(u.starts_with("https://nominatim.openstreetmap.org/search?"));
    }
}
