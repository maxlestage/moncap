use std::sync::{Arc, Mutex};

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

/// Une position GPS partagée par les clients.
#[derive(Clone, Serialize, Deserialize)]
struct Position {
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

/// État partagé : la liste des positions enregistrées.
type Db = Arc<Mutex<Vec<Position>>>;

#[tokio::main]
async fn main() {
    let db: Db = Arc::new(Mutex::new(Vec::new()));

    // Routes volontairement minimales.
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/positions", get(list_positions).post(add_position))
        .route("/route", post(compute_route))
        .layer(CorsLayer::permissive())
        .with_state(db);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    println!("moncap-gps écoute sur http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

/// GET /positions — renvoie toutes les positions.
async fn list_positions(State(db): State<Db>) -> Json<Vec<Position>> {
    Json(db.lock().unwrap().clone())
}

/// POST /positions — ajoute une position.
async fn add_position(State(db): State<Db>, Json(pos): Json<Position>) -> Json<Position> {
    db.lock().unwrap().push(pos.clone());
    Json(pos)
}

/// POST /route — calcule distance et cap entre deux points.
async fn compute_route(Json(req): Json<RouteRequest>) -> Json<RouteResponse> {
    Json(RouteResponse {
        distance_km: haversine_km(&req.from, &req.to),
        bearing_deg: bearing_deg(&req.from, &req.to),
    })
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
