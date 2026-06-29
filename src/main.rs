mod entity;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rayon::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use entity::position;

/// Données d'une nouvelle position envoyée par le client.
#[derive(Deserialize)]
struct NewPosition {
    lat: f64,
    lon: f64,
    label: String,
}

/// Deux points pour calculer un trajet.
#[derive(Deserialize)]
struct RouteRequest {
    from: Coord,
    to: Coord,
}

#[derive(Deserialize, Serialize, Clone)]
struct Coord {
    lat: f64,
    lon: f64,
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
    position: position::Model,
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

#[tokio::main]
async fn main() {
    // Logs structurés (niveau via RUST_LOG, défaut: info).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    // Connexion Postgres. Configurable via DATABASE_URL.
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/moncap".to_string());
    let db = Database::connect(&db_url)
        .await
        .expect("connexion Postgres impossible");
    ensure_schema(&db)
        .await
        .expect("création du schéma impossible");

    // Routes volontairement minimales.
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/positions", get(list_positions).post(add_position))
        .route(
            "/positions/:id",
            axum::routing::put(update_position).delete(delete_position),
        )
        .route("/positions/nearest", get(nearest_position))
        .route("/positions/import", post(import_gpx))
        .route("/positions.gpx", get(export_gpx))
        .route("/stats", get(stats))
        .route("/route", post(compute_route))
        .route("/route/multi", post(compute_multi_route))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(db);

    // Heroku impose le port via la variable d'environnement PORT.
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("moncap-gps écoute sur http://{addr}");
    axum::serve(listener, app).await.unwrap();
}

/// Crée la table `positions` si elle n'existe pas.
async fn ensure_schema(db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS positions (\
            id SERIAL PRIMARY KEY,\
            lat DOUBLE PRECISION NOT NULL,\
            lon DOUBLE PRECISION NOT NULL,\
            label TEXT NOT NULL\
        )",
    )
    .await?;
    Ok(())
}

/// GET /positions — renvoie toutes les positions.
async fn list_positions(
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<position::Model>>, AppError> {
    let items = position::Entity::find().all(&db).await?;
    Ok(Json(items))
}

/// POST /positions — ajoute une position.
async fn add_position(
    State(db): State<DatabaseConnection>,
    Json(input): Json<NewPosition>,
) -> Result<Json<position::Model>, AppError> {
    let saved = position::ActiveModel {
        lat: Set(input.lat),
        lon: Set(input.lon),
        label: Set(input.label),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    Ok(Json(saved))
}

/// PUT /positions/:id — met à jour une position (renommer/déplacer).
async fn update_position(
    State(db): State<DatabaseConnection>,
    Path(id): Path<i32>,
    Json(input): Json<NewPosition>,
) -> Result<Response, AppError> {
    if position::Entity::find_by_id(id).one(&db).await?.is_none() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let updated = position::ActiveModel {
        id: Set(id),
        lat: Set(input.lat),
        lon: Set(input.lon),
        label: Set(input.label),
    }
    .update(&db)
    .await?;
    Ok(Json(updated).into_response())
}

/// DELETE /positions/:id — supprime une position.
async fn delete_position(
    State(db): State<DatabaseConnection>,
    Path(id): Path<i32>,
) -> Result<StatusCode, AppError> {
    let res = position::Entity::delete_by_id(id).exec(&db).await?;
    if res.rows_affected == 0 {
        Ok(StatusCode::NOT_FOUND)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

/// POST /positions/import — importe des positions depuis un document GPX.
async fn import_gpx(
    State(db): State<DatabaseConnection>,
    body: String,
) -> Result<Json<Vec<position::Model>>, AppError> {
    let mut created = Vec::new();
    for wpt in parse_gpx_waypoints(&body) {
        let saved = position::ActiveModel {
            lat: Set(wpt.lat),
            lon: Set(wpt.lon),
            label: Set(wpt.label),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        created.push(saved);
    }
    Ok(Json(created))
}

/// GET /stats — vue d'ensemble des positions enregistrées.
async fn stats(State(db): State<DatabaseConnection>) -> Result<Json<Stats>, AppError> {
    let items = position::Entity::find().all(&db).await?;

    // Réductions parallèles (rayon) hors du runtime async.
    let stats = tokio::task::spawn_blocking(move || compute_stats(&items))
        .await
        .expect("tâche de calcul interrompue");

    Ok(Json(stats))
}

/// Calcule les statistiques d'un ensemble de positions (parallélisé).
fn compute_stats(items: &[position::Model]) -> Stats {
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

    Stats {
        count: coords.len(),
        total_km: route_legs_km(&coords).iter().sum(),
        bbox,
        centroid,
    }
}

/// GET /positions/nearest?lat=&lon= — la position enregistrée la plus proche.
async fn nearest_position(
    State(db): State<DatabaseConnection>,
    Query(q): Query<NearestQuery>,
) -> Result<Json<NearestResponse>, AppError> {
    let from = Coord {
        lat: q.lat,
        lon: q.lon,
    };
    let items = position::Entity::find().all(&db).await?;

    // Calcul parallèle (rayon) sur le pool dédié via spawn_blocking, pour ne
    // pas bloquer le runtime async. Bénéfice réel quand `items` est grand.
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
    .expect("tâche de calcul interrompue");

    match nearest {
        Some((position, distance_km)) => Ok(Json(NearestResponse {
            position,
            distance_km,
        })),
        None => Err(AppError::NotFound(
            "aucune position enregistrée".to_string(),
        )),
    }
}

/// POST /route — calcule distance et cap entre deux points.
async fn compute_route(Json(req): Json<RouteRequest>) -> Json<RouteResponse> {
    Json(RouteResponse {
        distance_km: haversine_km(&req.from, &req.to),
        bearing_deg: bearing_deg(&req.from, &req.to),
    })
}

/// POST /route/multi — distance totale d'un itinéraire à plusieurs points.
async fn compute_multi_route(Json(req): Json<MultiRouteRequest>) -> Json<MultiRouteResponse> {
    let legs_km = route_legs_km(&req.points);
    let total_km: f64 = legs_km.iter().sum();
    Json(MultiRouteResponse {
        duration_min: duration_min(total_km, req.speed_kmh),
        total_km,
        legs_km,
    })
}

/// Durée en minutes pour parcourir `km` à `speed_kmh` (0 si vitesse invalide).
fn duration_min(km: f64, speed_kmh: f64) -> f64 {
    if speed_kmh > 0.0 {
        km / speed_kmh * 60.0
    } else {
        0.0
    }
}

/// GET /positions.gpx — exporte les positions au format GPX.
async fn export_gpx(State(db): State<DatabaseConnection>) -> Result<Response, AppError> {
    let items = position::Entity::find().all(&db).await?;
    let body = to_gpx(&items);
    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/gpx+xml")],
        body,
    )
        .into_response())
}

/// Construit un document GPX (waypoints) à partir des positions.
fn to_gpx(positions: &[position::Model]) -> String {
    let mut gpx = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <gpx version=\"1.1\" creator=\"moncap-gps\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n",
    );
    for p in positions {
        gpx.push_str(&format!(
            "  <wpt lat=\"{}\" lon=\"{}\"><name>{}</name></wpt>\n",
            p.lat,
            p.lon,
            xml_escape(&p.label),
        ));
    }
    gpx.push_str("</gpx>\n");
    gpx
}

/// Échappe les caractères XML spéciaux d'un texte.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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

/// Distance de chaque segment d'un itinéraire (vide si moins de 2 points).
/// Parallélisé : `par_windows` conserve l'ordre des segments.
fn route_legs_km(points: &[Coord]) -> Vec<f64> {
    points
        .par_windows(2)
        .map(|w| haversine_km(&w[0], &w[1]))
        .collect()
}

/// Distance en km entre deux coordonnées (formule de Haversine).
fn haversine_km(a: &Coord, b: &Coord) -> f64 {
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

/// Erreurs de l'API converties en réponses HTTP.
#[derive(thiserror::Error, Debug)]
enum AppError {
    #[error("erreur base de données: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    NotFound(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            // Les détails internes ne sont pas exposés au client.
            AppError::Database(err) => {
                tracing::error!("erreur base de données: {err}");
                (StatusCode::INTERNAL_SERVER_ERROR, "erreur interne").into_response()
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg).into_response(),
        }
    }
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
        let positions = vec![position::Model {
            id: 1,
            lat: 48.0,
            lon: 2.0,
            label: "A & <B>".to_string(),
        }];
        let gpx = to_gpx(&positions);
        assert!(gpx.contains("<wpt lat=\"48\" lon=\"2\">"));
        assert!(gpx.contains("A &amp; &lt;B&gt;"));
        assert!(gpx.trim_end().ends_with("</gpx>"));
    }

    #[test]
    fn gpx_roundtrip_parses_back() {
        let positions = vec![
            position::Model {
                id: 1,
                lat: 48.8566,
                lon: 2.3522,
                label: "Paris & <Co>".to_string(),
            },
            position::Model {
                id: 2,
                lat: 45.764,
                lon: 4.8357,
                label: "Lyon".to_string(),
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
}
