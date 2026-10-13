//! La recherche d'adresse.
//!
//! **Deux sources, la plus juste d'abord.**
//!
//! 1. La **Base Adresse Nationale** (Géoplateforme de l'IGN), le géocodeur
//!    officiel des adresses françaises : il connaît chaque numéro de rue,
//!    tolère les fautes de frappe et les compléments, et classe les
//!    résultats par proximité quand on lui donne notre position.
//! 2. **Nominatim** (OpenStreetMap) en repli : les lieux qui ne sont pas des
//!    adresses (un parc, une gare, un monument), et l'étranger.
//!
//! **La saisie est nettoyée avant de partir.** Nominatim exige que chaque
//! mot de la requête corresponde à quelque chose : « 124 Rue Billaudel,
//! APT 03 » ne trouvait rien à cause de « APT 03 ». Les compléments
//! d'adresse (appartement, bâtiment, étage, escalier, porte…) sont retirés,
//! et si rien ne sort encore, on retente en raccourcissant l'adresse par la
//! fin (« rue, ville, pays » → « rue, ville » → « rue »).
//!
//! **Une recherche par validation, jamais par frappe** : la politique
//! d'usage de Nominatim interdit l'auto-complétion côté client et plafonne
//! à une requête par seconde ; une requête déjà posée n'est pas reposée.

use std::cell::RefCell;
use std::collections::HashMap;

use gloo_net::http::Request;
use serde::Deserialize;

use crate::api::encode_composant;
use crate::types::{Coord, Lieu};

/// L'intervalle minimal entre deux requêtes Nominatim, imposé par la politique.
pub const INTERVALLE_MS: f64 = 1000.0;

/// Combien de variantes raccourcies essayer au plus auprès de Nominatim.
const VARIANTES_MAX: usize = 3;

thread_local! {
    static MEMOIRE: RefCell<HashMap<String, Vec<Lieu>>> = RefCell::new(HashMap::new());
    static DERNIERE: RefCell<Option<f64>> = const { RefCell::new(None) };
}

/// Combien attendre avant d'avoir le droit d'interroger Nominatim.
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

// ---------------------------------------------------------------------------
// Le nettoyage de la saisie.
// ---------------------------------------------------------------------------

/// Un mot en minuscules, sans accents ni ponctuation (« n°12 » → « n12 »).
fn forme(mot: &str) -> String {
    mot.chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'À' | 'Â' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'î' | 'ï' | 'Î' | 'Ï' => 'i',
            'ô' | 'ö' | 'Ô' | 'Ö' => 'o',
            'ù' | 'û' | 'ü' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

/// Les mots qui ouvrent un complément d'adresse : ils partent, avec la
/// désignation qui les suit (« APT 03 », « bât. B », « 3e étage »).
const COMPLEMENTS: &[&str] = &[
    "apt",
    "appt",
    "app",
    "apart",
    "appart",
    "appartement",
    "appartements",
    "apartment",
    "bat",
    "batiment",
    "bt",
    "bloc",
    "etage",
    "etg",
    "esc",
    "escalier",
    "porte",
    "logement",
    "lgt",
    "chambre",
    "ch",
    "boite",
    "bal",
    "digicode",
    "code",
    "interphone",
    "suite",
    "unit",
    "flat",
    "floor",
    "rdc",
];

/// Les mots qui partent seuls (rien à voir avec un lieu sur une carte).
const PARASITES: &[&str] = &["cedex", "bp", "cs", "tsa"];

/// Une désignation courte : « 03 », « B », « 2e », « 12B », « n°4 ».
fn designation(mot: &str) -> bool {
    let f = forme(mot);
    let f = f.trim_start_matches('n');
    !f.is_empty()
        && f.chars().count() <= 4
        && f.chars().all(|c| c.is_ascii_alphanumeric())
        && (f.chars().any(|c| c.is_ascii_digit()) || f.chars().count() == 1)
}

/// Un ordinal d'étage : « 3e », « 3ème », « 1er ».
fn ordinal(mot: &str) -> bool {
    let f = forme(mot);
    let chiffres = f.chars().take_while(char::is_ascii_digit).count();
    chiffres > 0 && matches!(&f[chiffres..], "e" | "eme" | "er" | "ere" | "nd" | "th")
}

/// Retire les compléments d'adresse d'un segment (entre deux virgules).
fn nettoie_segment(segment: &str) -> String {
    let mots: Vec<&str> = segment.split_whitespace().collect();
    let mut garde = Vec::with_capacity(mots.len());
    let mut i = 0;
    while i < mots.len() {
        let f = forme(mots[i]);
        if mots[i].starts_with('#') {
            i += 1;
            continue;
        }
        if COMPLEMENTS.contains(&f.as_str()) {
            // « 3e étage » : l'ordinal déjà gardé part aussi.
            if (f.starts_with("etage") || f == "etg" || f == "floor")
                && garde.last().is_some_and(|m: &&str| ordinal(m))
            {
                garde.pop();
            }
            i += 1;
            while i < mots.len()
                && (designation(mots[i]) || matches!(forme(mots[i]).as_str(), "n" | "no" | "num"))
            {
                i += 1;
            }
            continue;
        }
        if PARASITES.contains(&f.as_str()) {
            i += 1;
            while i < mots.len() && designation(mots[i]) && forme(mots[i]).len() > 1 {
                i += 1;
            }
            continue;
        }
        garde.push(mots[i]);
        i += 1;
    }
    garde.join(" ")
}

/// La saisie débarrassée de ses compléments d'adresse, segments vides ôtés.
pub fn nettoie(saisie: &str) -> String {
    saisie
        .split([',', ';', '\n'])
        .map(nettoie_segment)
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Les requêtes à essayer dans l'ordre : l'adresse nettoyée, puis raccourcie
/// par la fin, segment par segment, sans doublon.
pub fn variantes(saisie: &str) -> Vec<String> {
    let propre = nettoie(saisie);
    let segments: Vec<&str> = propre.split(", ").collect();
    let mut v: Vec<String> = Vec::new();
    for n in (1..=segments.len()).rev() {
        let q = segments[..n].join(", ");
        if q.chars().count() >= 3 && !v.contains(&q) {
            v.push(q);
        }
    }
    v.truncate(VARIANTES_MAX);
    v
}

// ---------------------------------------------------------------------------
// Base Adresse Nationale (Géoplateforme).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Ban {
    #[serde(default)]
    features: Vec<BanLieu>,
}

#[derive(Deserialize)]
struct BanLieu {
    geometry: BanGeometrie,
    properties: BanProprietes,
}

#[derive(Deserialize)]
struct BanGeometrie {
    /// `[lon, lat]`.
    coordinates: Vec<f64>,
}

#[derive(Deserialize)]
struct BanProprietes {
    #[serde(default)]
    label: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    postcode: String,
    #[serde(default)]
    city: String,
    #[serde(default)]
    score: f64,
}

/// En dessous, la Base Adresse devine plus qu'elle ne trouve.
const SCORE_MIN: f64 = 0.4;

pub fn url_ban(requete: &str, pres_de: Option<Coord>) -> String {
    let mut u = format!(
        "https://data.geopf.fr/geocodage/search?index=address&limit=5&q={}",
        encode_composant(requete.trim())
    );
    if let Some(c) = pres_de {
        u.push_str(&format!("&lat={:.5}&lon={:.5}", c.lat, c.lon));
    }
    u
}

/// Les lieux d'une réponse de la Base Adresse.
fn lieux_ban(json: &str) -> Vec<Lieu> {
    let Ok(r) = serde_json::from_str::<Ban>(json) else {
        return Vec::new();
    };
    r.features
        .into_iter()
        .filter(|f| f.properties.score >= SCORE_MIN)
        .filter_map(|f| {
            let (&lon, &lat) = (
                f.geometry.coordinates.first()?,
                f.geometry.coordinates.get(1)?,
            );
            let p = f.properties;
            // « 124 Rue Billaudel, 33100 Bordeaux » : la virgule sépare la
            // rue de la ville, comme le nom court l'attend.
            let nom = if !p.name.is_empty() && !p.city.is_empty() {
                format!("{}, {} {}", p.name, p.postcode, p.city).replace("  ", " ")
            } else {
                p.label
            };
            Some(Lieu {
                nom,
                coord: Coord { lat, lon },
            })
        })
        .collect()
}

async fn cherche_ban(requete: &str, pres_de: Option<Coord>) -> Option<Vec<Lieu>> {
    let reponse = Request::get(&url_ban(requete, pres_de)).send().await.ok()?;
    if !reponse.ok() {
        return None;
    }
    Some(lieux_ban(&reponse.text().await.ok()?))
}

// ---------------------------------------------------------------------------
// Nominatim (OpenStreetMap).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Resultat {
    display_name: String,
    lat: String,
    lon: String,
}

pub fn url(requete: &str, pres_de: Option<Coord>) -> String {
    let mut u = format!(
        "https://nominatim.openstreetmap.org/search?format=jsonv2&limit=5\
         &accept-language=fr&countrycodes=fr&q={}",
        encode_composant(requete.trim())
    );
    if let Some(c) = pres_de {
        // Une boîte d'environ 100 km autour de nous : elle oriente le
        // classement sans exclure le reste (bounded=0).
        u.push_str(&format!(
            "&viewbox={:.4},{:.4},{:.4},{:.4}&bounded=0",
            c.lon - 0.7,
            c.lat + 0.5,
            c.lon + 0.7,
            c.lat - 0.5
        ));
    }
    u
}

fn lieux_nominatim(json: &str) -> Vec<Lieu> {
    serde_json::from_str::<Vec<Resultat>>(json)
        .unwrap_or_default()
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
        .collect()
}

enum Echec {
    Reseau,
    TropVite,
    Statut(u16),
}

async fn cherche_nominatim(requete: &str, pres_de: Option<Coord>) -> Result<Vec<Lieu>, Echec> {
    let attendre = attente(
        DERNIERE.with(|d| *d.borrow()),
        crate::navigateur::horloge_ms(),
    );
    if attendre > 0.0 {
        gloo_timers::future::TimeoutFuture::new(attendre.ceil() as u32).await;
    }
    DERNIERE.with(|d| *d.borrow_mut() = Some(crate::navigateur::horloge_ms()));
    let reponse = Request::get(&url(requete, pres_de))
        .send()
        .await
        .map_err(|_| Echec::Reseau)?;
    if reponse.status() == 429 {
        return Err(Echec::TropVite);
    }
    if !reponse.ok() {
        return Err(Echec::Statut(reponse.status()));
    }
    Ok(lieux_nominatim(
        &reponse.text().await.map_err(|_| Echec::Reseau)?,
    ))
}

// ---------------------------------------------------------------------------
// La recherche.
// ---------------------------------------------------------------------------

/// Cherche un lieu : la Base Adresse d'abord, Nominatim ensuite, sur la
/// saisie nettoyée puis raccourcie. `pres_de` (notre position) classe les
/// résultats par proximité.
pub async fn cherche(saisie: &str, pres_de: Option<Coord>) -> Result<Vec<Lieu>, String> {
    let k = cle(saisie);
    if let Some(deja) = MEMOIRE.with(|m| m.borrow().get(&k).cloned()) {
        return Ok(deja);
    }
    let essais = variantes(saisie);
    let mut echec = None;
    let mut lieux = Vec::new();
    'cherche: for q in &essais {
        if let Some(l) = cherche_ban(q, pres_de).await {
            if !l.is_empty() {
                lieux = l;
                break 'cherche;
            }
        }
        match cherche_nominatim(q, pres_de).await {
            Ok(l) if !l.is_empty() => {
                lieux = l;
                break 'cherche;
            }
            Ok(_) => {}
            Err(e) => {
                echec = Some(e);
                if matches!(echec, Some(Echec::TropVite)) {
                    break 'cherche;
                }
            }
        }
    }
    if let (true, Some(e)) = (lieux.is_empty(), echec) {
        return Err(match e {
            Echec::Reseau => "Recherche impossible : pas de réseau ?".to_string(),
            Echec::TropVite => "Trop de recherches d'un coup — patiente un instant.".into(),
            Echec::Statut(s) => format!("La recherche a répondu {s}."),
        });
    }
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
        let u = url("12 rue d'Ornano, Bordeaux", None);
        assert!(u.ends_with("&q=12%20rue%20d'Ornano%2C%20Bordeaux"), "{u}");
        assert!(u.starts_with("https://nominatim.openstreetmap.org/search?"));
    }

    #[test]
    fn les_complements_d_adresse_partent() {
        assert_eq!(nettoie("124 Rue Billaudel ,  APT 03"), "124 Rue Billaudel");
        assert_eq!(
            nettoie("124 rue Billaudel apt. 3, Bordeaux"),
            "124 rue Billaudel, Bordeaux"
        );
        assert_eq!(
            nettoie("Bât. B, 3e étage, 12 avenue Thiers, 33100 Bordeaux"),
            "12 avenue Thiers, 33100 Bordeaux"
        );
        assert_eq!(nettoie("5 rue X Appartement n°12"), "5 rue X");
        assert_eq!(nettoie("8 place Y, Esc. A, porte 4"), "8 place Y");
        assert_eq!(nettoie("10 rue Z #5"), "10 rue Z");
        assert_eq!(
            nettoie("1 cours V, 33000 Bordeaux Cedex"),
            "1 cours V, 33000 Bordeaux"
        );
    }

    #[test]
    fn les_noms_ordinaires_restent_intacts() {
        // « Chambord », « Batignolles », « Porte de Versailles » : un mot qui
        // commence comme un complément n'en est pas un.
        assert_eq!(nettoie("Château de Chambord"), "Château de Chambord");
        assert_eq!(
            nettoie("Square des Batignolles, Paris"),
            "Square des Batignolles, Paris"
        );
        assert_eq!(nettoie("Gare Saint-Jean"), "Gare Saint-Jean");
        assert_eq!(nettoie("12 rue de la Paix"), "12 rue de la Paix");
    }

    #[test]
    fn on_retente_en_raccourcissant() {
        assert_eq!(
            variantes("124 Rue Billaudel, APT 03, Bordeaux, France"),
            vec![
                "124 Rue Billaudel, Bordeaux, France",
                "124 Rue Billaudel, Bordeaux",
                "124 Rue Billaudel"
            ]
        );
        assert_eq!(variantes("Gare Saint-Jean"), vec!["Gare Saint-Jean"]);
        assert!(variantes("APT 3").is_empty());
    }

    #[test]
    fn la_base_adresse_est_lue() {
        let json = r#"{"type":"FeatureCollection","features":[
          {"type":"Feature","geometry":{"type":"Point","coordinates":[-0.5571,44.8443]},
           "properties":{"label":"124 Rue Billaudel 33100 Bordeaux","score":0.97,
           "name":"124 Rue Billaudel","postcode":"33100","city":"Bordeaux"}},
          {"type":"Feature","geometry":{"type":"Point","coordinates":[2.0,48.0]},
           "properties":{"label":"Ailleurs","score":0.2,"name":"x","postcode":"","city":"y"}}]}"#;
        let l = lieux_ban(json);
        assert_eq!(l.len(), 1, "le résultat trop incertain est écarté");
        assert_eq!(l[0].nom, "124 Rue Billaudel, 33100 Bordeaux");
        assert_eq!(l[0].nom_court(), "124 Rue Billaudel, 33100 Bordeaux");
        assert!((l[0].coord.lat - 44.8443).abs() < 1e-9);
        assert!((l[0].coord.lon + 0.5571).abs() < 1e-9);
        assert!(lieux_ban("pas du json").is_empty());
        assert!(lieux_ban(r#"{"features":[]}"#).is_empty());
    }

    #[test]
    fn la_position_oriente_les_deux_sources() {
        let ici = Some(Coord {
            lat: 44.84,
            lon: -0.57,
        });
        let b = url_ban("124 rue Billaudel", ici);
        assert!(b.starts_with("https://data.geopf.fr/geocodage/search?"));
        assert!(b.contains("&lat=44.84000&lon=-0.57000"), "{b}");
        let n = url("124 rue Billaudel", ici);
        assert!(
            n.contains("&viewbox=-1.2700,45.3400,0.1300,44.3400&bounded=0"),
            "{n}"
        );
        assert!(!url_ban("x", None).contains("lat="));
    }

    #[test]
    fn nominatim_est_lu() {
        let l = lieux_nominatim(
            r#"[{"display_name":"Gare Saint-Jean, Bordeaux","lat":"44.82","lon":"-0.55"},
                {"display_name":"cassé","lat":"x","lon":"0"}]"#,
        );
        assert_eq!(l.len(), 1);
        assert!(lieux_nominatim("{}").is_empty());
    }
}
