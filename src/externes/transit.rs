//! Les transports en commun (Transitous).
//!
//! Transitous est un service communautaire de calcul d'itinéraires en
//! transports, bâti sur MOTIS et alimenté par les GTFS publics — sans clé.
//! Mesuré sur Bordeaux : six itinéraires mêlant trams E et C, bus 70, G et 27,
//! et le TER, en une requête. L'app passe par ici et non en direct : la réponse
//! brute pèse 219 Ko là où l'écran a besoin de quelques kilo-octets, et le
//! fournisseur se remplace alors d'un seul côté.

use std::time::Duration;

use paquets::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Zones, client_http};
use crate::erreur::{Erreur, Resultat, requete_query};

/// Un tronçon d'un trajet : une marche, ou un véhicule et sa ligne.
#[derive(Clone, Debug, Serialize)]
pub(super) struct TransitLeg {
    /// WALK, TRAM, BUS, REGIONAL_RAIL, RAIL, SUBWAY, FERRY…
    mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ligne: Option<String>,
    /// Couleur de la ligne, hexadécimal sans croisillon, telle que le réseau
    /// la publie.
    #[serde(skip_serializing_if = "Option::is_none")]
    couleur: Option<String>,
    /// La girouette du véhicule — ce qu'on lit sur son front.
    #[serde(skip_serializing_if = "Option::is_none")]
    direction: Option<String>,
    de: String,
    a: String,
    depart: String,
    arrivee: String,
    /// Tracé en polyligne encodée, précision sept — celle de MOTIS, qui
    /// l'annonce dans sa réponse.
    trace: String,
    /// Retard temps réel en secondes, quand le réseau le publie : l'écart
    /// entre le départ annoncé et l'horaire en papier. Absent quand le
    /// passage n'est connu qu'à l'horaire — l'app distingue alors « à
    /// l'heure, mesuré » de « théorique ».
    #[serde(skip_serializing_if = "Option::is_none")]
    retard_s: Option<i64>,
}

/// Un itinéraire complet.
#[derive(Clone, Debug, Serialize)]
pub(super) struct TransitItineraire {
    duree_s: i64,
    depart: String,
    arrivee: String,
    correspondances: i64,
    legs: Vec<TransitLeg>,
}

static TRANSIT_CACHE: Zones<TransitItineraire> = Zones::new("transit");

/// Quatre-vingt-dix secondes : un horaire de passage est une donnée du moment,
/// mais deux appuis successifs sur le même trajet n'ont pas à payer deux fois.
const TRANSIT_TTL: Duration = Duration::from_secs(90);
const TRANSIT_MAX_ZONES: usize = 64;

#[derive(Deserialize)]
struct TransitQuery {
    from: Option<String>,
    to: Option<String>,
    /// Heure de départ souhaitée (ou d'arrivée, cf. `arrive`), ISO 8601 UTC
    /// terminé par « Z ». Absente : maintenant.
    time: Option<String>,
    /// « 1 » : `time` est une heure d'arrivée au plus tard, pas de départ.
    arrive: Option<String>,
}

/// L'écart en secondes entre deux horodatages ISO de MOTIS.
///
/// Sans bibliothèque de dates : les deux horodatages sortent du même
/// générateur, même format, même fuseau — la différence se lit sur les
/// secondes du jour, et un retard qui enjambe minuit se ramène dans
/// ±12 heures. Un tram n'a pas douze heures de retard ; un écart pareil
/// serait une donnée cassée, rendue comme telle.
fn transit_ecart_s(reel: &str, prevu: &str) -> Option<i64> {
    let secondes = |h: &str| -> Option<i64> {
        let t = h.split('T').nth(1)?;
        let mut morceaux = t.trim_end_matches('Z').split(':');
        let heures: i64 = morceaux.next()?.parse().ok()?;
        let minutes: i64 = morceaux.next()?.parse().ok()?;
        let sec: i64 = morceaux
            .next()
            .and_then(|v| v.split('.').next())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Some(heures * 3600 + minutes * 60 + sec)
    };
    let mut ecart = secondes(reel)? - secondes(prevu)?;
    if ecart > 43_200 {
        ecart -= 86_400;
    } else if ecart < -43_200 {
        ecart += 86_400;
    }
    Some(ecart)
}

/// Valide l'heure venue du client, qui entre telle quelle dans l'URL amont.
///
/// Le charme est strict — chiffres, tirets, deux-points, « T », point, « Z » —
/// parce qu'un « + » de fuseau serait mangé par l'encodage d'URL, et que tout
/// le reste n'a rien à faire dans une heure. L'app envoie de l'UTC en « Z ».
fn transit_heure(brut: Option<&str>) -> Option<String> {
    let h = brut?.trim();
    if h.len() < 16 || h.len() > 30 || !h.ends_with('Z') {
        return None;
    }
    if h.chars().all(|c| c.is_ascii_digit() || "T:-.Z".contains(c)) {
        Some(h.to_string())
    } else {
        None
    }
}

/// « lat,lon » validé et arrondi au dix-millième — onze mètres, le même arrêt.
fn transit_point(brut: Option<&str>) -> Option<(f64, f64)> {
    let n: Vec<f64> = brut?
        .split(',')
        .filter_map(|v| v.trim().parse::<f64>().ok())
        .collect();
    let [lat, lon] = n[..] else { return None };
    if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 {
        return None;
    }
    let q = |v: f64| (v * 10_000.0).round() / 10_000.0;
    Some((q(lat), q(lon)))
}

/// Taille les itinéraires MOTIS au strict nécessaire de l'écran.
fn transit_taille(brut: &serde_json::Value) -> Vec<TransitItineraire> {
    let texte =
        |v: &serde_json::Value, cle: &str| v.get(cle).and_then(|x| x.as_str()).map(str::to_string);
    let nom = |v: &serde_json::Value, cle: &str| {
        v.get(cle)
            .and_then(|x| x.get("name"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string()
    };
    brut.get("itineraries")
        .and_then(|v| v.as_array())
        .map(|its| {
            its.iter()
                .filter_map(|it| {
                    let legs = it
                        .get("legs")?
                        .as_array()?
                        .iter()
                        .filter_map(|l| {
                            Some(TransitLeg {
                                mode: texte(l, "mode")?,
                                ligne: texte(l, "routeShortName").filter(|s| !s.is_empty()),
                                couleur: texte(l, "routeColor").filter(|s| !s.is_empty()),
                                direction: texte(l, "headsign").filter(|s| !s.is_empty()),
                                de: nom(l, "from"),
                                a: nom(l, "to"),
                                depart: texte(l, "startTime").unwrap_or_default(),
                                arrivee: texte(l, "endTime").unwrap_or_default(),
                                trace: l
                                    .get("legGeometry")
                                    .and_then(|g| g.get("points"))
                                    .and_then(|p| p.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                // Le retard n'a de sens que si le réseau
                                // publie du temps réel pour ce passage.
                                retard_s: if l.get("realTime").and_then(|v| v.as_bool())
                                    == Some(true)
                                {
                                    match (texte(l, "startTime"), texte(l, "scheduledStartTime")) {
                                        (Some(reel), Some(prevu)) => transit_ecart_s(&reel, &prevu),
                                        _ => None,
                                    }
                                } else {
                                    None
                                },
                            })
                        })
                        .collect::<Vec<_>>();
                    if legs.is_empty() {
                        return None;
                    }
                    Some(TransitItineraire {
                        duree_s: it.get("duration").and_then(|v| v.as_i64()).unwrap_or(0),
                        depart: texte(it, "startTime").unwrap_or_default(),
                        arrivee: texte(it, "endTime").unwrap_or_default(),
                        correspondances: it.get("transfers").and_then(|v| v.as_i64()).unwrap_or(0),
                        legs,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// GET /transit — itinéraires en transports en commun entre deux points.
///
/// `?from=lat,lon&to=lat,lon`. La réponse est un tableau d'itinéraires, du
/// plus tôt arrivé au plus tard, chacun détaillant ses tronçons.
pub(super) async fn liste(req: Request) -> Resultat<Response> {
    let q: TransitQuery = requete_query(&req)?;
    let (Some(de), Some(vers)) = (
        transit_point(q.from.as_deref()),
        transit_point(q.to.as_deref()),
    ) else {
        return Err(Erreur::requete("from et to attendus : lat,lon"));
    };

    let heure = transit_heure(q.time.as_deref());
    let arrivee = q.arrive.as_deref() == Some("1") && heure.is_some();
    let zone = format!(
        "{},{}|{},{}|{}|{}",
        de.0,
        de.1,
        vers.0,
        vers.1,
        heure.as_deref().unwrap_or("-"),
        if arrivee { "a" } else { "d" }
    );
    if let Some((data, false)) = TRANSIT_CACHE.lit(&zone, TRANSIT_TTL) {
        return Ok(Response::new().json(data));
    }

    let mut url = format!(
        "https://api.transitous.org/api/v1/plan\
         ?fromPlace={},{}&toPlace={},{}&numItineraries=15",
        de.0, de.1, vers.0, vers.1
    );
    // « Partir à » et « arriver avant » : les deux mesurés contre l'API —
    // `time` décale les départs, `arriveBy=true` borne les arrivées.
    if let Some(h) = &heure {
        url.push_str(&format!("&time={h}"));
        if arrivee {
            url.push_str("&arriveBy=true");
        }
    }
    let brut: Option<serde_json::Value> = async {
        let resp = client_http()
            .get(&url)
            // Transitous exige un client qui se nomme : sans User-Agent, il
            // répond 403 — mesuré, 403 anonyme contre 200 nommé. reqwest n'en
            // envoie aucun par défaut, et le dyno s'est fait refuser pour ça.
            .header(
                "User-Agent",
                "moncap-gps/1.2 (moncap-c41a5aaf07e8.herokuapp.com)",
            )
            .timeout(Duration::from_secs(12))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            tracing::warn!("Transit : statut HTTP {}", resp.status());
            return None;
        }
        resp.json().await.ok()
    }
    .await;

    let Some(brut) = brut else {
        // Pas de liste vide : vide veut dire « aucun trajet possible », et
        // l'app l'annoncerait comme tel. Ici on ne sait rien — même leçon que
        // pour la qualité de l'air.
        return Err(Erreur::Indisponible(
            "transports indisponibles pour l'instant".to_owned(),
        ));
    };

    // Le même plafond que partout ailleurs : quinze choix, celui de l'écran.
    let mut data = transit_taille(&brut);
    data.truncate(15);
    TRANSIT_CACHE.range(TRANSIT_MAX_ZONES, &zone, data.clone());
    Ok(Response::new().json(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les points de transit viennent du client et entrent dans une URL vers
    /// Transitous : bornés, arrondis, ou refusés.
    #[test]
    fn le_point_de_transit_est_borne() {
        // L'heure entre telle quelle dans l'URL amont : stricte ou refusée.
        assert_eq!(
            transit_heure(Some("2026-08-21T08:00:00Z")).as_deref(),
            Some("2026-08-21T08:00:00Z")
        );
        for mauvaise in [
            "2026-08-21T08:00:00+02:00", // un « + » serait mangé par l'URL
            "hier",
            "",
            "2026-08-21",
            "javascript:Z",
        ] {
            assert!(transit_heure(Some(mauvaise)).is_none(), "{mauvaise:?}");
        }
        assert!(transit_heure(None).is_none());

        assert_eq!(
            transit_point(Some("44.8820,-0.6051")),
            Some((44.882, -0.6051))
        );
        // Arrondi au dix-millième : onze mètres, le même arrêt — et une clé
        // de cache que deux appuis voisins partagent.
        assert_eq!(
            transit_point(Some("44.88204,-0.60508")),
            Some((44.882, -0.6051))
        );
        for mauvais in ["91,0", "0,181", "nawak", "1", "", "1,2,3"] {
            assert!(transit_point(Some(mauvais)).is_none(), "{mauvais:?}");
        }
        assert!(transit_point(None).is_none());
    }

    /// La taille garde ce que l'écran montre, et rien d'autre.
    ///
    /// Le gabarit reprend la forme réelle d'une réponse MOTIS, relevée sur
    /// Transitous pour Bordeaux : la réponse brute pèse 219 Ko pour six
    /// itinéraires, l'écran a besoin de la ligne, sa couleur, la girouette,
    /// les arrêts, les heures et le tracé.
    #[test]
    fn la_taille_du_transit_garde_l_essentiel() {
        let brut: serde_json::Value = serde_json::json!({
            "itineraries": [{
                "duration": 3180, "transfers": 1,
                "startTime": "2026-08-19T18:40:00Z", "endTime": "2026-08-19T19:33:00Z",
                "legs": [
                    {"mode": "WALK", "startTime": "2026-08-19T18:40:00Z",
                     "endTime": "2026-08-19T18:47:00Z",
                     "from": {"name": "START", "lat": 44.882, "lon": -0.605},
                     "to": {"name": "Ausone", "lat": 44.8792, "lon": -0.6025},
                     "legGeometry": {"points": "abc", "precision": 7}},
                    {"mode": "TRAM", "routeShortName": "E", "routeColor": "967651",
                     "headsign": "Quinconces", "realTime": true,
                     "scheduledStartTime": "2026-08-19T18:44:00Z",
                     "startTime": "2026-08-19T18:47:00Z", "endTime": "2026-08-19T19:03:00Z",
                     "from": {"name": "Ausone"}, "to": {"name": "Quinconces"},
                     "legGeometry": {"points": "def", "precision": 7},
                     "agencyName": "TBM", "tripId": "long-inutile", "intermediateStops": []}
                ]
            }]
        });
        let its = transit_taille(&brut);
        assert_eq!(its.len(), 1);
        let it = &its[0];
        assert_eq!((it.duree_s, it.correspondances), (3180, 1));
        assert_eq!(it.legs.len(), 2);
        let tram = &it.legs[1];
        assert_eq!(tram.ligne.as_deref(), Some("E"));
        assert_eq!(tram.couleur.as_deref(), Some("967651"));
        assert_eq!(tram.direction.as_deref(), Some("Quinconces"));
        assert_eq!(
            (tram.de.as_str(), tram.a.as_str()),
            ("Ausone", "Quinconces")
        );
        assert_eq!(tram.trace, "def");
        // Temps réel publié : trois minutes de retard, mesurées entre le
        // départ annoncé et l'horaire en papier.
        assert_eq!(tram.retard_s, Some(180));
        // La marche n'a pas de temps réel, et n'en invente pas.
        assert!(it.legs[0].retard_s.is_none());
        // L'écart sait enjamber minuit sans exploser.
        assert_eq!(
            transit_ecart_s("2026-08-20T00:02:00Z", "2026-08-19T23:58:00Z"),
            Some(240)
        );
        // La marche n'a ni ligne ni couleur, et ne les invente pas.
        assert!(it.legs[0].ligne.is_none() && it.legs[0].couleur.is_none());
        // Ce que l'écran ne montre pas n'est pas expédié.
        let json = serde_json::to_string(&its).unwrap();
        assert!(!json.contains("tripId") && !json.contains("agencyName"));

        // Une réponse sans itinéraire rend une liste vide, pas une panique.
        assert!(transit_taille(&serde_json::json!({})).is_empty());
    }

    /// Un tronçon sans mode tombe ; un itinéraire sans tronçon aussi.
    #[test]
    fn les_itineraires_vides_tombent() {
        let brut = serde_json::json!({
            "itineraries": [
                {"duration": 60, "legs": [{"startTime": "x"}]},
                {"legs": [{"mode": "WALK"}]}
            ]
        });
        let its = transit_taille(&brut);
        assert_eq!(its.len(), 1);
        assert_eq!(its[0].duree_s, 0);
        assert_eq!(its[0].legs[0].de, "");
    }
}
