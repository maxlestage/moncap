//! Le trafic en temps réel : bouchons et routes fermées (TomTom).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use paquets::prelude::*;
use serde::{Deserialize, Serialize};

use super::{client_http, verrou};
use crate::erreur::{Resultat, requete_query};

/// Un bouchon (ou route fermée) en temps réel, tel qu'exposé aux clients.
#[derive(Clone, Debug, Serialize)]
struct TrafficPoint {
    lat: f64,
    lon: f64,
    /// Retard estimé en secondes (0 si inconnu).
    delay: i64,
}

/// Paramètres de `GET /traffic` : emprise « minLon,minLat,maxLon,maxLat ».
#[derive(Deserialize)]
struct TrafficQuery {
    bbox: String,
}

/// Cache mémoire des bouchons par emprise (arrondie) : borne le nombre d'appels
/// à TomTom (quota gratuit) même avec beaucoup d'utilisateurs.
type TrafficCache = Mutex<HashMap<String, (Instant, Vec<TrafficPoint>)>>;
static TRAFFIC_CACHE: LazyLock<TrafficCache> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Fraîcheur du cache trafic (les incidents TomTom évoluent au fil des minutes).
const TRAFFIC_TTL: Duration = Duration::from_secs(60);

/// La clé TomTom, détenue **par le serveur** (`TOMTOM_KEY`).
pub(super) fn tomtom_key() -> Option<String> {
    std::env::var("TOMTOM_KEY")
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

/// L'emprise validée : 4 nombres, dans les bornes géographiques.
fn emprise(brut: &str) -> Option<[f64; 4]> {
    let parts: Vec<f64> = brut
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let [a, b, c, d] = parts[..] else { return None };
    if !(-180.0..=180.0).contains(&a)
        || !(-90.0..=90.0).contains(&b)
        || !(-180.0..=180.0).contains(&c)
        || !(-90.0..=90.0).contains(&d)
    {
        return None;
    }
    Some([a, b, c, d])
}

/// GET /traffic?bbox=minLon,minLat,maxLon,maxLat — bouchons et routes fermées
/// en temps réel (API TomTom Traffic Incidents).
///
/// La clé TomTom est détenue **par le serveur** (variable d'environnement
/// `TOMTOM_KEY`), jamais exposée aux clients : tous les utilisateurs profitent
/// du trafic réel sans configurer de clé. Sans clé → liste vide (silencieux).
/// Résultat mis en cache 60 s par emprise ; en cas d'échec, liste vide.
pub(super) async fn liste(req: Request) -> Resultat<Response> {
    let q: TrafficQuery = requete_query(&req)?;
    let vide = || Ok(Response::new().json(Vec::<TrafficPoint>::new()));
    let Some(key) = tomtom_key() else {
        return vide();
    };
    let Some(parts) = emprise(&q.bbox) else {
        return vide();
    };
    // Clé de cache : emprise arrondie (~0,1° ≈ 11 km) pour regrouper les appels.
    let cache_key = format!(
        "{:.1},{:.1},{:.1},{:.1}",
        parts[0], parts[1], parts[2], parts[3]
    );
    let bbox = format!("{},{},{},{}", parts[0], parts[1], parts[2], parts[3]);

    // Lecture du cache sous verrou court, puis on relâche : on ne tient PAS le
    // verrou pendant l'appel réseau à TomTom, sinon toutes les requêtes
    // /traffic (même sur d'autres emprises) seraient sérialisées derrière un
    // seul fetch.
    {
        let guard = verrou(&TRAFFIC_CACHE);
        if let Some((at, data)) = guard.get(&cache_key)
            && at.elapsed() < TRAFFIC_TTL
        {
            return Ok(Response::new().json(data));
        }
    }
    let fetched = fetch_traffic(&key, &bbox).await;
    let mut guard = verrou(&TRAFFIC_CACHE);
    let data = match fetched {
        Some(data) => {
            // Purge grossière des entrées périmées avant d'insérer.
            guard.retain(|_, (at, _)| at.elapsed() < TRAFFIC_TTL);
            guard.insert(cache_key, (Instant::now(), data.clone()));
            data
        }
        None => guard
            .get(&cache_key)
            .map(|(_, d)| d.clone())
            .unwrap_or_default(),
    };
    Ok(Response::new().json(data))
}

/// Interroge l'API TomTom Traffic Incidents (v5) sur l'emprise donnée et ne
/// retient que les bouchons (catégorie 6) et routes fermées (8). `None` en cas
/// d'échec. Garde-fou à 300 incidents.
async fn fetch_traffic(key: &str, bbox: &str) -> Option<Vec<TrafficPoint>> {
    let fields = "{incidents{geometry{type,coordinates},properties{iconCategory,delay}}}";
    let resp = client_http()
        .get("https://api.tomtom.com/traffic/services/5/incidentDetails")
        .query(&[
            ("key", key),
            ("bbox", bbox),
            ("fields", fields),
            ("language", "fr-FR"),
            ("categoryFilter", "6,8"),
            ("timeValidityFilter", "present"),
        ])
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: TomTomResp = resp.json().await.ok()?;
    Some(points_trafic(body))
}

/// Un point par incident : le point lui-même, ou le milieu de la ligne.
fn points_trafic(body: TomTomResp) -> Vec<TrafficPoint> {
    let mut out = Vec::new();
    for inc in body.incidents.unwrap_or_default() {
        let Some(geom) = inc.geometry else { continue };
        // Point représentatif : le point lui-même, ou le milieu de la ligne.
        let coord = match geom.gtype.as_str() {
            "Point" => lonlat(&geom.coordinates),
            "LineString" => geom
                .coordinates
                .as_array()
                .and_then(|a| a.get(a.len() / 2))
                .and_then(lonlat),
            _ => None,
        };
        let Some((lon, lat)) = coord else { continue };
        let delay = inc
            .properties
            .and_then(|p| p.delay)
            .map(|d| d as i64)
            .unwrap_or(0);
        out.push(TrafficPoint { lat, lon, delay });
        if out.len() >= 300 {
            break;
        }
    }
    out
}

/// Extrait `(lon, lat)` d'une valeur JSON `[lon, lat]`.
fn lonlat(v: &serde_json::Value) -> Option<(f64, f64)> {
    let a = v.as_array()?;
    Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?))
}

#[derive(Deserialize)]
struct TomTomResp {
    incidents: Option<Vec<TomTomIncident>>,
}

#[derive(Deserialize)]
struct TomTomIncident {
    geometry: Option<TomTomGeom>,
    properties: Option<TomTomProps>,
}

#[derive(Deserialize)]
struct TomTomGeom {
    #[serde(rename = "type", default)]
    gtype: String,
    #[serde(default)]
    coordinates: serde_json::Value,
}

#[derive(Deserialize)]
struct TomTomProps {
    delay: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_emprise_est_bornee() {
        assert_eq!(
            emprise("-0.7, 44.8,-0.5,44.9"),
            Some([-0.7, 44.8, -0.5, 44.9])
        );
        for mauvaise in ["", "1,2,3", "1,2,3,4,5", "181,0,1,1", "0,91,1,1", "a,b,c,d"] {
            assert!(emprise(mauvaise).is_none(), "{mauvaise:?}");
        }
    }

    #[test]
    fn un_point_par_incident() {
        let body: TomTomResp = serde_json::from_value(serde_json::json!({
            "incidents": [
                {"geometry": {"type": "Point", "coordinates": [2.35, 48.85]},
                 "properties": {"iconCategory": 6, "delay": 125.7}},
                {"geometry": {"type": "LineString",
                              "coordinates": [[0.0, 1.0], [2.0, 3.0], [4.0, 5.0]]}},
                {"geometry": {"type": "Polygon", "coordinates": []}},
                {"properties": {"delay": 10}}
            ]
        }))
        .unwrap();
        let pts = points_trafic(body);
        assert_eq!(pts.len(), 2);
        assert_eq!((pts[0].lat, pts[0].lon, pts[0].delay), (48.85, 2.35, 125));
        // Le milieu de la ligne, et un retard inconnu vaut zéro.
        assert_eq!((pts[1].lat, pts[1].lon, pts[1].delay), (3.0, 2.0, 0));
    }
}
