//! Les positions enregistrées, leur import et export GPX, leurs
//! statistiques, et les calculs de trajet (distance, cap, itinéraire).

use paquets::prelude::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::base::Position;
use crate::commun::{coord_valide, nettoie_label, verifie_coord};
use crate::connexion;
use crate::erreur::{Erreur, Resultat, corps_json, param, requete_query};
use crate::temps_reel::{Evenement, hub};

/// Au plus tant de positions par import GPX.
const MAX_IMPORT: usize = 1000;

pub fn routes(app: &mut App) {
    app.get("/positions", liste);
    app.post("/positions", ajoute);
    app.get("/positions/nearest", plus_proche);
    app.post("/positions/import", import_gpx);
    app.put("/positions/:id", modifie);
    app.delete("/positions/:id", supprime);
    app.get("/positions.gpx", export_gpx);
    app.get("/stats", stats);
    app.post("/route", compute_route);
    app.post("/route/multi", compute_multi_route);
}

/// Données d'une nouvelle position envoyée par le client.
#[derive(Deserialize)]
struct NewPosition {
    lat: f64,
    lon: f64,
    label: String,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
pub struct Coord {
    pub lat: f64,
    pub lon: f64,
}

/// Deux points pour calculer un trajet.
#[derive(Deserialize)]
struct RouteRequest {
    from: Coord,
    to: Coord,
}

/// Résultat d'un trajet : distance et cap.
#[derive(Serialize)]
struct RouteResponse {
    distance_km: f64,
    bearing_deg: f64,
}

/// Trajet à plusieurs points (itinéraire).
#[derive(Deserialize)]
struct MultiRouteRequest {
    points: Vec<Coord>,
    /// Vitesse moyenne pour estimer la durée (défaut : 50 km/h).
    #[serde(default = "default_speed_kmh")]
    speed_kmh: f64,
}

fn default_speed_kmh() -> f64 {
    50.0
}

/// Résultat d'un itinéraire : distance totale, détail par segment, durée estimée.
#[derive(Serialize)]
struct MultiRouteResponse {
    total_km: f64,
    legs_km: Vec<f64>,
    duration_min: f64,
}

/// Point de référence pour chercher la position la plus proche.
#[derive(Deserialize)]
struct NearestQuery {
    lat: f64,
    lon: f64,
}

/// La position la plus proche et sa distance.
#[derive(Serialize)]
struct NearestResponse {
    position: Position,
    distance_km: f64,
}

/// Boîte englobante des positions.
#[derive(Serialize)]
struct BBox {
    min_lat: f64,
    min_lon: f64,
    max_lat: f64,
    max_lon: f64,
}

/// Vue d'ensemble des positions enregistrées.
#[derive(Serialize)]
struct Stats {
    count: usize,
    /// Longueur de l'itinéraire reliant les positions dans l'ordre enregistré.
    total_km: f64,
    bbox: Option<BBox>,
    centroid: Option<Coord>,
}

/// Les positions d'un compte, dans l'ordre d'enregistrement : la liste,
/// l'export et les statistiques ne dépendent plus de l'humeur de Postgres.
async fn positions_de(db: &Db, uid: i32) -> Resultat<Vec<Position>> {
    Ok(Position::query()
        .filter(Position::USER_ID.eq(uid))
        .order_by(Position::ID.asc())
        .all(db)
        .await?)
}

/// GET /positions — les positions de l'utilisateur.
async fn liste(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    Ok(Response::new().json(positions_de(req.db(), qui.user_id).await?))
}

/// POST /positions — ajoute une position pour l'utilisateur.
async fn ajoute(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let p: NewPosition = corps_json(&req).await?;
    verifie_coord(p.lat, p.lon)?;
    let mut position = Position {
        id: 0,
        lat: p.lat,
        lon: p.lon,
        label: nettoie_label(&p.label),
        user_id: qui.user_id,
    };
    position.insert(req.db()).await?;
    hub(&req).diffuse(&Evenement::PositionsChanged);
    Ok(Response::new().json(position))
}

/// PUT /positions/:id — met à jour une position de l'utilisateur. Celle
/// d'un autre se comporte comme une position qui n'existe pas : 404, vide.
async fn modifie(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let id: i32 = param(&req, "id")?;
    let p: NewPosition = corps_json(&req).await?;
    verifie_coord(p.lat, p.lon)?;
    let maj: Vec<Position> = req
        .db()
        .raw(
            "UPDATE positions SET lat = $1, lon = $2, label = $3 \
             WHERE id = $4 AND user_id = $5 RETURNING id, lat, lon, label, user_id",
            &paquets::orm::params![p.lat, p.lon, nettoie_label(&p.label), id, qui.user_id],
        )
        .await?;
    let Some(position) = maj.into_iter().next() else {
        return Err(Erreur::introuvable(""));
    };
    hub(&req).diffuse(&Evenement::PositionsChanged);
    Ok(Response::new().json(position))
}

/// DELETE /positions/:id — supprime une position de l'utilisateur.
async fn supprime(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let id: i32 = param(&req, "id")?;
    let n = Position::delete_many()
        .filter(Position::ID.eq(id))
        .filter(Position::USER_ID.eq(qui.user_id))
        .execute(req.db())
        .await?;
    if n == 0 {
        return Err(Erreur::introuvable(""));
    }
    hub(&req).diffuse(&Evenement::PositionsChanged);
    Ok(Response::new().status(204))
}

/// GET /positions/nearest?lat=&lon= — la position enregistrée la plus proche.
async fn plus_proche(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let q: NearestQuery = requete_query(&req)?;
    verifie_coord(q.lat, q.lon)?;
    let from = Coord {
        lat: q.lat,
        lon: q.lon,
    };
    let items = positions_de(req.db(), qui.user_id).await?;

    // Calcul parallèle (rayon) hors de la boucle d'événements : le bénéfice
    // est réel quand `items` est grand.
    let nearest = tokio::task::spawn_blocking(move || {
        items
            .into_par_iter()
            .map(|p| {
                let d = haversine_km(
                    &from,
                    &Coord {
                        lat: p.lat,
                        lon: p.lon,
                    },
                );
                (p, d)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    })
    .await
    .map_err(Erreur::interne)?;

    match nearest {
        Some((position, distance_km)) => Ok(Response::new().json(NearestResponse {
            position,
            distance_km,
        })),
        None => Err(Erreur::introuvable("aucune position enregistrée")),
    }
}

/// POST /positions/import — importe des positions depuis un document GPX
/// (corps brut, quel que soit son type).
async fn import_gpx(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let corps = req.text().await?;
    let db = req.db();
    let mut crees = Vec::new();
    // On ignore les waypoints invalides et on plafonne à 1000 imports.
    for wpt in parse_gpx_waypoints(&corps)
        .into_iter()
        .filter(|w| coord_valide(w.lat, w.lon))
        .take(MAX_IMPORT)
    {
        let mut position = Position {
            id: 0,
            lat: wpt.lat,
            lon: wpt.lon,
            label: nettoie_label(&wpt.label),
            user_id: qui.user_id,
        };
        position.insert(db).await?;
        crees.push(position);
    }
    if !crees.is_empty() {
        hub(&req).diffuse(&Evenement::PositionsChanged);
    }
    Ok(Response::new().json(crees))
}

/// GET /positions.gpx?token=… — exporte les positions au format GPX.
/// Téléchargé via un lien : le jeton passe dans l'URL, l'en-tête est ignoré.
async fn export_gpx(req: Request) -> Resultat<Response> {
    let jeton = req
        .query("token")
        .map(|t| t.into_owned())
        .ok_or_else(|| Erreur::requete("jeton manquant"))?;
    let qui = connexion::depuis_jeton(req.db(), &jeton).await?;
    let items = positions_de(req.db(), qui.user_id).await?;
    Ok(Response::new()
        .content_type("application/gpx+xml")
        .send(to_gpx(&items)))
}

/// GET /stats — vue d'ensemble des positions de l'utilisateur.
async fn stats(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let items = positions_de(req.db(), qui.user_id).await?;
    // Réductions parallèles (rayon) hors de la boucle d'événements.
    let stats = tokio::task::spawn_blocking(move || compute_stats(&items))
        .await
        .map_err(Erreur::interne)?;
    Ok(Response::new().json(stats))
}

/// POST /route — calcule distance et cap entre deux points.
async fn compute_route(req: Request) -> Resultat<Response> {
    let r: RouteRequest = corps_json(&req).await?;
    verifie_coord(r.from.lat, r.from.lon)?;
    verifie_coord(r.to.lat, r.to.lon)?;
    Ok(Response::new().json(RouteResponse {
        distance_km: haversine_km(&r.from, &r.to),
        bearing_deg: bearing_deg(&r.from, &r.to),
    }))
}

/// POST /route/multi — distance totale d'un itinéraire à plusieurs points.
async fn compute_multi_route(req: Request) -> Resultat<Response> {
    let r: MultiRouteRequest = corps_json(&req).await?;
    if r.points.iter().any(|p| !coord_valide(p.lat, p.lon)) {
        return Err(Erreur::requete("coordonnées invalides"));
    }
    let legs_km = route_legs_km(&r.points);
    let total_km: f64 = legs_km.iter().sum();
    Ok(Response::new().json(MultiRouteResponse {
        duration_min: duration_min(total_km, r.speed_kmh),
        total_km,
        legs_km,
    }))
}

// ---------------------------------------------------------------------------
// Les calculs.
// ---------------------------------------------------------------------------

/// Calcule les statistiques d'un ensemble de positions (parallélisé).
fn compute_stats(items: &[Position]) -> Stats {
    let coords: Vec<Coord> = items
        .par_iter()
        .map(|p| Coord {
            lat: p.lat,
            lon: p.lon,
        })
        .collect();

    // Boîte englobante : réduction parallèle min/max.
    let bbox = coords
        .par_iter()
        .map(|c| BBox {
            min_lat: c.lat,
            min_lon: c.lon,
            max_lat: c.lat,
            max_lon: c.lon,
        })
        .reduce_with(|a, b| BBox {
            min_lat: a.min_lat.min(b.min_lat),
            min_lon: a.min_lon.min(b.min_lon),
            max_lat: a.max_lat.max(b.max_lat),
            max_lon: a.max_lon.max(b.max_lon),
        });

    // Centroïde : sommes parallèles.
    let centroid = if coords.is_empty() {
        None
    } else {
        let n = coords.len() as f64;
        let (slat, slon) = coords
            .par_iter()
            .map(|c| (c.lat, c.lon))
            .reduce(|| (0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1));
        Some(Coord {
            lat: slat / n,
            lon: slon / n,
        })
    };

    // `+ 0.0` normalise un éventuel -0.0 (somme vide) en 0.0.
    let total_km = route_legs_km(&coords).iter().sum::<f64>() + 0.0;

    Stats {
        count: coords.len(),
        total_km,
        bbox,
        centroid,
    }
}

/// Durée en minutes pour parcourir `km` à `speed_kmh` (0 si vitesse invalide).
fn duration_min(km: f64, speed_kmh: f64) -> f64 {
    if speed_kmh > 0.0 {
        km / speed_kmh * 60.0
    } else {
        0.0
    }
}

/// Distance de chaque segment d'un itinéraire (vide si moins de 2 points).
/// Parallélisé : `par_windows` conserve l'ordre des segments.
pub fn route_legs_km(points: &[Coord]) -> Vec<f64> {
    points
        .par_windows(2)
        .map(|w| haversine_km(&w[0], &w[1]))
        .collect()
}

/// Distance en km entre deux coordonnées (formule de Haversine).
pub fn haversine_km(a: &Coord, b: &Coord) -> f64 {
    const R: f64 = 6371.0; // rayon terrestre en km
    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * R * h.sqrt().asin()
}

/// Cap initial en degrés (0 = nord) du point a vers le point b.
fn bearing_deg(a: &Coord, b: &Coord) -> f64 {
    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlon = (b.lon - a.lon).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

// ---------------------------------------------------------------------------
// Le GPX.
// ---------------------------------------------------------------------------

/// Construit un document GPX (waypoints) à partir des positions.
fn to_gpx(positions: &[Position]) -> String {
    use std::fmt::Write;
    // Pré-dimensionné pour éviter les réallocations ; write! écrit directement
    // dans le tampon (pas de String temporaire par waypoint).
    let mut gpx = String::with_capacity(160 + positions.len() * 96);
    gpx.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <gpx version=\"1.1\" creator=\"moncap-gps\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n",
    );
    for p in positions {
        let _ = writeln!(
            gpx,
            "  <wpt lat=\"{}\" lon=\"{}\"><name>{}</name></wpt>",
            p.lat,
            p.lon,
            xml_escape(&p.label),
        );
    }
    gpx.push_str("</gpx>\n");
    gpx
}

/// Échappe les caractères XML spéciaux d'un texte. Renvoie l'entrée telle
/// quelle (sans allocation) quand il n'y a rien à échapper — le cas courant.
fn xml_escape(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains(['&', '<', '>', '"', '\'']) {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Inverse de `xml_escape`.
fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Extrait les waypoints (`<wpt lat lon><name>`) d'un document GPX.
fn parse_gpx_waypoints(xml: &str) -> Vec<NewPosition> {
    let mut out = Vec::new();
    for chunk in xml.split("<wpt").skip(1) {
        let Some(tag_end) = chunk.find('>') else {
            continue;
        };
        let attrs = &chunk[..tag_end];
        let body = &chunk[tag_end + 1..];
        let (Some(lat), Some(lon)) = (attr_f64(attrs, "lat"), attr_f64(attrs, "lon")) else {
            continue;
        };
        let label = between(body, "<name>", "</name>")
            .map(|n| xml_unescape(&n))
            .unwrap_or_else(|| format!("Point {}", out.len() + 1));
        out.push(NewPosition { lat, lon, label });
    }
    out
}

/// Lit un attribut numérique `name="..."` dans un fragment de balise.
fn attr_f64(attrs: &str, name: &str) -> Option<f64> {
    let key = format!("{name}=\"");
    let start = attrs.find(&key)? + key.len();
    let end = attrs[start..].find('"')? + start;
    attrs[start..end].trim().parse().ok()
}

/// Renvoie le texte situé entre `open` et `close`, s'il existe.
fn between(s: &str, open: &str, close: &str) -> Option<String> {
    let start = s.find(open)? + open.len();
    let end = s[start..].find(close)? + start;
    Some(s[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARIS: Coord = Coord {
        lat: 48.8566,
        lon: 2.3522,
    };
    const LYON: Coord = Coord {
        lat: 45.7640,
        lon: 4.8357,
    };

    #[test]
    fn distance_paris_lyon() {
        // Distance réelle ≈ 392 km.
        let d = haversine_km(&PARIS, &LYON);
        assert!((d - 391.5).abs() < 1.0, "distance inattendue: {d}");
    }

    #[test]
    fn distance_to_self_is_zero() {
        assert!(haversine_km(&PARIS, &PARIS) < 1e-9);
    }

    #[test]
    fn bearing_paris_lyon_is_southeast() {
        // Lyon est au sud-est de Paris : cap entre 90° et 180°.
        let b = bearing_deg(&PARIS, &LYON);
        assert!((90.0..180.0).contains(&b), "cap inattendu: {b}");
    }

    #[test]
    fn multi_route_sums_legs() {
        const MARSEILLE: Coord = Coord {
            lat: 43.2965,
            lon: 5.3698,
        };
        let legs = route_legs_km(&[PARIS, LYON, MARSEILLE]);
        assert_eq!(legs.len(), 2);
        let total: f64 = legs.iter().sum();
        // Paris→Lyon (~391) + Lyon→Marseille (~278) ≈ 669 km.
        assert!((total - 669.0).abs() < 5.0, "total inattendu: {total}");
    }

    #[test]
    fn multi_route_single_point_is_empty() {
        assert!(route_legs_km(&[PARIS]).is_empty());
    }

    #[test]
    fn duration_scales_with_speed() {
        // 100 km à 50 km/h = 2 h = 120 min.
        assert!((duration_min(100.0, 50.0) - 120.0).abs() < 1e-9);
        // Vitesse nulle => 0.
        assert_eq!(duration_min(100.0, 0.0), 0.0);
    }

    #[test]
    fn gpx_contains_escaped_waypoints() {
        let positions = vec![Position {
            id: 1,
            lat: 48.0,
            lon: 2.0,
            label: "A & <B>".to_string(),
            user_id: 1,
        }];
        let gpx = to_gpx(&positions);
        assert!(gpx.contains("<wpt lat=\"48\" lon=\"2\">"));
        assert!(gpx.contains("A &amp; &lt;B&gt;"));
        assert!(gpx.trim_end().ends_with("</gpx>"));
    }

    #[test]
    fn gpx_roundtrip_parses_back() {
        let positions = vec![
            Position {
                id: 1,
                lat: 48.8566,
                lon: 2.3522,
                label: "Paris & <Co>".to_string(),
                user_id: 1,
            },
            Position {
                id: 2,
                lat: 45.764,
                lon: 4.8357,
                label: "Lyon".to_string(),
                user_id: 1,
            },
        ];
        let parsed = parse_gpx_waypoints(&to_gpx(&positions));
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].label, "Paris & <Co>");
        assert!((parsed[0].lat - 48.8566).abs() < 1e-9);
        assert!((parsed[1].lon - 4.8357).abs() < 1e-9);
    }

    #[test]
    fn gpx_parse_ignores_invalid_waypoints() {
        // Un wpt sans lat/lon est ignoré ; l'autre est conservé.
        let xml = "<gpx><wpt lon=\"2.0\"><name>X</name></wpt>\
                   <wpt lat=\"1.0\" lon=\"2.0\"></wpt></gpx>";
        let parsed = parse_gpx_waypoints(xml);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].label, "Point 1");
    }

    #[test]
    fn xml_escape_covers_apostrophe_and_borrows_plain_text() {
        assert_eq!(xml_escape("l'été \"x\""), "l&apos;été &quot;x&quot;");
        assert!(matches!(xml_escape("rien"), std::borrow::Cow::Borrowed(_)));
        assert_eq!(xml_unescape(&xml_escape("a&b<c>'\"")), "a&b<c>'\"");
    }

    #[test]
    fn stats_empty_total_is_positive_zero() {
        let s = compute_stats(&[]);
        assert_eq!(s.count, 0);
        assert_eq!(s.total_km, 0.0);
        // Pas de -0.0 dans la sortie JSON.
        assert!(s.total_km.is_sign_positive());
        assert!(s.bbox.is_none() && s.centroid.is_none());
    }

    #[test]
    fn stats_bbox_and_centroid() {
        let items = [
            Position {
                id: 1,
                lat: 48.0,
                lon: 2.0,
                label: String::new(),
                user_id: 1,
            },
            Position {
                id: 2,
                lat: 46.0,
                lon: 4.0,
                label: String::new(),
                user_id: 1,
            },
        ];
        let s = compute_stats(&items);
        assert_eq!(s.count, 2);
        let b = s.bbox.unwrap();
        assert_eq!(
            (b.min_lat, b.min_lon, b.max_lat, b.max_lon),
            (46.0, 2.0, 48.0, 4.0)
        );
        let c = s.centroid.unwrap();
        assert_eq!((c.lat, c.lon), (47.0, 3.0));
        assert!(s.total_km > 200.0);
    }
}

#[cfg(test)]
mod essais {
    use serde_json::{Value, json};

    use crate::essais::{bearer, client_ou_rien, inscrit};

    #[tokio::test]
    async fn positions_persist_and_are_scoped_to_user() {
        let c = client_ou_rien!();
        let (_, jeton) = inscrit(&c, "pos").await;
        let r = c
            .post("/positions")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"lat": 48.8566, "lon": 2.3522, "label": "  Paris  "}))
            .await;
        assert_eq!(r.status(), 200, "{}", r.text());
        let cree: Value = r.json();
        assert_eq!(cree["label"], "Paris");
        assert!(cree.get("user_id").is_none());

        let r = c
            .get("/positions")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        let liste: Vec<Value> = r.json();
        assert!(liste.iter().any(|p| p["label"] == "Paris"));

        // Un autre utilisateur ne voit pas ces positions (isolation par compte).
        let (_, autre) = inscrit(&c, "pos-other").await;
        let r = c
            .get("/positions")
            .header("authorization", &bearer(&autre))
            .await;
        let liste: Vec<Value> = r.json();
        assert!(liste.iter().all(|p| p["label"] != "Paris"));
    }

    #[tokio::test]
    async fn sans_jeton_401_avant_le_corps() {
        let c = client_ou_rien!();
        let r = c.post("/positions").body("pas du json").await;
        assert_eq!(r.status(), 401);
        assert_eq!(r.text(), "authentification requise");
        let r = c.get("/stats").await;
        assert_eq!(r.status(), 401);
    }

    #[tokio::test]
    async fn modifier_et_supprimer_seulement_les_siennes() {
        let c = client_ou_rien!();
        let (_, jeton) = inscrit(&c, "posmaj").await;
        let (_, autre) = inscrit(&c, "posmaj-autre").await;
        let r = c
            .post("/positions")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"lat": 1.0, "lon": 2.0, "label": "A"}))
            .await;
        let id = r.json::<Value>()["id"].as_i64().unwrap();

        // Coordonnées invalides : 400 avant toute recherche.
        let r = c
            .put(&format!("/positions/{id}"))
            .header("authorization", &bearer(&jeton))
            .json(&json!({"lat": 91.0, "lon": 2.0, "label": "B"}))
            .await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.text(), "coordonnées invalides");

        // La position d'autrui n'existe pas : 404, corps vide.
        let r = c
            .put(&format!("/positions/{id}"))
            .header("authorization", &bearer(&autre))
            .json(&json!({"lat": 3.0, "lon": 4.0, "label": "B"}))
            .await;
        assert_eq!(r.status(), 404);
        assert!(r.text().is_empty());
        let r = c
            .delete(&format!("/positions/{id}"))
            .header("authorization", &bearer(&autre))
            .await;
        assert_eq!(r.status(), 404);

        let r = c
            .put(&format!("/positions/{id}"))
            .header("authorization", &bearer(&jeton))
            .json(&json!({"lat": 3.0, "lon": 4.0, "label": "B"}))
            .await;
        assert_eq!(r.status(), 200);
        let v: Value = r.json();
        assert_eq!(v["id"].as_i64(), Some(id));
        assert_eq!(v["lat"], 3.0);
        assert_eq!(v["label"], "B");

        let r = c
            .delete(&format!("/positions/{id}"))
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .delete(&format!("/positions/{id}"))
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 404);
    }

    #[tokio::test]
    async fn la_plus_proche_et_les_stats() {
        let c = client_ou_rien!();
        let (_, jeton) = inscrit(&c, "proche").await;
        let r = c
            .get("/positions/nearest?lat=48.85&lon=2.35")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 404);
        assert_eq!(r.text(), "aucune position enregistrée");
        for (lat, lon, label) in [(48.8566, 2.3522, "Paris"), (45.764, 4.8357, "Lyon")] {
            c.post("/positions")
                .header("authorization", &bearer(&jeton))
                .json(&json!({"lat": lat, "lon": lon, "label": label}))
                .await;
        }
        let r = c
            .get("/positions/nearest?lat=45.7&lon=4.8")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        let v: Value = r.json();
        assert_eq!(v["position"]["label"], "Lyon");
        assert!(v["distance_km"].as_f64().unwrap() < 10.0);
        let r = c
            .get("/positions/nearest?lat=95&lon=4.8")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 400);
        let r = c
            .get("/positions/nearest?lat=45")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 400);

        let r = c
            .get("/stats")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        let s: Value = r.json();
        assert_eq!(s["count"], 2);
        assert!((s["total_km"].as_f64().unwrap() - 391.5).abs() < 1.0);
        assert_eq!(s["bbox"]["max_lat"], 48.8566);
    }

    #[tokio::test]
    async fn import_puis_export_gpx() {
        let c = client_ou_rien!();
        let (_, jeton) = inscrit(&c, "gpx").await;
        let gpx = "<?xml version=\"1.0\"?><gpx>\
                   <wpt lat=\"48.0\" lon=\"2.0\"><name>Chez Paul &amp; Marie</name></wpt>\
                   <wpt lat=\"99\" lon=\"2.0\"><name>Hors du globe</name></wpt>\
                   <wpt lat=\"47.0\" lon=\"3.0\"></wpt></gpx>";
        let r = c
            .post("/positions/import")
            .header("authorization", &bearer(&jeton))
            .header("content-type", "application/gpx+xml")
            .body(gpx)
            .await;
        assert_eq!(r.status(), 200, "{}", r.text());
        let crees: Vec<Value> = r.json();
        assert_eq!(crees.len(), 2);
        assert_eq!(crees[0]["label"], "Chez Paul & Marie");
        assert_eq!(crees[1]["label"], "Point 3");

        let r = c.get(&format!("/positions.gpx?token={jeton}")).await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.header("content-type"), Some("application/gpx+xml"));
        let corps = r.text();
        assert!(corps.contains("<wpt lat=\"48\" lon=\"2\"><name>Chez Paul &amp; Marie</name>"));
        assert!(corps.ends_with("</gpx>\n"));
        // L'export ne lit que le jeton de l'URL.
        let r = c
            .get("/positions.gpx")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 400);
        let r = c.get("/positions.gpx?token=mc_faux").await;
        assert_eq!(r.status(), 401);

        // Un import vide ne crée rien.
        let r = c
            .post("/positions/import")
            .header("authorization", &bearer(&jeton))
            .body("rien")
            .await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.json::<Value>(), json!([]));
    }

    #[tokio::test]
    async fn les_calculs_de_trajet_sans_compte() {
        let c = client_ou_rien!();
        let r = c
            .post("/route")
            .json(&json!({"from": {"lat": 48.8566, "lon": 2.3522}, "to": {"lat": 45.764, "lon": 4.8357}}))
            .await;
        assert_eq!(r.status(), 200);
        let v: Value = r.json();
        assert!((v["distance_km"].as_f64().unwrap() - 391.5).abs() < 1.0);
        let r = c
            .post("/route")
            .json(&json!({"from": {"lat": 100.0, "lon": 0.0}, "to": {"lat": 0.0, "lon": 0.0}}))
            .await;
        assert_eq!(r.status(), 400);
        let r = c
            .post("/route/multi")
            .json(&json!({"points": [{"lat": 0.0, "lon": 0.0}]}))
            .await;
        assert_eq!(r.status(), 200);
        let v: Value = r.json();
        assert_eq!(
            v,
            json!({"total_km": 0.0, "legs_km": [], "duration_min": 0.0})
        );
        let r = c
            .post("/route/multi")
            .json(&json!({"points": [{"lat": 0.0, "lon": 0.0}, {"lat": 0.0, "lon": 1.0}], "speed_kmh": 0.0}))
            .await;
        let v: Value = r.json();
        assert_eq!(v["legs_km"].as_array().unwrap().len(), 1);
        assert_eq!(v["duration_min"], 0.0);
    }
}
