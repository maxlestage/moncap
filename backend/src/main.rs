mod entity;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

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

#[derive(Deserialize)]
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

#[tokio::main]
async fn main() {
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
        .route("/positions/:id", axum::routing::delete(delete_position))
        .route("/positions/nearest", get(nearest_position))
        .route("/positions.gpx", get(export_gpx))
        .route("/route", post(compute_route))
        .route("/route/multi", post(compute_multi_route))
        .layer(CorsLayer::permissive())
        .with_state(db);

    // Heroku impose le port via la variable d'environnement PORT.
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("moncap-gps écoute sur http://{addr}");
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

/// GET /positions/nearest?lat=&lon= — la position enregistrée la plus proche.
async fn nearest_position(
    State(db): State<DatabaseConnection>,
    Query(q): Query<NearestQuery>,
) -> Result<Json<NearestResponse>, AppError> {
    let from = Coord {
        lat: q.lat,
        lon: q.lon,
    };
    let nearest = position::Entity::find()
        .all(&db)
        .await?
        .into_iter()
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
        .min_by(|a, b| a.1.total_cmp(&b.1));

    match nearest {
        Some((position, distance_km)) => Ok(Json(NearestResponse {
            position,
            distance_km,
        })),
        None => Err(AppError(sea_orm::DbErr::Custom(
            "aucune position enregistrée".to_string(),
        ))),
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

/// Distance de chaque segment d'un itinéraire (vide si moins de 2 points).
fn route_legs_km(points: &[Coord]) -> Vec<f64> {
    points
        .windows(2)
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

/// Erreur convertie en réponse HTTP 500.
struct AppError(sea_orm::DbErr);

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self {
        AppError(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()).into_response()
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
}
