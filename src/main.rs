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
    FromQueryResult, PaginatorTrait, QueryFilter, QueryOrder, Set,
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
use entity::{alert, position, search, trip, user};
use migration::Migrator;
use sea_orm_migration::MigratorTrait;

/// Identifiant unique attribué à chaque connexion live.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Durée de vie d'un signalement (prolongeable par confirmation).
const ALERT_TTL_SECS: i64 = 30 * 60;
/// Prolongation apportée par un vote « toujours là ».
const ALERT_EXTEND_SECS: i64 = 10 * 60;
/// Durée de vie maximale d'un signalement, même très confirmé.
const ALERT_MAX_SECS: i64 = 2 * 60 * 60;

/// État partagé : base et canal temps réel.
#[derive(Clone)]
struct AppState {
    db: DatabaseConnection,
    tx: broadcast::Sender<String>,
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

/// Un signalement façon Waze (police, accident, bouchon, danger…), tel
/// qu'échangé avec les clients.
#[derive(Clone, Serialize, Deserialize)]
struct Alert {
    id: i64,
    category: String,
    lat: f64,
    lon: f64,
    label: String,
    ts: u64,
    #[serde(default)]
    confirms: i32,
    #[serde(default)]
    denies: i32,
}

/// Convertit un signalement stocké en base vers le format client.
fn wire_alert(m: &alert::Model) -> Alert {
    Alert {
        id: m.id as i64,
        category: m.category.clone(),
        lat: m.lat,
        lon: m.lon,
        label: m.label.clone(),
        ts: (m.created_at as u64) * 1000,
        confirms: m.confirms,
        denies: m.denies,
    }
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
    /// Un signalement nouveau ou mis à jour (votes).
    Alert(Alert),
    /// Un signalement supprimé (expiré ou infirmé par les votes).
    AlertGone { id: i64 },
    /// Instantané des signalements en cours (à la connexion).
    Alerts { alerts: Vec<Alert> },
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Diffuse un événement à tous les clients connectés — de cette instance,
/// et des autres via le pont Redis s'il est actif.
fn broadcast_event(tx: &broadcast::Sender<String>, ev: &ServerEvent) {
    if let Ok(json) = serde_json::to_string(ev) {
        let _ = tx.send(json.clone());
        if let Some(publisher) = REDIS_PUB.get() {
            let _ = publisher.send(json);
        }
    }
}

/// Canal vers la tâche de publication Redis (None = mono-instance).
static REDIS_PUB: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<String>> =
    std::sync::OnceLock::new();

/// Canal Redis pub/sub partagé par toutes les instances.
const REDIS_CHANNEL: &str = "moncap:events";

/// URL Redis : préfère REDIS_TLS_URL, sinon REDIS_URL. Heroku Redis utilise un
/// certificat auto-signé → on suffixe `#insecure` (recommandation Heroku pour
/// redis-rs) pour désactiver la vérification du certificat, pas le TLS.
fn redis_url() -> Option<String> {
    let url = std::env::var("REDIS_TLS_URL")
        .or_else(|_| std::env::var("REDIS_URL"))
        .ok()?;
    if url.starts_with("rediss://") && !url.contains('#') {
        Some(format!("{url}#insecure"))
    } else {
        Some(url)
    }
}

/// Démarre le pont Redis pub/sub (si REDIS_URL est défini) : les événements
/// émis ici sont publiés vers les autres instances, et ceux des autres
/// instances sont rediffusés aux clients locaux. Anti-écho par identifiant
/// d'instance ; reconnexion automatique.
fn start_redis_bridge(tx: broadcast::Sender<String>) {
    let Some(url) = redis_url() else {
        tracing::info!("Redis non configuré : temps réel mono-instance");
        return;
    };
    let client = match redis::Client::open(url) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Redis ignoré (URL invalide) : {e}");
            return;
        }
    };
    let instance = format!("{}-{}", std::process::id(), now_ms());
    let (pub_tx, mut pub_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let _ = REDIS_PUB.set(pub_tx);

    // Publication : événements locaux → Redis (enveloppe "instance|json").
    let pub_client = client.clone();
    let pub_instance = instance.clone();
    tokio::spawn(async move {
        loop {
            match pub_client.get_multiplexed_async_connection().await {
                Ok(mut conn) => {
                    while let Some(json) = pub_rx.recv().await {
                        let payload = format!("{pub_instance}|{json}");
                        let sent: Result<(), _> = redis::cmd("PUBLISH")
                            .arg(REDIS_CHANNEL)
                            .arg(&payload)
                            .query_async(&mut conn)
                            .await;
                        if sent.is_err() {
                            break; // connexion perdue → on retentera
                        }
                    }
                }
                Err(e) => tracing::warn!("Redis (publication) indisponible : {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });

    // Abonnement : événements des autres instances → clients locaux.
    tokio::spawn(async move {
        use futures_util::StreamExt;
        loop {
            match client.get_async_pubsub().await {
                Ok(mut pubsub) => {
                    if pubsub.subscribe(REDIS_CHANNEL).await.is_ok() {
                        tracing::info!("Pont Redis pub/sub actif (multi-instances)");
                        let mut stream = pubsub.on_message();
                        while let Some(msg) = stream.next().await {
                            let Ok(payload) = msg.get_payload::<String>() else {
                                continue;
                            };
                            if let Some((src, json)) = payload.split_once('|') {
                                if src != instance && !json.is_empty() {
                                    let _ = tx.send(json.to_string());
                                }
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("Redis (abonnement) indisponible : {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });
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

/// Construit le routeur complet de l'API (routes, limiteurs de débit, couches
/// compression/CORS/trace). Extrait de `main` pour être réutilisable tel quel
/// par les tests d'intégration.
fn build_app(state: AppState) -> Router {
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
    Router::new()
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
        .route(
            "/searches",
            get(list_searches).post(add_search).delete(clear_searches),
        )
        .route("/searches/:id", axum::routing::delete(delete_search))
        .route("/alerts", get(list_alerts))
        .route("/alerts/:id/vote", post(vote_alert))
        .route("/account", get(account_info).delete(delete_account))
        .route("/leaderboard", get(leaderboard))
        .route("/privacy", get(privacy_page))
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
        .with_state(state)
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
    // Pont Redis pub/sub (optionnel) : partage le temps réel entre instances.
    start_redis_bridge(tx.clone());
    let state = AppState { db, tx };

    let app = build_app(state);

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
    // Espaces superflus retirés, mais la casse est conservée : l'auth est
    // sensible à la casse (« Max » ≠ « max »).
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
    // Correspondance exacte : la casse compte (« Max » ≠ « max »).
    let username = c.username.trim();
    let found = user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(&db)
        .await?;
    match found {
        Some(u) if auth::verify_password(&c.password, &u.password_hash) => Ok(Json(AuthResponse {
            token: auth::make_token(u.id)?,
            username: u.username,
        })),
        Some(_) => Err(AppError::Unauthorized),
        None => {
            // Compte inexistant : on vérifie quand même une empreinte factice
            // pour ne pas révéler l'absence du compte par le temps de réponse.
            auth::dummy_verify(&c.password);
            Err(AppError::Unauthorized)
        }
    }
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
    let norm = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
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
    let Some(uid) = auth::verify_token(&q.token) else {
        return Err(AppError::Unauthorized);
    };
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, uid)))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, uid: i32) {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let mut rx = state.tx.subscribe();

    // Instantané des signalements en cours (persistés), envoyé à la connexion.
    let snapshot = ServerEvent::Alerts {
        alerts: active_alerts(&state.db).await.unwrap_or_default(),
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
                Some(Ok(Message::Text(t))) => handle_client_msg(&state, id, uid, &t).await,
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
async fn handle_client_msg(state: &AppState, id: u64, uid: i32, text: &str) {
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
            let now = (now_ms() / 1000) as i64;
            // Anti-abus : au plus 3 signalements par 2 minutes et 10 actifs
            // par utilisateur (silencieusement ignoré au-delà).
            let recent = alert::Entity::find()
                .filter(alert::Column::UserId.eq(uid))
                .filter(alert::Column::CreatedAt.gt(now - 120))
                .count(&state.db)
                .await
                .unwrap_or(0);
            let active = alert::Entity::find()
                .filter(alert::Column::UserId.eq(uid))
                .count(&state.db)
                .await
                .unwrap_or(0);
            if recent >= 3 || active >= 10 {
                return;
            }
            // Persisté en base : survit aux redémarrages et visible par les
            // clients qui se connectent plus tard.
            let saved = alert::ActiveModel {
                category: Set(clean_label(&category)),
                label: Set(clean_label(&label)),
                lat: Set(lat),
                lon: Set(lon),
                created_at: Set(now),
                expires_at: Set(now + ALERT_TTL_SECS),
                confirms: Set(0),
                denies: Set(0),
                user_id: Set(uid),
                ..Default::default()
            }
            .insert(&state.db)
            .await;
            if let Ok(saved) = saved {
                broadcast_event(&state.tx, &ServerEvent::Alert(wire_alert(&saved)));
            }
        }
    }
}

/// Signalements actifs : purge les expirés puis renvoie les restants.
async fn active_alerts(db: &DatabaseConnection) -> Result<Vec<Alert>, sea_orm::DbErr> {
    let now = (now_ms() / 1000) as i64;
    alert::Entity::delete_many()
        .filter(alert::Column::ExpiresAt.lte(now))
        .exec(db)
        .await?;
    let items = alert::Entity::find()
        .order_by_desc(alert::Column::CreatedAt)
        .all(db)
        .await?;
    Ok(items.iter().map(wire_alert).collect())
}

/// GET /alerts — signalements en cours (authentifié).
async fn list_alerts(
    AuthUser(_uid): AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<Alert>>, AppError> {
    Ok(Json(active_alerts(&db).await?))
}

/// Vote sur un signalement : « toujours là » (up) ou « plus là ».
#[derive(Deserialize)]
struct AlertVote {
    up: bool,
}

/// POST /alerts/:id/vote — confirme (prolonge) ou infirme (peut supprimer).
async fn vote_alert(
    AuthUser(_uid): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(v): Json<AlertVote>,
) -> Result<StatusCode, AppError> {
    let Some(found) = alert::Entity::find_by_id(id).one(&state.db).await? else {
        return Ok(StatusCode::NOT_FOUND);
    };
    let now = (now_ms() / 1000) as i64;
    if v.up {
        // Confirmé : on prolonge, plafonné par rapport à la création.
        let extended =
            (found.expires_at.max(now) + ALERT_EXTEND_SECS).min(found.created_at + ALERT_MAX_SECS);
        let updated = alert::ActiveModel {
            id: Set(found.id),
            confirms: Set(found.confirms + 1),
            expires_at: Set(extended),
            ..Default::default()
        }
        .update(&state.db)
        .await?;
        broadcast_event(&state.tx, &ServerEvent::Alert(wire_alert(&updated)));
    } else {
        let denies = found.denies + 1;
        if denies >= 2 && denies > found.confirms {
            // Infirmé par la communauté : on supprime.
            alert::Entity::delete_by_id(found.id)
                .exec(&state.db)
                .await?;
            broadcast_event(
                &state.tx,
                &ServerEvent::AlertGone {
                    id: found.id as i64,
                },
            );
        } else {
            let updated = alert::ActiveModel {
                id: Set(found.id),
                denies: Set(denies),
                ..Default::default()
            }
            .update(&state.db)
            .await?;
            broadcast_event(&state.tx, &ServerEvent::Alert(wire_alert(&updated)));
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Vue d'ensemble du compte : compteurs et points (gamification légère).
#[derive(Serialize)]
struct AccountInfo {
    username: String,
    points: i64,
    alerts: u64,
    trips: u64,
    positions: u64,
    searches: u64,
}

/// Barème des points : signaler rapporte le plus (c'est ce qui aide les autres).
fn score(alerts: u64, trips: u64, positions: u64, searches: u64) -> i64 {
    (alerts * 10 + trips * 5 + positions * 2 + searches) as i64
}

/// GET /account — infos du compte et points de contribution.
async fn account_info(
    AuthUser(uid): AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<AccountInfo>, AppError> {
    let found = user::Entity::find_by_id(uid)
        .one(&db)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let alerts = alert::Entity::find()
        .filter(alert::Column::UserId.eq(uid))
        .count(&db)
        .await?;
    let trips = trip::Entity::find()
        .filter(trip::Column::UserId.eq(uid))
        .count(&db)
        .await?;
    let positions = position::Entity::find()
        .filter(position::Column::UserId.eq(uid))
        .count(&db)
        .await?;
    let searches = search::Entity::find()
        .filter(search::Column::UserId.eq(uid))
        .count(&db)
        .await?;
    Ok(Json(AccountInfo {
        username: found.username,
        points: score(alerts, trips, positions, searches),
        alerts,
        trips,
        positions,
        searches,
    }))
}

/// DELETE /account — supprime le compte et TOUTES ses données (RGPD /
/// exigence App Store). Les signalements supprimés sont notifiés en direct.
async fn delete_account(
    AuthUser(uid): AuthUser,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    let db = &state.db;
    // Signalements : on prévient les clients connectés avant de supprimer.
    let my_alerts = alert::Entity::find()
        .filter(alert::Column::UserId.eq(uid))
        .all(db)
        .await?;
    alert::Entity::delete_many()
        .filter(alert::Column::UserId.eq(uid))
        .exec(db)
        .await?;
    for a in &my_alerts {
        broadcast_event(&state.tx, &ServerEvent::AlertGone { id: a.id as i64 });
    }
    position::Entity::delete_many()
        .filter(position::Column::UserId.eq(uid))
        .exec(db)
        .await?;
    trip::Entity::delete_many()
        .filter(trip::Column::UserId.eq(uid))
        .exec(db)
        .await?;
    search::Entity::delete_many()
        .filter(search::Column::UserId.eq(uid))
        .exec(db)
        .await?;
    user::Entity::delete_by_id(uid).exec(db).await?;
    broadcast_event(&state.tx, &ServerEvent::PositionsChanged);
    Ok(StatusCode::NO_CONTENT)
}

/// Une ligne du classement des contributeurs.
#[derive(Serialize)]
struct LeaderEntry {
    name: String,
    points: i64,
}

#[derive(sea_orm::FromQueryResult)]
struct LeaderRow {
    username: String,
    alerts: i64,
    trips: i64,
    positions: i64,
    searches: i64,
}

/// Pseudonymise un e-mail pour l'affichage public : « maxle… ».
fn public_name(username: &str) -> String {
    let local = username.split('@').next().unwrap_or(username);
    let prefix: String = local.chars().take(5).collect();
    if local.chars().count() > 5 {
        format!("{prefix}…")
    } else {
        prefix
    }
}

/// GET /leaderboard — top 10 des contributeurs (noms pseudonymisés).
async fn leaderboard(
    AuthUser(_uid): AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<LeaderEntry>>, AppError> {
    let rows = LeaderRow::find_by_statement(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        "SELECT u.username, \
           (SELECT count(*) FROM alerts a WHERE a.user_id = u.id) AS alerts, \
           (SELECT count(*) FROM trips t WHERE t.user_id = u.id) AS trips, \
           (SELECT count(*) FROM positions p WHERE p.user_id = u.id) AS positions, \
           (SELECT count(*) FROM searches s WHERE s.user_id = u.id) AS searches \
         FROM users u",
    ))
    .all(&db)
    .await?;
    let mut entries: Vec<LeaderEntry> = rows
        .iter()
        .map(|r| LeaderEntry {
            name: public_name(&r.username),
            points: score(
                r.alerts as u64,
                r.trips as u64,
                r.positions as u64,
                r.searches as u64,
            ),
        })
        .collect();
    entries.sort_by_key(|b| std::cmp::Reverse(b.points));
    entries.truncate(10);
    Ok(Json(entries))
}

/// GET /privacy — politique de confidentialité (publique, exigée pour la
/// distribution App Store).
async fn privacy_page() -> axum::response::Html<&'static str> {
    axum::response::Html(PRIVACY_HTML)
}

const PRIVACY_HTML: &str = r#"<!doctype html>
<html lang="fr"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>MonCap GPS — Politique de confidentialité</title>
<style>body{font-family:-apple-system,sans-serif;max-width:720px;margin:2rem auto;padding:0 1rem;line-height:1.6;color:#1a1d21}h1{font-size:1.5rem}h2{font-size:1.1rem;margin-top:1.5rem}</style>
</head><body>
<h1>🛰️ MonCap GPS — Politique de confidentialité</h1>
<h2>Données collectées</h2>
<p>MonCap GPS stocke : votre adresse e-mail de connexion, un mot de passe haché
(Argon2, jamais en clair), vos positions enregistrées, vos trajets parcourus,
vos recherches récentes et vos signalements (avec votes).</p>
<h2>Position en temps réel</h2>
<p>Pendant l'utilisation, votre position et le nom associé à votre compte sont
diffusés en direct aux autres utilisateurs connectés. Cette diffusion n'est pas
conservée. Le bouton « Stop partage » l'interrompt à tout moment.</p>
<h2>Services tiers</h2>
<p>Le calcul d'itinéraires et la recherche de lieux utilisent Apple Plans ;
le vélo utilise BRouter (OpenStreetMap) ; le dénivelé utilise open-meteo ;
les limitations de vitesse utilisent Overpass (OpenStreetMap). Seules des
coordonnées géographiques leur sont transmises, jamais votre identité.</p>
<h2>Conservation et suppression</h2>
<p>Vos données sont conservées tant que votre compte existe. Vous pouvez
supprimer définitivement votre compte et l'ensemble de vos données depuis
l'application : menu ▸ Compte ▸ « Supprimer mon compte ».</p>
<h2>Partage</h2>
<p>Vos données ne sont ni vendues ni transmises à des tiers.</p>
</body></html>"#;

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
    fn public_name_masks_email() {
        assert_eq!(public_name("maxlestage@icloud.com"), "maxle…");
        assert_eq!(public_name("bob@x.fr"), "bob");
        assert_eq!(public_name("sanschien"), "sansc…");
    }

    #[test]
    fn score_weights_contributions() {
        // 1 signalement (10) + 1 trajet (5) + 2 positions (4) + 1 recherche (1).
        assert_eq!(score(1, 1, 2, 1), 20);
        assert_eq!(score(0, 0, 0, 0), 0);
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

/// Tests d'intégration des handlers HTTP sur une vraie base Postgres.
///
/// Ils s'exécutent uniquement si `TEST_DATABASE_URL` est défini (base jetable,
/// ex. service Postgres du CI) ; sinon chaque test se termine sans rien faire,
/// pour que `cargo test` reste vert dans un environnement sans base.
/// La requête passe par `oneshot` (aucun port réseau ouvert).
#[cfg(test)]
mod integration {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Sérialise l'application des migrations : elles ne sont appliquées qu'une
    /// fois pour toute la suite (le drapeau passe à `true` après le premier
    /// test). Chaque test garde en revanche sa **propre** connexion : un pool
    /// sqlx est lié au runtime qui l'a créé, et chaque `#[tokio::test]` a le
    /// sien — partager le pool casserait les tests suivants.
    static MIGRATE_ONCE: tokio::sync::Mutex<bool> = tokio::sync::Mutex::const_new(false);

    /// Application de test **et** sa connexion (pour insérer des données
    /// directement, ex. un signalement). `None` si `TEST_DATABASE_URL` absent.
    async fn test_app_db() -> Option<(Router, DatabaseConnection)> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let db = Database::connect(&url)
            .await
            .expect("connexion base de test");
        {
            let mut migrated = MIGRATE_ONCE.lock().await;
            if !*migrated {
                Migrator::up(&db, None)
                    .await
                    .expect("migrations base de test");
                *migrated = true;
            }
        }
        let (tx, _rx) = broadcast::channel::<String>(16);
        Some((build_app(AppState { db: db.clone(), tx }), db))
    }

    /// Application de test, ou `None` si `TEST_DATABASE_URL` n'est pas défini.
    async fn test_app() -> Option<Router> {
        Some(test_app_db().await?.0)
    }

    /// Identifiant unique par test (base partagée) — le préfixe isole les cas.
    fn uniq(prefix: &str) -> String {
        format!("{prefix}-{}@test.moncap", now_ms())
    }

    fn json_req(
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: serde_json::Value,
    ) -> Request<Body> {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    fn get_req(uri: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method("GET").uri(uri);
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::empty()).unwrap()
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }

    /// Inscrit un utilisateur et renvoie son jeton.
    async fn signup(app: &Router, user: &str) -> String {
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/auth/signup",
                None,
                serde_json::json!({"username": user, "password": "s3cret!"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "signup doit réussir");
        body_json(resp).await["token"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn signup_then_authed_and_401_without_token() {
        let Some(app) = test_app().await else { return };
        let token = signup(&app, &uniq("flow")).await;

        // Requête authentifiée : OK avec le jeton.
        let resp = app
            .clone()
            .oneshot(get_req("/account", Some(&token)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Sans jeton : 401.
        let resp = app
            .clone()
            .oneshot(get_req("/account", None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn login_is_case_sensitive() {
        let Some(app) = test_app().await else { return };
        let user = uniq("Case"); // contient une majuscule
        signup(&app, &user).await;

        // Même casse : OK.
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/auth/login",
                None,
                serde_json::json!({"username": user, "password": "s3cret!"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Casse différente : 401 (auth sensible à la casse).
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/auth/login",
                None,
                serde_json::json!({"username": user.to_lowercase(), "password": "s3cret!"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_password_rejected() {
        let Some(app) = test_app().await else { return };
        let user = uniq("pwd");
        signup(&app, &user).await;
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/auth/login",
                None,
                serde_json::json!({"username": user, "password": "mauvais"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn duplicate_signup_rejected() {
        let Some(app) = test_app().await else { return };
        let user = uniq("dup");
        signup(&app, &user).await;
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/auth/signup",
                None,
                serde_json::json!({"username": user, "password": "s3cret!"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn positions_persist_and_are_scoped_to_user() {
        let Some(app) = test_app().await else { return };
        let token = signup(&app, &uniq("pos")).await;

        // Ajoute une position.
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/positions",
                Some(&token),
                serde_json::json!({"lat": 48.8566, "lon": 2.3522, "label": "Paris"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // La liste de l'utilisateur contient bien la position ajoutée.
        let resp = app
            .clone()
            .oneshot(get_req("/positions", Some(&token)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let list = body_json(resp).await;
        let arr = list.as_array().expect("liste de positions");
        assert!(
            arr.iter().any(|p| p["label"] == "Paris"),
            "la position ajoutée doit apparaître dans la liste de l'utilisateur"
        );

        // Un autre utilisateur ne voit pas ces positions (isolation par compte).
        let other = signup(&app, &uniq("pos-other")).await;
        let resp = app
            .clone()
            .oneshot(get_req("/positions", Some(&other)))
            .await
            .unwrap();
        let arr = body_json(resp).await;
        assert!(
            arr.as_array()
                .unwrap()
                .iter()
                .all(|p| p["label"] != "Paris"),
            "les positions ne doivent pas fuiter vers un autre compte"
        );
    }

    #[tokio::test]
    async fn account_deletion_removes_data_and_frees_email() {
        let Some(app) = test_app().await else { return };
        let email = uniq("del");
        let token = signup(&app, &email).await;

        // Un peu de données rattachées au compte.
        app.clone()
            .oneshot(json_req(
                "POST",
                "/positions",
                Some(&token),
                serde_json::json!({"lat": 1.0, "lon": 2.0, "label": "temp"}),
            ))
            .await
            .unwrap();

        // Suppression : 204.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/account")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // L'ancien jeton ne donne plus accès (utilisateur supprimé) → 401.
        let resp = app
            .clone()
            .oneshot(get_req("/account", Some(&token)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // L'e-mail est de nouveau libre : ré-inscription possible.
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/auth/signup",
                None,
                serde_json::json!({"username": email, "password": "s3cret!"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn alert_up_vote_increments_confirms() {
        let Some((app, db)) = test_app_db().await else {
            return;
        };
        let token = signup(&app, &uniq("voter")).await;

        // Les signalements sont créés via WebSocket : on en insère un en base
        // directement pour tester le handler de vote HTTP.
        let now = (now_ms() / 1000) as i64;
        let created = alert::ActiveModel {
            category: Set("police".into()),
            label: Set(String::new()),
            lat: Set(48.85),
            lon: Set(2.35),
            created_at: Set(now),
            expires_at: Set(now + 1800),
            confirms: Set(0),
            denies: Set(0),
            user_id: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        // Vote « toujours là ».
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                &format!("/alerts/{}/vote", created.id),
                Some(&token),
                serde_json::json!({"up": true}),
            ))
            .await
            .unwrap();
        assert!(resp.status().is_success());

        // Le compteur de confirmations a augmenté.
        let updated = alert::Entity::find_by_id(created.id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.confirms, 1);
    }

    #[tokio::test]
    async fn trips_round_trip() {
        let Some(app) = test_app().await else { return };
        let token = signup(&app, &uniq("trip")).await;

        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/trips",
                Some(&token),
                serde_json::json!({
                    "label": "Balade",
                    "points": [{"lat": 48.85, "lon": 2.35}, {"lat": 48.86, "lon": 2.36}],
                    "duration_min": 12.0
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app
            .clone()
            .oneshot(get_req("/trips", Some(&token)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let list = body_json(resp).await;
        assert!(
            list.as_array()
                .unwrap()
                .iter()
                .any(|t| t["label"] == "Balade"),
            "le trajet enregistré doit apparaître dans l'historique"
        );
    }

    #[tokio::test]
    async fn leaderboard_returns_array() {
        let Some(app) = test_app().await else { return };
        let token = signup(&app, &uniq("leader")).await;
        // Un peu d'activité pour marquer des points.
        app.clone()
            .oneshot(json_req(
                "POST",
                "/positions",
                Some(&token),
                serde_json::json!({"lat": 1.0, "lon": 2.0, "label": "P"}),
            ))
            .await
            .unwrap();

        let resp = app
            .clone()
            .oneshot(get_req("/leaderboard", Some(&token)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_json(resp).await.is_array());
    }
}
