//! La qualité de l'air (Open-Meteo).
//!
//! Open-Meteo publie une API de qualité de l'air gratuite et **sans clé**, qui
//! accepte plusieurs coordonnées en une requête. On échantillonne donc la zone
//! regardée sur une grille plutôt que d'interroger point par point : une carte
//! se lit par taches, pas par relevés isolés.

use std::time::Duration;

use paquets::prelude::*;
use serde::{Deserialize, Serialize};

use super::feux::{BBOX_ATTENDUE, FiresQuery, fires_zone};
use super::{EnCours, Zones, attends, client_http, lance_unique};
use crate::erreur::{Erreur, Resultat, requete_query};

/// Un point de mesure de la grille.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct AirPoint {
    lat: f64,
    lon: f64,
    /// Indice européen (EAQI, 0-100+). Mesuré : Open-Meteo le calcule
    /// **partout**, y compris au-dessus du Sahara, et non sur le seul domaine
    /// européen comme je l'avais supposé. Il sert donc d'échelle unique ;
    /// l'américain reste un repli au cas où il viendrait à manquer.
    #[serde(skip_serializing_if = "Option::is_none")]
    eu: Option<f64>,
    /// Indice américain (AQI). Autre convention, non comparable : au-dessus du
    /// Sahara il monte à 1142 quand l'européen plafonne à 33, les poussières
    /// n'étant pas pondérées de la même façon.
    #[serde(skip_serializing_if = "Option::is_none")]
    us: Option<f64>,
    /// Particules fines en µg/m³ : la seule grandeur physique du lot, la même
    /// partout. Les deux indices, eux, sont des conventions régionales.
    #[serde(skip_serializing_if = "Option::is_none")]
    pm25: Option<f64>,
    /// Hauteur et largeur de la maille, en degrés.
    ///
    /// Une mesure ne vaut pas en un point mais sur une surface : le modèle CAMS
    /// travaille au cinquième de degré, et deux relevés distants de dix
    /// kilomètres rendent la même valeur. Expédier la maille permet à la carte
    /// de peindre cette surface au lieu de poser une pastille au milieu du vide.
    dlat: f64,
    dlon: f64,
    /// Place de la mesure dans la grille : colonne, ligne, et les dimensions.
    ///
    /// La carte a besoin du **treillis**, pas seulement de points épars : pour
    /// dégrader d'une mesure à sa voisine il lui faut savoir qui est voisin de
    /// qui. Le déduire des coordonnées serait fragile — une maille sans mesure
    /// est retirée de la réponse, et le treillis deviné aurait alors un trou
    /// invisible qui décalerait tout le reste d'une colonne.
    ix: usize,
    iy: usize,
    nx: usize,
    ny: usize,
}

static AIR_CACHE: Zones<AirPoint> = Zones::new();

/// Les zones dont le rafraîchissement est en cours.
static EN_COURS: EnCours = EnCours::new();

/// La qualité de l'air bouge à l'heure, pas à la minute.
///
/// Une heure pleine, et non la demi-heure d'avant : la réponse d'Open-Meteo
/// porte `interval: 3600` et une heure ronde comme horodatage — rafraîchir plus
/// souvent redemandait la même valeur, et chaque coordonnée nous est comptée.
const AIR_TTL: Duration = Duration::from_secs(3600);
/// Zones gardées, même raison que pour les feux.
const AIR_MAX_ZONES: usize = 24;
/// Plafond de la grille : 16 × 24 = 384 mailles, de quoi **couvrir** l'écran.
///
/// Cent huit suffisaient à poser des pastilles ; elles ne suffisent pas à
/// peindre une surface. Quatre cents coordonnées passent encore en une requête,
/// six cents non — Open-Meteo rend alors un 414, l'URL étant trop longue
/// (mesuré). Plus de lignes que de colonnes parce qu'un téléphone est plus haut
/// que large, et qu'un degré de longitude couvre moins de terrain qu'un degré
/// de latitude dès qu'on quitte l'équateur : à cette proportion les mailles
/// tombent à peu près carrées à l'écran sous nos latitudes.
const AIR_GRILLE_X: usize = 16;
const AIR_GRILLE_Y: usize = 24;

/// Pas du modèle CAMS, en degrés — mesuré : une demande à 48,85 revient à 48,80.
const AIR_PAS_MODELE: f64 = 0.2;

/// Le refus quand l'amont n'a rien donné à temps.
const INDISPONIBLE: &str = "mesures d'air indisponibles pour l'instant";

/// Nombre de colonnes et de lignes pour une zone, sans dépasser la finesse du
/// modèle.
///
/// Chez Open-Meteo **chaque coordonnée compte pour un appel**, et le quota
/// gratuit se compte à la journée. Demander 384 points sur une ville, c'est
/// payer 384 appels pour lire 384 fois la même valeur : le modèle ne distingue
/// rien en deçà du cinquième de degré. Le dyno s'est fait refuser pour cette
/// raison — 0 mesure rendue en 1 087 ms, quand la même requête passait depuis
/// ailleurs. La grille suit donc la taille réelle de la zone, et une ville ne
/// coûte plus que quatre appels.
///
/// Jamais moins de deux : la carte dégrade d'une mesure à sa voisine, et il lui
/// faut des voisines.
fn air_dims(o: f64, su: f64, e: f64, no: f64) -> (usize, usize) {
    let compte = |etendue: f64, max: usize| {
        let n = (etendue / AIR_PAS_MODELE).round();
        (n as usize).clamp(2, max)
    };
    (compte(e - o, AIR_GRILLE_X), compte(no - su, AIR_GRILLE_Y))
}

/// Les quatre bords d'une zone déjà validée (`ouest,sud,est,nord`).
fn bords(zone: &str) -> Option<[f64; 4]> {
    let n: Vec<f64> = zone
        .split(',')
        .filter_map(|v| v.parse::<f64>().ok())
        .collect();
    match n[..] {
        [o, su, e, no] => Some([o, su, e, no]),
        _ => None,
    }
}

/// Points d'échantillonnage d'une zone, répartis régulièrement.
///
/// Les points sont placés au **centre** des mailles et non sur les bords :
/// posés sur les bords, deux zones voisines mesureraient deux fois la même
/// ligne et laisseraient le milieu de chaque maille sans mesure.
fn air_grille(zone: &str) -> Vec<(f64, f64)> {
    let Some([o, su, e, no]) = bords(zone) else {
        return vec![];
    };
    let (nx, ny) = air_dims(o, su, e, no);
    let pas_x = (e - o) / nx as f64;
    let pas_y = (no - su) / ny as f64;
    let mut points = Vec::with_capacity(nx * ny);
    for iy in 0..ny {
        for ix in 0..nx {
            points.push((
                su + pas_y * (iy as f64 + 0.5),
                o + pas_x * (ix as f64 + 0.5),
            ));
        }
    }
    points
}

/// Hauteur et largeur d'une maille de la grille, en degrés.
fn air_maille(zone: &str) -> (f64, f64) {
    let Some([o, su, e, no]) = bords(zone) else {
        return (0.0, 0.0);
    };
    let (nx, ny) = air_dims(o, su, e, no);
    ((no - su) / ny as f64, (e - o) / nx as f64)
}

/// GET /air — qualité de l'air sur la zone regardée.
///
/// `?bbox=ouest,sud,est,nord`, mêmes bornes que pour les feux. Sans paramètre,
/// le monde entier — la grille est alors très lâche, ce qui reste honnête :
/// c'est un aperçu, pas une mesure de proximité.
///
/// Mesuré sur le dyno : 108 points rendus pour la France, l'indice européen
/// allant de 21 à 49 et les particules fines de 2,9 à 14,2 µg/m³.
pub(super) async fn liste(req: Request) -> Resultat<Response> {
    let q: FiresQuery = requete_query(&req)?;
    let zone = fires_zone(q.bbox.as_deref()).ok_or_else(|| Erreur::requete(BBOX_ATTENDUE))?;

    match AIR_CACHE.lit(&zone, AIR_TTL) {
        Some((data, stale)) => {
            if stale {
                spawn_air_refresh(zone);
            }
            Ok(Response::new().json(data))
        }
        // Même raison que pour les feux : un cadrage neuf ne doit pas
        // s'afficher désert le temps d'un aller-retour.
        None => match attends(fetch_air(&zone)).await {
            Some(data) => {
                AIR_CACHE.range(AIR_MAX_ZONES, &zone, data.clone());
                Ok(Response::new().json(data))
            }
            // Pas de liste vide ici : une liste vide veut dire « rien à
            // mesurer », et l'app l'annonçait en accusant la mer. Or à ce
            // point on ne sait rien — l'amont a refusé, ou n'a pas répondu
            // à temps. Un 503 le dit, et l'app en tire la bonne phrase.
            None => {
                spawn_air_refresh(zone);
                Err(Erreur::Indisponible(INDISPONIBLE.to_owned()))
            }
        },
    }
}

/// Rafraîchit la grille d'une zone en tâche de fond, une par une.
fn spawn_air_refresh(zone: String) {
    let z = zone.clone();
    lance_unique(&EN_COURS, &z, async move {
        match fetch_air(&zone).await {
            Some(data) => {
                tracing::info!("Air : {} points en cache pour {zone}", data.len());
                AIR_CACHE.range(AIR_MAX_ZONES, &zone, data);
            }
            None => tracing::warn!("Air : rafraîchissement impossible pour {zone}"),
        }
    });
}

/// Interroge Open-Meteo pour toute la grille en une seule requête.
async fn fetch_air(zone: &str) -> Option<Vec<AirPoint>> {
    let points = air_grille(zone);
    if points.is_empty() {
        return None;
    }
    let lats = points
        .iter()
        .map(|(la, _)| format!("{la:.3}"))
        .collect::<Vec<_>>()
        .join(",");
    let lons = points
        .iter()
        .map(|(_, lo)| format!("{lo:.3}"))
        .collect::<Vec<_>>()
        .join(",");
    let url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality\
         ?latitude={lats}&longitude={lons}&current=european_aqi,us_aqi,pm2_5"
    );
    let resp = client_http()
        .get(&url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        tracing::warn!("Air : statut HTTP {}", resp.status());
        return None;
    }
    let brut: serde_json::Value = resp.json().await.ok()?;
    Some(air_points(zone, &points, brut))
}

/// Recolle la réponse d'Open-Meteo à la grille demandée.
fn air_points(zone: &str, points: &[(f64, f64)], brut: serde_json::Value) -> Vec<AirPoint> {
    // Une seule coordonnée donne un objet, plusieurs donnent un tableau.
    let entrees = match brut {
        serde_json::Value::Array(v) => v,
        autre => vec![autre],
    };
    let (dlat, dlon) = air_maille(zone);
    let Some([o, su, e, no]) = bords(zone) else {
        return vec![];
    };
    let (nx, ny) = air_dims(o, su, e, no);
    entrees
        .iter()
        .zip(points.iter())
        .enumerate()
        .filter_map(|(i, (e, (la, lo)))| {
            let courant = e.get("current")?;
            let nombre = |cle: &str| courant.get(cle).and_then(|v| v.as_f64());
            Some(AirPoint {
                // Les coordonnées **demandées**, pas celles rendues.
                //
                // Open-Meteo répond avec le point du modèle le plus proche,
                // calé au cinquième de degré : 48,85 devient 48,80 (mesuré).
                // Zoomé sur une ville, toutes nos mailles retombent alors
                // sur le même point rendu, et la carte n'affichait qu'une
                // pastille — empilée quatre cents fois. La réponse conserve
                // l'ordre de la demande, ce qui permet de recoller chaque
                // mesure à sa maille.
                lat: *la,
                lon: *lo,
                eu: nombre("european_aqi"),
                us: nombre("us_aqi"),
                pm25: nombre("pm2_5"),
                dlat,
                dlon,
                // `air_grille` parcourt les lignes du sud au nord, et chaque
                // ligne d'ouest en est.
                ix: i % nx,
                iy: i / nx,
                nx,
                ny,
            })
        })
        // Une maille sans aucune mesure ne peint rien : autant ne pas
        // l'expédier.
        .filter(|p| p.eu.is_some() || p.us.is_some() || p.pm25.is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La grille échantillonne la zone au **centre** des mailles. Posée sur
    /// les bords, deux zones voisines mesureraient deux fois la même ligne et
    /// laisseraient le milieu de chaque maille sans mesure.
    #[test]
    fn la_grille_de_l_air_couvre_la_zone_sans_toucher_les_bords() {
        // Quinze degrés sur dix et demi : bien au-delà du pas du modèle, donc
        // la grille est plafonnée.
        let points = air_grille("-5,41,10,51.5");
        assert_eq!(points.len(), AIR_GRILLE_X * AIR_GRILLE_Y);

        // Tous strictement à l'intérieur.
        for (la, lo) in &points {
            assert!(*lo > -5.0 && *lo < 10.0, "longitude {lo}");
            assert!(*la > 41.0 && *la < 51.5, "latitude {la}");
        }
        // Et répartis : les extrêmes tombent à une demi-maille des bords.
        let pas_x = 15.0 / AIR_GRILLE_X as f64;
        let min_lon = points.iter().map(|(_, lo)| *lo).fold(f64::MAX, f64::min);
        assert!((min_lon - (-5.0 + pas_x / 2.0)).abs() < 1e-9, "{min_lon}");

        // Une zone illisible ne produit pas de grille.
        assert!(air_grille("nawak").is_empty());
    }

    /// Les mailles doivent **pavér** la zone : mises bout à bout, elles la
    /// recouvrent exactement. Sans quoi la carte peinte laisserait des rubans
    /// de fond nu entre les taches.
    #[test]
    fn les_mailles_de_l_air_pavent_la_zone() {
        let zone = "-5,41,10,51.5";
        let (dlat, dlon) = air_maille(zone);
        assert!((dlon * AIR_GRILLE_X as f64 - 15.0).abs() < 1e-9, "{dlon}");
        assert!((dlat * AIR_GRILLE_Y as f64 - 10.5).abs() < 1e-9, "{dlat}");

        // Chaque point est au centre de sa maille : les bords se touchent.
        let points = air_grille(zone);
        let mut lons: Vec<f64> = points.iter().map(|(_, lo)| *lo).collect();
        lons.sort_by(f64::total_cmp);
        lons.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        for paire in lons.windows(2) {
            assert!(
                (paire[1] - paire[0] - dlon).abs() < 1e-9,
                "trou entre {} et {}",
                paire[0],
                paire[1]
            );
        }
        // Le premier bord gauche retombe sur l'ouest de la zone.
        assert!((lons[0] - dlon / 2.0 - (-5.0)).abs() < 1e-9, "{}", lons[0]);

        assert_eq!(air_maille("nawak"), (0.0, 0.0));
    }

    /// La grille ne doit jamais être plus fine que le modèle.
    ///
    /// Chez Open-Meteo chaque coordonnée compte pour un appel, et le quota se
    /// compte à la journée. Demander 384 points sur une ville, c'est payer 384
    /// appels pour lire 384 fois la même valeur — le modèle ne distingue rien
    /// en deçà du cinquième de degré. C'est ce qui a fait refuser le dyno.
    #[test]
    fn la_grille_de_l_air_ne_depasse_pas_la_finesse_du_modele() {
        // Une ville vue de près : la zone fait un quart de degré.
        let ville = air_grille("-0.75,44.75,-0.5,45");
        assert_eq!(ville.len(), 4, "une ville ne doit coûter que quatre appels");

        // Jamais moins de deux par côté : la nappe dégrade d'une mesure à sa
        // voisine, et il lui faut des voisines.
        let (nx, ny) = air_dims(0.0, 0.0, 0.01, 0.01);
        assert_eq!((nx, ny), (2, 2));

        // Une zone moyenne suit la finesse du modèle sans la dépasser.
        let (nx, ny) = air_dims(0.0, 0.0, 1.0, 2.0);
        assert_eq!((nx, ny), (5, 10), "un degré vaut cinq mailles de 0,2°");

        // Le monde reste plafonné.
        assert_eq!(
            air_dims(-180.0, -85.0, 180.0, 85.0),
            (AIR_GRILLE_X, AIR_GRILLE_Y)
        );

        // Et les indices restent cohérents avec les dimensions annoncées.
        let (nx, _) = air_dims(-0.75, 44.75, -0.5, 45.0);
        assert_eq!(nx, 2);
        assert_eq!(ville.len() % nx, 0);
    }

    /// La réponse se recolle aux mailles demandées, dans l'ordre : les
    /// coordonnées rendues par l'amont sont ignorées, une maille sans aucune
    /// mesure disparaît sans décaler le treillis.
    #[test]
    fn les_mesures_se_recollent_a_leur_maille() {
        let zone = "-0.75,44.75,-0.5,45";
        let points = air_grille(zone);
        let brut = serde_json::json!([
            {"latitude": 44.8, "longitude": -0.6,
             "current": {"european_aqi": 21.0, "us_aqi": 30.0, "pm2_5": 4.5}},
            {"latitude": 44.8, "longitude": -0.6,
             "current": {"european_aqi": null, "us_aqi": null, "pm2_5": null}},
            {"latitude": 44.8, "longitude": -0.6, "current": {"pm2_5": 7.0}},
            {"latitude": 44.8, "longitude": -0.6}
        ]);
        let air = air_points(zone, &points, brut);
        assert_eq!(air.len(), 2);
        assert_eq!((air[0].lat, air[0].lon), points[0]);
        assert_eq!((air[0].ix, air[0].iy, air[0].nx, air[0].ny), (0, 0, 2, 2));
        assert_eq!(air[0].eu, Some(21.0));
        // La troisième maille : première colonne, deuxième ligne.
        assert_eq!((air[1].ix, air[1].iy), (0, 1));
        let json = serde_json::to_value(&air[1]).unwrap();
        assert_eq!(json["pm25"], 7.0);
        assert!(json.get("eu").is_none() && json.get("us").is_none());
        assert_eq!(json["dlat"], 0.125);

        // Une seule coordonnée : un objet, pas un tableau.
        let seul = serde_json::json!({"current": {"european_aqi": 10.0}});
        assert_eq!(air_points(zone, &points, seul).len(), 1);
    }
}
