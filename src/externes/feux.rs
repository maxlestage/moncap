//! Les feux actifs, détectés par satellite (NASA FIRMS).

use std::collections::HashMap;
use std::time::Duration;

use paquets::prelude::*;
use serde::{Deserialize, Serialize};

use super::{EnCours, Zones, attends, client_http, lance_unique};
use crate::cache::cache;
use crate::erreur::{Erreur, Resultat, requete_query};

/// Le refus d'une zone illisible, pour les feux comme pour l'air.
pub(super) const BBOX_ATTENDUE: &str = "bbox attendue : ouest,sud,est,nord";

/// Un foyer d'incendie détecté par satellite (NASA FIRMS), tel qu'exposé aux
/// clients.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct FirePoint {
    lat: f64,
    lon: f64,
    /// Dernière détection satellite (epoch secondes UTC), si connue — permet
    /// au client d'afficher « Mise à jour il y a X h ». Absente du JSON quand
    /// la colonne CSV manque (compatibilité clients existants).
    #[serde(skip_serializing_if = "Option::is_none")]
    t: Option<i64>,
    /// **Première** détection à cet endroit sur la fenêtre demandée — permet
    /// au client d'afficher « Créé il y a X j ». Le client le décrivait déjà,
    /// mais rien ne le lui envoyait : un feu d'une semaine et un feu d'une
    /// heure se ressemblaient trait pour trait. Absente comme `t` quand la
    /// colonne CSV manque.
    #[serde(skip_serializing_if = "Option::is_none")]
    t0: Option<i64>,
}

/// Un cache par zone demandée. La France tenait dans une seule entrée ;
/// le monde se regarde par morceaux, et deux visiteurs qui consultent deux
/// continents ne doivent pas se chasser mutuellement du cache.
static FIRES_CACHE: Zones<FirePoint> = Zones::new("feux");

/// Les zones dont le rafraîchissement est en cours.
static EN_COURS: EnCours = EnCours::new();

/// Nombre de zones gardées. Au-delà, la plus ancienne cède la place — sans
/// borne, une carte parcourue longuement finirait par tout garder en mémoire.
const FIRES_MAX_ZONES: usize = 24;

/// Zone du monde entier, au format attendu par FIRMS : ouest,sud,est,nord.
const FIRES_MONDE: &str = "-180,-90,180,90";

/// Zone regardée, en degrés : `?bbox=ouest,sud,est,nord`. Absente, c'est le
/// monde entier — un visiteur qui ne demande rien de précis voit tout.
#[derive(Deserialize)]
pub(super) struct FiresQuery {
    pub bbox: Option<String>,
}

/// Zone demandée, validée et arrondie au quart de degré.
///
/// L'arrondi sert de clé de cache : deux cartes voisines partagent alors la
/// même entrée, au lieu d'en créer une par pixel de déplacement.
pub(super) fn fires_zone(brut: Option<&str>) -> Option<String> {
    let Some(brut) = brut else {
        return Some(FIRES_MONDE.to_string());
    };
    let n: Vec<f64> = brut
        .split(',')
        .filter_map(|v| v.trim().parse::<f64>().ok())
        .collect();
    let [o, su, e, no] = n[..] else { return None };
    if [o, su, e, no].iter().any(|v| !v.is_finite()) || o >= e || su >= no {
        return None;
    }
    // Ramenée dans le globe plutôt que refusée. Dézoomée, une carte MapLibre
    // annonce des longitudes bien au-delà de ±180 — jusqu'à ±246 mesuré sur
    // l'app — parce que le monde s'y répète. Refuser ces boîtes rendait un 400,
    // et la carte restait vide précisément quand on regardait le plus large.
    // Quatre-vingt-cinq degrés et non quatre-vingt-dix : au-delà, le Mercator
    // sphérique des cartes part à l'infini, et la nappe de l'air — tendue sur
    // ce quadrillage-là — n'aurait plus de bord haut. Rien n'est perdu : aucune
    // carte du monde ne montre les pôles, et rien n'y brûle.
    let borne = |v: f64, max: f64| v.clamp(-max, max);
    // Arrondi **vers l'extérieur**, et non au plus proche. Au plus proche, une
    // carte serrée sur une ville s'écrasait : Bordeaux vue de près donne une
    // boîte plus étroite qu'un quart de degré, dont les deux bords tombaient
    // sur le même quart. La zone devenait plate, la grille de l'air se réduisait
    // à un seul point répété cent huit fois — la pastille verte unique de
    // l'écran — et la NASA recevait une boîte sans surface. Vers l'extérieur, la
    // zone fait toujours au moins un quart de degré et couvre ce qu'on regarde.
    let bas = |v: f64| (v * 4.0).floor() / 4.0;
    let haut = |v: f64| (v * 4.0).ceil() / 4.0;
    Some(format!(
        "{},{},{},{}",
        bas(borne(o, 180.0)),
        bas(borne(su, 85.0)),
        haut(borne(e, 180.0)),
        haut(borne(no, 85.0))
    ))
}

/// Durée de fraîcheur du cache des feux (les données FIRMS NRT ne sont mises à
/// jour que quelques fois par jour).
const FIRES_TTL: Duration = Duration::from_secs(900);

/// Clé FIRMS (MAP_KEY) détenue **par le serveur** (variable d'environnement
/// `FIRMS_MAP_KEY`, repli `FIRMS_KEY`), jamais exposée aux clients.
pub(super) fn fires_key() -> Option<String> {
    std::env::var("FIRMS_MAP_KEY")
        .or_else(|_| std::env::var("FIRMS_KEY"))
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

/// Au démarrage, avec une clé : le monde entier part en cache.
pub(super) fn demarre(disque: &'static Db) {
    if let Some(key) = fires_key() {
        spawn_fires_refresh(disque, key, FIRES_MONDE.to_string());
    }
}

/// GET /fires — incendies actifs (NASA FIRMS), dans le monde ou sur une zone.
///
/// `?bbox=ouest,sud,est,nord` restreint la demande à ce qu'on regarde ; sans
/// paramètre, c'est la planète entière. La profondeur d'historique suit la
/// surface, la réponse est plafonnée, et chaque zone a son cache.
///
/// Répond **immédiatement** avec le dernier cache connu ; si celui-ci est
/// périmé, un rafraîchissement part en tâche de fond. La requête FIRMS
/// (7 jours de détections) peut prendre bien plus que le timeout HTTP d'un
/// client mobile : elle ne doit jamais bloquer la réponse.
pub(super) async fn liste(req: Request) -> Resultat<Response> {
    let q: FiresQuery = requete_query(&req)?;
    let zone = fires_zone(q.bbox.as_deref()).ok_or_else(|| Erreur::requete(BBOX_ATTENDUE))?;
    let Some(key) = fires_key() else {
        return Ok(Response::new().json(Vec::<FirePoint>::new()));
    };
    let disque = cache(&req);

    match FIRES_CACHE
        .lit_ou_disque(disque, FIRES_MAX_ZONES, &zone, FIRES_TTL)
        .await
    {
        // Zone déjà vue : on répond tout de suite, quitte à rafraîchir derrière.
        Some((data, stale)) => {
            if stale {
                spawn_fires_refresh(disque, key, zone);
            }
            Ok(Response::new().json(data))
        }
        // Zone inconnue : on **attend**. Répondre une liste vide en lançant la
        // recherche derrière condamnait chaque nouveau cadrage à s'afficher
        // désert : sur une carte qu'on fait glisser, chaque zone est nouvelle
        // une fois, et c'est précisément ce moment-là qu'on regarde.
        None => {
            // Un seul jour ici, et c'est délibéré : quelqu'un attend derrière,
            // et sept jours de CSV ne tiennent pas dans les cinq secondes
            // qu'on s'accorde. On montre donc vite ce qu'on a.
            let data = attends(fetch_fires_days(&key, 1, &zone)).await;
            if let Some(data) = data {
                FIRES_CACHE
                    .range_et_disque(disque, FIRES_MAX_ZONES, &zone, data.clone())
                    .await;
                // …puis on va chercher les sept jours derrière. Sans cela, le
                // cache gardait sa version courte un quart d'heure entier
                // (FIRES_TTL) : la carte restait sans âges ni cicatrices tout
                // ce temps, alors que personne n'attendait plus.
                spawn_fires_refresh(disque, key, zone);
                Ok(Response::new().json(data))
            } else {
                // Trop long ou en échec : le rafraîchissement continue en fond,
                // la prochaine demande trouvera le résultat.
                spawn_fires_refresh(disque, key, zone);
                Ok(Response::new().json(Vec::<FirePoint>::new()))
            }
        }
    }
}

/// Lance (au plus un à la fois par zone) un rafraîchissement du cache des
/// feux en tâche de fond. Essaie d'abord 7 jours d'historique (pour « Créé il
/// y a X j ») ; si la NASA ne répond pas, repli sur 2 jours, puis sur 1, bien
/// plus légers, pour que la carte reste alimentée quoi qu'il arrive.
fn spawn_fires_refresh(disque: &'static Db, key: String, zone: String) {
    let z = zone.clone();
    lance_unique(&EN_COURS, &z, async move {
        // Sept jours, et non plus un. La profondeur ne sert pas à montrer de
        // vieux feux — le client n'affiche que les foyers vus dans les
        // dernières 36 h — mais à les **dater** et à dessiner la zone brûlée,
        // qui s'accumule sur toute l'histoire du foyer. Avec un seul jour,
        // tout feu semblait nouveau et toute cicatrice semblait petite.
        //
        // C'est le chemin de fond : personne n'attend derrière, le coût d'un
        // CSV de sept jours se paie ici et non sur l'écran de quelqu'un. Si
        // la NASA ne suit pas, on redescend plutôt que de ne rien mettre en
        // cache — deux jours valent mieux qu'un jour, qui vaut mieux que rien.
        let profond = match fetch_fires_days(&key, 7, &zone).await {
            Some(d) => Some(d),
            None => match fetch_fires_days(&key, 2, &zone).await {
                Some(d) => Some(d),
                None => fetch_fires_days(&key, 1, &zone).await,
            },
        };
        match profond {
            Some(data) => {
                tracing::info!("FIRMS : {} cellules en cache pour {zone}", data.len());
                FIRES_CACHE
                    .range_et_disque(disque, FIRES_MAX_ZONES, &zone, data)
                    .await;
            }
            // Échec total : cache inchangé (re-tentative à la prochaine requête).
            None => tracing::warn!("FIRMS : rafraîchissement impossible pour {zone}"),
        }
    });
}

/// Interroge l'API FIRMS (CSV) : zone demandée, VIIRS S-NPP, `days` derniers
/// jours. `None` en cas d'échec. Timeout dédié large : le CSV multi-jours est
/// volumineux en pleine saison des feux, et cet appel tourne en tâche de fond
/// (jamais sur le chemin d'une réponse client).
async fn fetch_fires_days(key: &str, days: u8, area: &str) -> Option<Vec<FirePoint>> {
    let url = format!(
        "https://firms.modaps.eosdis.nasa.gov/api/area/csv/{key}/VIIRS_SNPP_NRT/{area}/{days}"
    );
    let resp = client_http()
        .get(&url)
        .timeout(Duration::from_secs(90))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        tracing::warn!("FIRMS : statut HTTP {}", resp.status());
        return None;
    }
    let text = resp.text().await.ok()?;
    // FIRMS renvoie parfois ses erreurs (clé invalide, quota dépassé…) en 200
    // avec un message texte : sans l'en-tête CSV attendu, c'est un échec.
    if !text.starts_with("latitude") {
        tracing::warn!(
            "FIRMS : réponse inattendue : {:?}",
            text.chars().take(120).collect::<String>()
        );
        return None;
    }
    // Sept jours du monde entier pèsent des mégaoctets : l'analyse se fait
    // sur un fil à part, pas sur la boucle d'événements.
    tokio::task::spawn_blocking(move || parse_firms_csv(&text))
        .await
        .ok()
}

/// Taille d'une cellule de la grille de dédoublonnage (~550 m ≈ 1 pixel
/// VIIRS) : sur 7 jours, un même foyer redonne les mêmes pixels chaque
/// passage ; on n'en garde qu'un par cellule, avec première/dernière détection.
const FIRES_GRID_DEG: f64 = 0.005;
/// Garde-fou sur la taille de la réponse (cellules les plus récentes d'abord).
const FIRES_MAX_POINTS: usize = 4000;

/// La plus ancienne de deux dates, `None` valant « inconnue » et non
/// « très ancienne » : `min` sur des `Option` ferait gagner `None`, et un
/// seul horodatage manquant suffirait à effacer l'âge de tout un foyer.
fn plus_ancienne(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

/// Analyse le CSV FIRMS en filtrant les faux positifs (torchères, brûlages,
/// détections douteuses) : on ne garde que les détections de confiance
/// nominale ou haute avec une puissance radiative (FRP) ≥ 2 MW, puis on
/// dédoublonne par cellule de grille en gardant la détection la plus récente.
///
/// Colonnes VIIRS : latitude,longitude,bright_ti4,scan,track,acq_date,
/// acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight.
fn parse_firms_csv(text: &str) -> Vec<FirePoint> {
    // Cellule → point le plus récent.
    let mut cells: HashMap<(i64, i64), FirePoint> = HashMap::new();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        let (Some(lat), Some(lon)) = (
            cols.first().and_then(|s| s.trim().parse::<f64>().ok()),
            cols.get(1).and_then(|s| s.trim().parse::<f64>().ok()),
        ) else {
            continue;
        };
        // Confiance « l » (low) = détection douteuse → écartée. (MODIS code la
        // confiance en pourcentage ; on écarte < 30 par prudence.)
        match cols.get(9).map(|s| s.trim()) {
            Some(c) if c.eq_ignore_ascii_case("l") || c.eq_ignore_ascii_case("low") => continue,
            Some(c) => {
                if let Ok(pct) = c.parse::<f64>()
                    && pct < 30.0
                {
                    continue;
                }
            }
            None => {}
        }
        // FRP < 2 MW : trop faible pour un feu de forêt (torchères, reflets).
        if let Some(frp) = cols.get(12).and_then(|s| s.trim().parse::<f64>().ok())
            && frp < 2.0
        {
            continue;
        }
        let t = cols
            .get(5)
            .zip(cols.get(6))
            .and_then(|(d, h)| acq_epoch(d.trim(), h.trim()));
        let cell = (
            (lat / FIRES_GRID_DEG).round() as i64,
            (lon / FIRES_GRID_DEG).round() as i64,
        );
        cells
            .entry(cell)
            .and_modify(|p| {
                if t > p.t {
                    // Détection plus récente : la cellule prend sa position.
                    p.lat = lat;
                    p.lon = lon;
                    p.t = t;
                }
                // La première détection ne bouge que vers le passé. `None` est
                // une date inconnue, pas une date ancienne : un seul horodatage
                // manquant ne doit pas effacer l'âge d'un foyer.
                p.t0 = plus_ancienne(p.t0, t);
            })
            .or_insert(FirePoint { lat, lon, t, t0: t });
    }
    let mut out: Vec<FirePoint> = cells.into_values().collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.t));
    if out.len() > FIRES_MAX_POINTS {
        let avant = out.len();
        out = eclaircit(out);
        // Un plafond silencieux se lit comme « tout est là ».
        tracing::info!("FIRMS : {avant} cellules éclaircies à {}", out.len());
    }
    out
}

/// Ramène une liste sous le plafond **sans la trier par ancienneté**.
///
/// Couper les plus vieilles paraissait juste : on garde les foyers actifs. Mais
/// sur le monde entier, l'heure d'une détection dit surtout par où le satellite
/// vient de passer. Les quatre mille plus récentes tombaient donc toutes dans la
/// même bande de longitudes — l'Afrique un soir, l'Asie le lendemain — et
/// l'Europe disparaissait de la carte alors qu'elle brûlait.
///
/// On éclaircit donc dans l'espace : la grille est doublée jusqu'à ce que le
/// compte passe, en gardant la détection la plus récente de chaque maille. Le
/// dessin perd en finesse là où les feux sont denses, et ne perd aucune région.
fn eclaircit(points: Vec<FirePoint>) -> Vec<FirePoint> {
    let mut pas = FIRES_GRID_DEG * 4.0;
    loop {
        let mut mailles: HashMap<(i64, i64), FirePoint> = HashMap::new();
        for p in &points {
            let cle = ((p.lat / pas).floor() as i64, (p.lon / pas).floor() as i64);
            mailles
                .entry(cle)
                .and_modify(|garde| {
                    // La maille garde la détection la plus récente, mais hérite
                    // de la plus ancienne date de début qu'elle a vue : sinon
                    // l'éclaircissement rajeunissait les foyers qu'il fusionne.
                    let debut = plus_ancienne(garde.t0, p.t0);
                    if p.t > garde.t {
                        *garde = p.clone();
                    }
                    garde.t0 = debut;
                })
                .or_insert_with(|| p.clone());
        }
        if mailles.len() <= FIRES_MAX_POINTS || pas > 45.0 {
            let mut out: Vec<FirePoint> = mailles.into_values().collect();
            out.sort_by_key(|p| std::cmp::Reverse(p.t));
            out.truncate(FIRES_MAX_POINTS);
            return out;
        }
        pas *= 2.0;
    }
}

/// Convertit une acquisition FIRMS (`acq_date` = « AAAA-MM-JJ », `acq_time` =
/// « HHMM » UTC, parfois sans zéros de tête) en epoch secondes UTC.
fn acq_epoch(date: &str, time: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let hm: i64 = time.parse().ok()?;
    let (h, min) = (hm / 100, hm % 100);
    if !(0..24).contains(&h) || !(0..60).contains(&min) {
        return None;
    }
    // Jours écoulés depuis l'époque Unix (algorithme « days from civil »).
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3_600 + min * 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_firms_filters_and_merges() {
        let csv = "latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight\n\
            44.5000,-1.2000,330.1,0.4,0.4,2026-07-26,0342,N,VIIRS,h,2.0NRT,290.0,12.5,N\n\
            44.5001,-1.2001,331.0,0.4,0.4,2026-07-28,0142,N,VIIRS,n,2.0NRT,291.0,8.0,N\n\
            45.0000,3.0000,320.0,0.4,0.4,2026-07-28,0142,N,VIIRS,l,2.0NRT,285.0,50.0,N\n\
            45.1000,3.1000,320.0,0.4,0.4,2026-07-28,0142,N,VIIRS,h,2.0NRT,285.0,0.5,N\n\
            46.0000,4.0000,325.0,0.4,0.4,2026-07-28,0142,N,VIIRS,85,2.0NRT,287.0,5.0,N\n\
            46.5000,4.5000,325.0,0.4,0.4,2026-07-28,0142,N,VIIRS,20,2.0NRT,287.0,5.0,N\n";
        let pts = parse_firms_csv(csv);
        // Confiance basse (l / 20 %) et FRP < 2 MW écartées ; les deux
        // premières lignes tombent dans la même cellule → fusionnées.
        assert_eq!(pts.len(), 2);
        let merged = pts
            .iter()
            .find(|p| (p.lat - 44.5001).abs() < 1e-6)
            .expect("cellule fusionnée absente");
        assert_eq!(merged.t, acq_epoch("2026-07-28", "0142"));
        // …et elle garde la **première** des deux dates : c'est ce qui permet
        // de dire « Créé il y a X j ». Sans cela, un foyer vu toute la semaine
        // paraissait né du jour même.
        assert_eq!(merged.t0, acq_epoch("2026-07-26", "0342"));
        assert!(
            merged.t0 < merged.t,
            "le début doit précéder la dernière vue"
        );
        assert!(pts.iter().any(|p| (p.lat - 46.0).abs() < 1e-6));
    }

    /// Une date inconnue n'est pas une date ancienne. `min` sur des `Option`
    /// ferait gagner `None`, et un seul horodatage manquant dans un foyer
    /// suffirait à effacer son âge.
    #[test]
    fn une_date_inconnue_n_efface_pas_l_age_d_un_foyer() {
        assert_eq!(plus_ancienne(Some(10), Some(4)), Some(4));
        assert_eq!(plus_ancienne(Some(10), None), Some(10));
        assert_eq!(plus_ancienne(None, Some(4)), Some(4));
        assert_eq!(plus_ancienne(None, None), None);

        // Sur le CSV : deux lignes de la même cellule, l'ancienne sans heure
        // exploitable. Le foyer garde la date qu'on connaît.
        let csv = "latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight\n\
            44.5000,-1.2000,330.1,0.4,0.4,,,N,VIIRS,h,2.0NRT,290.0,12.5,N\n\
            44.5001,-1.2001,331.0,0.4,0.4,2026-07-28,0142,N,VIIRS,n,2.0NRT,291.0,8.0,N\n";
        let pts = parse_firms_csv(csv);
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].t0, acq_epoch("2026-07-28", "0142"));
    }

    /// L'éclaircissement fusionne des mailles pour tenir sous le plafond. Il
    /// garde la détection la plus récente — mais doit hériter du début le plus
    /// ancien, sinon il rajeunit les foyers qu'il regroupe.
    #[test]
    fn l_eclaircissement_ne_rajeunit_pas_les_foyers() {
        let mut points = Vec::new();
        // Assez de points pour forcer l'éclaircissement, tous très proches :
        // ils finiront dans la même maille.
        for i in 0..(FIRES_MAX_POINTS + 500) {
            points.push(FirePoint {
                lat: 43.0 + (i % 50) as f64 * 0.001,
                lon: 6.0 + (i / 50) as f64 * 0.001,
                t: Some(5_000_000 + i as i64),
                // Un seul point porte un début très ancien : c'est lui que la
                // maille doit retenir.
                t0: Some(if i == 0 { 1_000 } else { 4_000_000 }),
            });
        }
        let out = eclaircit(points);
        assert!(out.len() <= FIRES_MAX_POINTS, "{} points", out.len());
        assert!(
            out.iter().any(|p| p.t0 == Some(1_000)),
            "le début le plus ancien a été perdu à la fusion"
        );
    }

    #[test]
    fn acq_epoch_converts_firms_timestamps() {
        // Valeurs de référence calculées avec `date -u +%s`.
        assert_eq!(acq_epoch("2026-07-28", "0342"), Some(1_785_210_120));
        // FIRMS omet parfois les zéros de tête (« 342 » = 03:42).
        assert_eq!(acq_epoch("2026-07-28", "342"), Some(1_785_210_120));
        // Année bissextile.
        assert_eq!(acq_epoch("2000-02-29", "2359"), Some(951_868_740));
        assert_eq!(acq_epoch("1970-01-01", "0001"), Some(60));
        // Entrées invalides → None.
        assert_eq!(acq_epoch("2026-13-01", "0000"), None);
        assert_eq!(acq_epoch("2026-07-28", "2460"), None);
        assert_eq!(acq_epoch("n/a", "0342"), None);
    }

    /// L'éclaircissement doit garder la **répartition**, pas les plus récentes.
    ///
    /// Couper par ancienneté paraissait juste — on garde les foyers actifs —
    /// mais sur le monde entier l'heure d'une détection dit surtout par où le
    /// satellite vient de passer. Les quatre mille plus récentes tombaient
    /// dans la même bande de longitudes, et l'Europe disparaissait de la carte
    /// alors qu'elle brûlait.
    #[test]
    fn l_eclaircissement_garde_toutes_les_regions() {
        // Deux amas : un très dense et très récent près de l'équateur, un
        // clairsemé et plus ancien en Europe. Le second doit survivre.
        let mut points = Vec::new();
        for i in 0..12_000 {
            points.push(FirePoint {
                lat: 10.0 + (i % 100) as f64 * 0.01,
                lon: 5.0 + (i / 100) as f64 * 0.01,
                t: Some(2_000_000),
                t0: Some(1_900_000),
            });
        }
        for i in 0..40 {
            points.push(FirePoint {
                lat: 45.0 + i as f64 * 0.3,
                lon: 2.0 + i as f64 * 0.2,
                t: Some(1_000_000),
                t0: Some(900_000),
            });
        }

        let out = eclaircit(points);
        assert!(out.len() <= FIRES_MAX_POINTS, "{} points", out.len());
        let europe = out.iter().filter(|p| p.lat >= 44.0).count();
        assert!(europe > 0, "l'Europe a disparu de la carte");
        // L'amas dense reste représenté, mais n'occupe plus toute la place.
        assert!(out.iter().any(|p| p.lat < 20.0), "l'amas dense a disparu");
    }

    /// La zone vient du client : elle entre dans une URL adressée à la NASA,
    /// et elle sert de clé de cache. Une boîte inversée, hors du globe ou mal
    /// formée doit être refusée avant l'un comme avant l'autre.
    #[test]
    fn la_zone_des_feux_est_bornee() {
        // Rien de demandé : le monde entier.
        assert_eq!(fires_zone(None).as_deref(), Some(FIRES_MONDE));

        // Arrondi au quart de degré, vers l'extérieur : deux cartes voisines
        // partagent une clé, et la zone couvre toujours ce qu'on regarde.
        assert_eq!(
            fires_zone(Some("-5.53,41.02,10.04,51.49")).as_deref(),
            Some("-5.75,41,10.25,51.5")
        );

        // Une boîte plus étroite qu'un quart de degré — une ville vue de près —
        // garde une surface. Arrondie au plus proche, elle s'écrasait : les deux
        // bords tombaient sur le même quart, la grille de l'air se réduisait à un
        // point répété, et la carte n'affichait qu'une pastille.
        let serree = fires_zone(Some("-0.62,44.85,-0.55,44.90")).unwrap();
        let n: Vec<f64> = serree.split(',').map(|v| v.parse().unwrap()).collect();
        assert!(n[2] - n[0] >= 0.25, "largeur nulle : {serree}");
        assert!(n[3] - n[1] >= 0.25, "hauteur nulle : {serree}");
        // Et elle contient bien ce qu'on regardait.
        assert!(n[0] <= -0.62 && n[2] >= -0.55 && n[1] <= 44.85 && n[3] >= 44.90);

        // Dézoomée, MapLibre annonce des longitudes bien au-delà de ±180 —
        // jusqu'à ±246 mesuré sur l'app, le monde s'y répétant. Ces boîtes-là
        // sont ramenées dans le globe, et non refusées : les refuser rendait
        // un 400, et la carte restait vide quand on regardait le plus large.
        assert_eq!(
            fires_zone(Some("-246.5,-85.05,246.5,85.05")).as_deref(),
            Some("-180,-85,180,85")
        );
        assert_eq!(
            fires_zone(Some("-5,41,10,120")).as_deref(),
            Some("-5,41,10,85")
        );

        // Reste refusé ce qui ne peut pas venir d'une carte.
        for mauvaise in [
            "10,41,-5,51",    // est avant ouest
            "-5,51,10,41",    // nord avant sud
            "-5,41,10",       // incomplète
            "",               // vide
            "a,b,c,d",        // pas des nombres
            "-5,41,10,51,99", // un champ de trop
        ] {
            assert!(fires_zone(Some(mauvaise)).is_none(), "{mauvaise:?}");
        }
    }

    #[test]
    fn les_champs_absents_ne_sont_pas_expedies() {
        let p = FirePoint {
            lat: 44.5,
            lon: -1.0,
            t: None,
            t0: None,
        };
        assert_eq!(
            serde_json::to_string(&p).unwrap(),
            r#"{"lat":44.5,"lon":-1.0}"#
        );
    }
}
