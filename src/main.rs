mod auth;
mod entity;
mod migration;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{FromRef, Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rayon::prelude::*;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectOptions, Database, DatabaseConnection, EntityTrait,
    QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, Any, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use auth::AuthUser;
use entity::{position, search, trip, user};
use migration::Migrator;
use sea_orm_migration::MigratorTrait;

/// Identifiant unique attribué à chaque connexion live et à chaque alerte.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// État partagé : base, canal temps réel, alertes récentes en mémoire.
#[derive(Clone)]
struct AppState {
    db: DatabaseConnection,
    tx: broadcast::Sender<String>,
    alerts: Arc<Mutex<Vec<Alert>>>,
}

impl FromRef<AppState> for DatabaseConnection {
    fn from_ref(s: &AppState) -> Self {
        s.db.clone()
    }
}

impl FromRef<AppState> for broadcast::Sender<String> {
    fn from_ref(s: &AppState) -> Self {
        s.tx.clone()
    }
}

/// Un signalement façon Waze (police, accident, bouchon, danger…).
#[derive(Clone, Serialize, Deserialize)]
struct Alert {
    id: u64,
    category: String,
    lat: f64,
    lon: f64,
    label: String,
    ts: u64,
}

/// Message reçu d'un client via WebSocket.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ClientEvent {
    /// Position GPS en direct de l'utilisateur.
    Live {
        lat: f64,
        lon: f64,
        label: String,
        #[serde(default)]
        avatar: String,
    },
    /// Nouveau signalement.
    Alert {
        category: String,
        lat: f64,
        lon: f64,
        #[serde(default)]
        label: String,
    },
}

/// Message diffusé aux clients via WebSocket.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ServerEvent {
    /// Les positions enregistrées ont changé → le client recharge.
    PositionsChanged,
    /// Position live d'un utilisateur.
    Live {
        id: u64,
        lat: f64,
        lon: f64,
        label: String,
        avatar: String,
    },
    /// Un utilisateur live s'est déconnecté.
    LiveGone { id: u64 },
    /// Un nouveau signalement.
    Alert(Alert),
    /// Instantané des signalements en cours (à la connexion).
    Alerts { alerts: Vec<Alert> },
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Diffuse un événement à tous les clients connectés.
fn broadcast_event(tx: &broadcast::Sender<String>, ev: &ServerEvent) {
    if let Ok(json) = serde_json::to_string(ev) {
        let _ = tx.send(json);
    }
}

/// Limiteur de débit par IP (fenêtre fixe, en mémoire).
struct RateLimiter {
    hits: Mutex<HashMap<String, (u64, u32)>>,
    limit: u32,
    window_secs: u64,
}

impl RateLimiter {
    fn new(limit: u32, window_secs: u64) -> Self {
        Self {
            hits: Mutex::new(HashMap::new()),
            limit,
            window_secs,
        }
    }

    /// Renvoie `true` si la requête est autorisée pour cette clé.
    fn allow(&self, key: &str) -> bool {
        let now = now_ms() / 1000;
        let mut map = self.hits.lock().unwrap();
        // Purge grossière pour éviter une croissance illimitée.
        if map.len() > 50_000 {
            map.retain(|_, (start, _)| now.saturating_sub(*start) < self.window_secs);
        }
        let entry = map.entry(key.to_owned()).or_insert((now, 0));
        if now.saturating_sub(entry.0) >= self.window_secs {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= self.limit
    }
}

/// IP du client : premier élément de `X-Forwarded-For` (Heroku), sinon inconnu.
fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Middleware de limitation de débit basé sur l'IP client.
async fn rate_limit(limiter: Arc<RateLimiter>, req: Request, next: Next) -> Response {
    if limiter.allow(&client_ip(req.headers())) {
        next.run(req).await
    } else {
        (
            StatusCode::TOO_MANY_REQUESTS,
            "trop de requêtes, réessaie plus tard",
        )
            .into_response()
    }
}

/// CORS restreint : autorise `localhost` (dev), `*.herokuapp.com` (prod) et
/// toute origine listée dans `ALLOWED_ORIGINS` (séparées par des virgules).
fn cors_layer() -> CorsLayer {
    let extra: Vec<String> = std::env::var("ALLOWED_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let allow = AllowOrigin::predicate(
        move |origin: &axum::http::HeaderValue, _req: &axum::http::request::Parts| {
            let o = origin.to_str().unwrap_or("");
            o.starts_with("http://localhost")
                || o.starts_with("http://127.0.0.1")
                || o.strip_prefix("https://").is_some_and(|host| {
                    host.split('/')
                        .next()
                        .unwrap_or("")
                        .ends_with(".herokuapp.com")
                })
                || extra.iter().any(|e| e == o)
        },
    );

    CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(allow)
}

/// Données d'une nouvelle position envoyée par le client.
#[derive(Deserialize)]
struct NewPosition {
    lat: f64,
    lon: f64,
    label: String,
}

/// Trajet parcouru envoyé par le client pour être enregistré.
#[derive(Deserialize)]
struct NewTrip {
    #[serde(default)]
    label: String,
    /// Points parcourus (dans l'ordre). La distance est recalculée côté serveur.
    points: Vec<Coord>,
    /// Durée réelle du trajet en minutes (mesurée par le client).
    #[serde(default)]
    duration_min: f64,
}

/// Recherche de destination envoyée par le client pour être mémorisée.
#[derive(Deserialize)]
struct NewSearch {
    name: String,
    #[serde(default)]
    subtitle: String,
    lat: f64,
    lon: f64,
}

/// Nombre maximal de recherches récentes conservées par utilisateur.
const MAX_RECENTS: usize = 12;

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

    // Connexion Postgres. DATABASE_URL est fourni par l'addon Heroku Postgres.
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) => normalize_pg_url(url),
        Err(_) => {
            tracing::warn!(
                "DATABASE_URL non défini. Sur Heroku, ajoute l'addon Heroku Postgres \
                 (Resources ▸ Add-ons ▸ Heroku Postgres). Tentative sur localhost…"
            );
            "postgres://postgres:postgres@localhost:5432/moncap".to_string()
        }
    };
    // Pool borné : Heroku Postgres limite le nombre de connexions (~20 sur les
    // petits plans, partagé entre dynos). On plafonne pour éviter l'épuisement,
    // ajustable via DATABASE_MAX_CONNECTIONS.
    let max_conns = std::env::var("DATABASE_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let mut opt = ConnectOptions::new(db_url);
    opt.max_connections(max_conns)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(8))
        .idle_timeout(std::time::Duration::from_secs(300))
        .sqlx_logging(false);
    let db = Database::connect(opt).await.expect(
        "connexion Postgres impossible — vérifie que l'addon Heroku Postgres est ajouté \
         (DATABASE_URL doit être défini)",
    );
    Migrator::up(&db, None)
        .await
        .expect("migrations impossibles");

    // Canal de diffusion temps réel (WebSocket).
    let (tx, _rx) = broadcast::channel::<String>(256);
    let state = AppState {
        db,
        tx,
        alerts: Arc::new(Mutex::new(Vec::new())),
    };

    // Front web statique (React + Bun, voir web/). Servi en repli : toute
    // requête qui ne correspond pas à l'API renvoie un fichier de web/dist,
    // avec index.html en repli (SPA).
    let web = ServeDir::new("web/dist").not_found_service(ServeFile::new("web/dist/index.html"));

    // Limiteurs : général (par IP) + strict sur l'authentification (anti brute-force).
    let general = Arc::new(RateLimiter::new(300, 60));
    let auth_rl = Arc::new(RateLimiter::new(20, 60));

    // Routes d'auth, sous limite stricte.
    let auth_routes = Router::new()
        .route("/auth/signup", post(signup))
        .route("/auth/login", post(login))
        .layer(middleware::from_fn(move |req, next| {
            rate_limit(auth_rl.clone(), req, next)
        }));

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
        .route("/trips", get(list_trips).post(add_trip))
        .route("/trips/:id", axum::routing::delete(delete_trip))
        .route("/searches", get(list_searches).post(add_search).delete(clear_searches))
        .route("/searches/:id", axum::routing::delete(delete_search))
        .route("/ws", get(ws_handler))
        .merge(auth_routes)
        .fallback_service(web)
        .layer(middleware::from_fn(move |req, next| {
            rate_limit(general.clone(), req, next)
        }))
        .layer(TraceLayer::new_for_http())
        // Compresse les réponses (gzip/brotli) selon Accept-Encoding — utile
        // surtout pour les payloads texte (listes JSON, export GPX).
        .layer(CompressionLayer::new())
        .layer(cors_layer())
        .with_state(state);

    // Heroku impose le port via la variable d'environnement PORT.
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("moncap-gps écoute sur http://{addr}");
    axum::serve(listener, app).await.unwrap();
}

/// Identifiants envoyés à l'inscription / connexion.
#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

/// Réponse d'authentification : jeton + nom d'utilisateur.
#[derive(Serialize)]
struct AuthResponse {
    token: String,
    username: String,
}

/// POST /auth/signup — crée un compte et renvoie un jeton.
async fn signup(
    State(db): State<DatabaseConnection>,
    Json(c): Json<Credentials>,
) -> Result<Json<AuthResponse>, AppError> {
    let username = c.username.trim().to_string();
    if username.chars().count() < 3 || c.password.len() < 6 {
        return Err(AppError::BadRequest(
            "nom (≥3) et mot de passe (≥6) requis".into(),
        ));
    }
    if user::Entity::find()
        .filter(user::Column::Username.eq(&username))
        .one(&db)
        .await?
        .is_some()
    {
        return Err(AppError::BadRequest("nom d'utilisateur déjà pris".into()));
    }
    let saved = user::ActiveModel {
        username: Set(username.clone()),
        password_hash: Set(auth::hash_password(&c.password)?),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    Ok(Json(AuthResponse {
        token: auth::make_token(saved.id)?,
        username,
    }))
}

/// POST /auth/login — vérifie les identifiants et renvoie un jeton.
async fn login(
    State(db): State<DatabaseConnection>,
    Json(c): Json<Credentials>,
) -> Result<Json<AuthResponse>, AppError> {
    let username = c.username.trim().to_string();
    let found = user::Entity::find()
        .filter(user::Column::Username.eq(&username))
        .one(&db)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !auth::verify_password(&c.password, &found.password_hash) {
        return Err(AppError::Unauthorized);
    }
    Ok(Json(AuthResponse {
        token: auth::make_token(found.id)?,
        username,
    }))
}

/// Ajoute `sslmode=require` pour les bases distantes (ex. Heroku Postgres
/// impose TLS), sauf si déjà précisé ou si la base est locale.
fn normalize_pg_url(url: String) -> String {
    let local = url.contains("@localhost") || url.contains("@127.0.0.1");
    if local || url.contains("sslmode=") {
        url
    } else {
        let sep = if url.contains('?') { '&' } else { '?' };
        format!("{url}{sep}sslmode=require")
    }
}

/// GET /positions — renvoie les positions de l'utilisateur.
async fn list_positions(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<position::Model>>, AppError> {
    let items = position::Entity::find()
        .filter(position::Column::UserId.eq(uid))
        .all(&db)
        .await?;
    Ok(Json(items))
}

/// POST /positions — ajoute une position pour l'utilisateur.
async fn add_position(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
    State(tx): State<broadcast::Sender<String>>,
    Json(input): Json<NewPosition>,
) -> Result<Json<position::Model>, AppError> {
    check_coord(input.lat, input.lon)?;
    let saved = position::ActiveModel {
        lat: Set(input.lat),
        lon: Set(input.lon),
        label: Set(clean_label(&input.label)),
        user_id: Set(uid),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    broadcast_event(&tx, &ServerEvent::PositionsChanged);
    Ok(Json(saved))
}

/// PUT /positions/:id — met à jour une position de l'utilisateur.
async fn update_position(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
    State(tx): State<broadcast::Sender<String>>,
    Path(id): Path<i32>,
    Json(input): Json<NewPosition>,
) -> Result<Response, AppError> {
    check_coord(input.lat, input.lon)?;
    let owned = position::Entity::find_by_id(id)
        .filter(position::Column::UserId.eq(uid))
        .one(&db)
        .await?;
    if owned.is_none() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let updated = position::ActiveModel {
        id: Set(id),
        lat: Set(input.lat),
        lon: Set(input.lon),
        label: Set(clean_label(&input.label)),
        user_id: Set(uid),
    }
    .update(&db)
    .await?;
    broadcast_event(&tx, &ServerEvent::PositionsChanged);
    Ok(Json(updated).into_response())
}

/// DELETE /positions/:id — supprime une position de l'utilisateur.
async fn delete_position(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
    State(tx): State<broadcast::Sender<String>>,
    Path(id): Path<i32>,
) -> Result<StatusCode, AppError> {
    let res = position::Entity::delete_many()
        .filter(position::Column::Id.eq(id))
        .filter(position::Column::UserId.eq(uid))
        .exec(&db)
        .await?;
    if res.rows_affected == 0 {
        Ok(StatusCode::NOT_FOUND)
    } else {
        broadcast_event(&tx, &ServerEvent::PositionsChanged);
        Ok(StatusCode::NO_CONTENT)
    }
}

/// GET /trips — renvoie les trajets enregistrés de l'utilisateur (récents d'abord).
async fn list_trips(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<trip::Model>>, AppError> {
    let items = trip::Entity::find()
        .filter(trip::Column::UserId.eq(uid))
        .order_by_desc(trip::Column::CreatedAt)
        .all(&db)
        .await?;
    Ok(Json(items))
}

/// POST /trips — enregistre un trajet parcouru par l'utilisateur.
async fn add_trip(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
    Json(input): Json<NewTrip>,
) -> Result<Json<trip::Model>, AppError> {
    // Au moins deux points valides pour former un tracé.
    let points: Vec<Coord> = input
        .points
        .into_iter()
        .filter(|p| valid_coord(p.lat, p.lon))
        .take(10_000)
        .collect();
    if points.len() < 2 {
        return Err(AppError::BadRequest(
            "un trajet requiert au moins deux points".into(),
        ));
    }
    let distance_km = route_legs_km(&points).iter().sum::<f64>() + 0.0;
    let label = {
        let l = clean_label(&input.label);
        if l.is_empty() {
            format!("Trajet de {:.1} km", distance_km)
        } else {
            l
        }
    };
    let polyline = encode_polyline(&points);
    let saved = trip::ActiveModel {
        label: Set(label),
        distance_km: Set(distance_km),
        duration_min: Set(input.duration_min.max(0.0)),
        polyline: Set(polyline),
        created_at: Set((now_ms() / 1000) as i64),
        user_id: Set(uid),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    Ok(Json(saved))
}

/// DELETE /trips/:id — supprime un trajet de l'utilisateur.
async fn delete_trip(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
    Path(id): Path<i32>,
) -> Result<StatusCode, AppError> {
    let res = trip::Entity::delete_many()
        .filter(trip::Column::Id.eq(id))
        .filter(trip::Column::UserId.eq(uid))
        .exec(&db)
        .await?;
    if res.rows_affected == 0 {
        Ok(StatusCode::NOT_FOUND)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

/// Encode un tracé en `lat,lon;lat,lon;…` (6 décimales, ~0,1 m de précision).
fn encode_polyline(points: &[Coord]) -> String {
    points
        .iter()
        .map(|c| format!("{:.6},{:.6}", c.lat, c.lon))
        .collect::<Vec<_>>()
        .join(";")
}

/// GET /searches — recherches de destination récentes de l'utilisateur.
async fn list_searches(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<search::Model>>, AppError> {
    let items = search::Entity::find()
        .filter(search::Column::UserId.eq(uid))
        .order_by_desc(search::Column::CreatedAt)
        .order_by_desc(search::Column::Id)
        .all(&db)
        .await?;
    Ok(Json(items))
}

/// POST /searches — mémorise une recherche (dédoublonne et plafonne la liste).
async fn add_search(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
    Json(input): Json<NewSearch>,
) -> Result<Json<search::Model>, AppError> {
    check_coord(input.lat, input.lon)?;
    let name = clean_label(&input.name);
    if name.is_empty() {
        return Err(AppError::BadRequest("nom de recherche vide".into()));
    }
    let saved = search::ActiveModel {
        name: Set(name),
        subtitle: Set(clean_label(&input.subtitle)),
        lat: Set(input.lat),
        lon: Set(input.lon),
        created_at: Set((now_ms() / 1000) as i64),
        user_id: Set(uid),
        ..Default::default()
    }
    .insert(&db)
    .await?;

    // Dédoublonne (même lieu) et ne garde que les MAX_RECENTS plus récentes.
    let all = search::Entity::find()
        .filter(search::Column::UserId.eq(uid))
        .order_by_desc(search::Column::CreatedAt)
        .order_by_desc(search::Column::Id)
        .all(&db)
        .await?;
    let prune = recents_to_prune(&all, saved.id, MAX_RECENTS);
    if !prune.is_empty() {
        search::Entity::delete_many()
            .filter(search::Column::UserId.eq(uid))
            .filter(search::Column::Id.is_in(prune))
            .exec(&db)
            .await?;
    }
    Ok(Json(saved))
}

/// DELETE /searches/:id — supprime une recherche récente de l'utilisateur.
async fn delete_search(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
    Path(id): Path<i32>,
) -> Result<StatusCode, AppError> {
    let res = search::Entity::delete_many()
        .filter(search::Column::Id.eq(id))
        .filter(search::Column::UserId.eq(uid))
        .exec(&db)
        .await?;
    if res.rows_affected == 0 {
        Ok(StatusCode::NOT_FOUND)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

/// DELETE /searches — efface toutes les recherches récentes de l'utilisateur.
async fn clear_searches(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<StatusCode, AppError> {
    search::Entity::delete_many()
        .filter(search::Column::UserId.eq(uid))
        .exec(&db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Clé de dédoublonnage d'une recherche : nom normalisé (minuscules, espaces
/// internes réduits) + coordonnées arrondies (~10 m). Deux recherches de même
/// clé désignent le même lieu.
fn recent_key(name: &str, lat: f64, lon: f64) -> String {
    let norm = name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    format!("{}|{:.4},{:.4}", norm, lat, lon)
}

/// À partir des recherches d'un utilisateur (les plus récentes d'abord),
/// renvoie les identifiants à supprimer : doublons d'un lieu déjà vu et tout
/// ce qui dépasse `max`. L'entrée `keep_id` (celle qu'on vient d'insérer) est
/// toujours conservée et fait autorité : un doublon plus ancien du même lieu
/// est élagué, quel que soit l'ordre des égalités de date.
fn recents_to_prune(items: &[search::Model], keep_id: i32, max: usize) -> Vec<i32> {
    let mut seen = std::collections::HashSet::new();
    let mut kept = 0usize;
    // On amorce avec l'entrée conservée pour que ses doublons plus anciens
    // soient élagués même si l'ordre par date les place avant elle.
    if let Some(k) = items.iter().find(|i| i.id == keep_id) {
        seen.insert(recent_key(&k.name, k.lat, k.lon));
        kept = 1;
    }
    let mut prune = Vec::new();
    for it in items {
        if it.id == keep_id {
            continue;
        }
        let key = recent_key(&it.name, it.lat, it.lon);
        // Doublon d'un lieu déjà retenu, ou dépassement du plafond → à élaguer.
        if !seen.insert(key) || kept >= max {
            prune.push(it.id);
        } else {
            kept += 1;
        }
    }
    prune
}

/// POST /positions/import — importe des positions depuis un document GPX.
async fn import_gpx(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
    State(tx): State<broadcast::Sender<String>>,
    body: String,
) -> Result<Json<Vec<position::Model>>, AppError> {
    let mut created = Vec::new();
    // On ignore les waypoints invalides et on plafonne à 1000 imports.
    for wpt in parse_gpx_waypoints(&body)
        .into_iter()
        .filter(|w| valid_coord(w.lat, w.lon))
        .take(1000)
    {
        let saved = position::ActiveModel {
            lat: Set(wpt.lat),
            lon: Set(wpt.lon),
            label: Set(clean_label(&wpt.label)),
            user_id: Set(uid),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        created.push(saved);
    }
    if !created.is_empty() {
        broadcast_event(&tx, &ServerEvent::PositionsChanged);
    }
    Ok(Json(created))
}

/// Jeton passé en query du WebSocket (les navigateurs ne peuvent pas poser
/// d'en-tête Authorization sur une connexion WebSocket).
#[derive(Deserialize)]
struct WsAuth {
    token: String,
}

/// GET /ws?token=... — connexion WebSocket temps réel (positions, live, alertes).
async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(q): Query<WsAuth>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    if auth::verify_token(&q.token).is_none() {
        return Err(AppError::Unauthorized);
    }
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state)))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let mut rx = state.tx.subscribe();

    // Instantané des signalements en cours, envoyé à la connexion.
    let snapshot = ServerEvent::Alerts {
        alerts: prune_alerts(&state.alerts),
    };
    if let Ok(json) = serde_json::to_string(&snapshot) {
        let _ = socket.send(Message::Text(json)).await;
    }

    loop {
        tokio::select! {
            res = rx.recv() => match res {
                Ok(msg) => {
                    if socket.send(Message::Text(msg)).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Text(t))) => handle_client_msg(&state, id, &t),
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(_)) => break,
                _ => {}
            },
        }
    }

    // L'utilisateur live disparaît pour les autres.
    broadcast_event(&state.tx, &ServerEvent::LiveGone { id });
}

/// Traite un message reçu d'un client et le rediffuse.
fn handle_client_msg(state: &AppState, id: u64, text: &str) {
    let Ok(ev) = serde_json::from_str::<ClientEvent>(text) else {
        return;
    };
    match ev {
        ClientEvent::Live {
            lat,
            lon,
            label,
            avatar,
        } => {
            if !valid_coord(lat, lon) {
                return;
            }
            broadcast_event(
                &state.tx,
                &ServerEvent::Live {
                    id,
                    lat,
                    lon,
                    label: clean_label(&label),
                    avatar: clean_label(&avatar),
                },
            );
        }
        ClientEvent::Alert {
            category,
            lat,
            lon,
            label,
        } => {
            if !valid_coord(lat, lon) {
                return;
            }
            let alert = Alert {
                id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
                category: clean_label(&category),
                lat,
                lon,
                label: clean_label(&label),
                ts: now_ms(),
            };
            {
                let mut alerts = state.alerts.lock().unwrap();
                alerts.push(alert.clone());
                let len = alerts.len();
                if len > 100 {
                    alerts.drain(0..len - 100);
                }
            }
            broadcast_event(&state.tx, &ServerEvent::Alert(alert));
        }
    }
}

/// Retire les signalements de plus de 30 min et renvoie ceux restants.
fn prune_alerts(alerts: &Arc<Mutex<Vec<Alert>>>) -> Vec<Alert> {
    let cutoff = now_ms().saturating_sub(30 * 60 * 1000);
    let mut a = alerts.lock().unwrap();
    a.retain(|x| x.ts >= cutoff);
    a.clone()
}

/// GET /stats — vue d'ensemble des positions de l'utilisateur.
async fn stats(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Stats>, AppError> {
    let items = position::Entity::find()
        .filter(position::Column::UserId.eq(uid))
        .all(&db)
        .await?;

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

    // `+ 0.0` normalise un éventuel -0.0 (somme vide) en 0.0.
    let total_km = route_legs_km(&coords).iter().sum::<f64>() + 0.0;

    Stats {
        count: coords.len(),
        total_km,
        bbox,
        centroid,
    }
}

/// GET /positions/nearest?lat=&lon= — la position enregistrée la plus proche.
async fn nearest_position(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
    Query(q): Query<NearestQuery>,
) -> Result<Json<NearestResponse>, AppError> {
    check_coord(q.lat, q.lon)?;
    let from = Coord {
        lat: q.lat,
        lon: q.lon,
    };
    let items = position::Entity::find()
        .filter(position::Column::UserId.eq(uid))
        .all(&db)
        .await?;

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
async fn compute_route(Json(req): Json<RouteRequest>) -> Result<Json<RouteResponse>, AppError> {
    check_coord(req.from.lat, req.from.lon)?;
    check_coord(req.to.lat, req.to.lon)?;
    Ok(Json(RouteResponse {
        distance_km: haversine_km(&req.from, &req.to),
        bearing_deg: bearing_deg(&req.from, &req.to),
    }))
}

/// POST /route/multi — distance totale d'un itinéraire à plusieurs points.
async fn compute_multi_route(
    Json(req): Json<MultiRouteRequest>,
) -> Result<Json<MultiRouteResponse>, AppError> {
    if req.points.iter().any(|p| !valid_coord(p.lat, p.lon)) {
        return Err(AppError::BadRequest("coordonnées invalides".into()));
    }
    let legs_km = route_legs_km(&req.points);
    let total_km: f64 = legs_km.iter().sum();
    Ok(Json(MultiRouteResponse {
        duration_min: duration_min(total_km, req.speed_kmh),
        total_km,
        legs_km,
    }))
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
async fn export_gpx(
    State(db): State<DatabaseConnection>,
    Query(q): Query<WsAuth>,
) -> Result<Response, AppError> {
    // Téléchargé via un lien : le jeton passe en query (?token=...).
    let uid = auth::verify_token(&q.token).ok_or(AppError::Unauthorized)?;
    let items = position::Entity::find()
        .filter(position::Column::UserId.eq(uid))
        .all(&db)
        .await?;
    let body = to_gpx(&items);
    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/gpx+xml")],
        body,
    )
        .into_response())
}

/// Construit un document GPX (waypoints) à partir des positions.
fn to_gpx(positions: &[position::Model]) -> String {
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
pub(crate) enum AppError {
    #[error("erreur base de données: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("non autorisé")]
    Unauthorized,
    #[error("erreur interne")]
    Internal,
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
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            AppError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "authentification requise").into_response()
            }
            AppError::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, "erreur interne").into_response()
            }
        }
    }
}

/// Longueur maximale d'un libellé.
const MAX_LABEL: usize = 120;

/// Vrai si les coordonnées sont finies et dans les plages valides.
fn valid_coord(lat: f64, lon: f64) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
}

/// Nettoie un libellé : trim + troncature à MAX_LABEL caractères.
fn clean_label(s: &str) -> String {
    s.trim().chars().take(MAX_LABEL).collect()
}

/// Valide des coordonnées, renvoie 400 sinon.
fn check_coord(lat: f64, lon: f64) -> Result<(), AppError> {
    if valid_coord(lat, lon) {
        Ok(())
    } else {
        Err(AppError::BadRequest("coordonnées invalides".into()))
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
            position::Model {
                id: 1,
                lat: 48.8566,
                lon: 2.3522,
                label: "Paris & <Co>".to_string(),
                user_id: 1,
            },
            position::Model {
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
    fn rate_limiter_blocks_over_limit() {
        let rl = RateLimiter::new(3, 60);
        assert!(rl.allow("1.2.3.4"));
        assert!(rl.allow("1.2.3.4"));
        assert!(rl.allow("1.2.3.4"));
        assert!(!rl.allow("1.2.3.4")); // 4e dépasse la limite
        assert!(rl.allow("9.9.9.9")); // autre IP : indépendante
    }

    #[test]
    fn coord_validation() {
        assert!(valid_coord(48.85, 2.35));
        assert!(valid_coord(-90.0, 180.0));
        assert!(!valid_coord(91.0, 0.0));
        assert!(!valid_coord(0.0, 200.0));
        assert!(!valid_coord(f64::NAN, 0.0));
        assert!(!valid_coord(0.0, f64::INFINITY));
    }

    #[test]
    fn label_is_trimmed_and_capped() {
        assert_eq!(clean_label("  Paris  "), "Paris");
        assert_eq!(clean_label(&"a".repeat(500)).chars().count(), MAX_LABEL);
    }

    #[test]
    fn stats_empty_total_is_positive_zero() {
        let s = compute_stats(&[]);
        assert_eq!(s.count, 0);
        assert_eq!(s.total_km, 0.0);
        // Pas de -0.0 dans la sortie JSON.
        assert!(s.total_km.is_sign_positive());
    }

    #[test]
    fn pg_url_adds_sslmode_for_remote_only() {
        // Distante sans sslmode → ajout.
        assert_eq!(
            normalize_pg_url("postgres://u:p@ec2.amazonaws.com:5432/d".into()),
            "postgres://u:p@ec2.amazonaws.com:5432/d?sslmode=require"
        );
        // Avec query existante → on ajoute avec &.
        assert!(
            normalize_pg_url("postgres://u:p@host/d?x=1".into()).ends_with("?x=1&sslmode=require")
        );
        // Locale → inchangée.
        assert_eq!(
            normalize_pg_url("postgres://postgres@localhost:5432/moncap".into()),
            "postgres://postgres@localhost:5432/moncap"
        );
        // sslmode déjà présent → inchangée.
        assert_eq!(
            normalize_pg_url("postgres://u@host/d?sslmode=disable".into()),
            "postgres://u@host/d?sslmode=disable"
        );
    }

    fn mk_search(id: i32, name: &str, lat: f64, lon: f64) -> search::Model {
        search::Model {
            id,
            name: name.to_string(),
            subtitle: String::new(),
            lat,
            lon,
            created_at: id as i64,
            user_id: 1,
        }
    }

    #[test]
    fn recents_prune_removes_duplicate_place() {
        // Plus récentes d'abord ; id=5 vient d'être inséré (keep_id).
        let items = vec![
            mk_search(5, "124 Rue Billaudel", 44.83, -0.57),
            mk_search(4, "Gare", 44.82, -0.55),
            mk_search(3, " 124  RUE  billaudel ", 44.83, -0.57), // doublon (casse + espaces)
            mk_search(2, "Parc", 44.84, -0.58),
        ];
        // Le doublon plus ancien (id=3) est supprimé ; le neuf (id=5) reste.
        assert_eq!(recents_to_prune(&items, 5, 12), vec![3]);
    }

    #[test]
    fn recents_prune_caps_to_max() {
        let items = vec![
            mk_search(3, "A", 1.0, 1.0),
            mk_search(2, "B", 2.0, 2.0),
            mk_search(1, "C", 3.0, 3.0),
        ];
        // max=2 : on garde le neuf (id=3) + un autre, on élague le plus ancien.
        assert_eq!(recents_to_prune(&items, 3, 2), vec![1]);
    }

    #[test]
    fn recents_prune_keeps_new_entry_even_if_old_duplicate() {
        // L'entrée fraîche (keep_id=2) est un doublon d'une plus ancienne (id=1).
        let items = vec![
            mk_search(2, "Lieu", 10.0, 10.0),
            mk_search(1, "lieu", 10.0, 10.0),
        ];
        // C'est l'ancienne (id=1) qui part, pas la nouvelle.
        assert_eq!(recents_to_prune(&items, 2, 12), vec![1]);
    }

    #[test]
    fn recents_prune_nothing_when_all_unique_under_cap() {
        let items = vec![
            mk_search(3, "A", 1.0, 1.0),
            mk_search(2, "B", 2.0, 2.0),
            mk_search(1, "C", 3.0, 3.0),
        ];
        assert!(recents_to_prune(&items, 3, 12).is_empty());
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
