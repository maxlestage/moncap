//! La base Postgres : les tables, leurs modèles, et les migrations.
//!
//! Le schéma historique a été créé par les migrations SeaORM (`seaql_migrations`,
//! `m0001_init` … `m0009_trips_nature`). On n'y touche pas : la migration 1
//! ci-dessous rejoue ce schéma avec des `IF NOT EXISTS`, ce qui ne change rien
//! à une base de production et crée tout sur une base neuve. Les migrations de
//! memoire vivent dans leur propre table (`_memoire_migrations`), si bien que
//! l'ancien binaire peut toujours redémarrer sur la même base en cas de retour
//! arrière : il ne voit aucune version inconnue dans `seaql_migrations`.

use std::time::{SystemTime, UNIX_EPOCH};

use paquets::orm::{Migration, Migrator};
use paquets::prelude::*;

/// Secondes Unix.
pub fn maintenant() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Millisecondes Unix.
pub fn maintenant_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Ajoute `sslmode=require` aux bases distantes (Heroku Postgres impose TLS,
/// avec un certificat que l'on ne vérifie pas) ; les bases locales et les URL
/// qui disent déjà leur `sslmode` restent telles quelles.
pub fn normalise_url_pg(url: &str) -> String {
    if url.contains("@localhost") || url.contains("@127.0.0.1") || url.contains("sslmode=") {
        return url.to_owned();
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}sslmode=require")
}

/// L'URL de la base : `DATABASE_URL`, sinon le Postgres local de
/// `docker-compose.yml`.
pub fn url_base() -> String {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        tracing::warn!("DATABASE_URL absent : base locale par défaut");
        "postgres://postgres:postgres@localhost:5432/moncap".to_owned()
    });
    normalise_url_pg(&url)
}

// ---------------------------------------------------------------------------
// Modèles. `user_id` n'est jamais renvoyé aux clients (sauf dans l'export du
// compte, qui passe par ses propres structures).
// ---------------------------------------------------------------------------

#[model(table = "users")]
pub struct Utilisateur {
    pub id: i32,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub created_at: i64,
    pub prefs: Option<String>,
    /// Les anciens jetons JWT émis avant cet instant ne valent plus rien.
    pub jetons_revoques_avant: i64,
}

#[model(table = "positions")]
pub struct Position {
    pub id: i32,
    pub lat: f64,
    pub lon: f64,
    pub label: String,
    #[serde(skip)]
    pub user_id: i32,
}

#[model(table = "trips")]
pub struct Trajet {
    pub id: i32,
    pub label: String,
    pub distance_km: f64,
    pub duration_min: f64,
    pub polyline: String,
    pub created_at: i64,
    #[serde(skip)]
    pub user_id: i32,
    pub pro: bool,
    pub motif: String,
}

#[model(table = "searches")]
pub struct Recherche {
    pub id: i32,
    pub name: String,
    pub subtitle: String,
    pub lat: f64,
    pub lon: f64,
    pub created_at: i64,
    #[serde(skip)]
    pub user_id: i32,
}

#[model(table = "alerts")]
pub struct Signalement {
    pub id: i32,
    pub category: String,
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    pub created_at: i64,
    pub expires_at: i64,
    pub confirms: i32,
    pub denies: i32,
    pub user_id: i32,
}

#[model(table = "reset_tokens")]
pub struct JetonReinit {
    pub id: i32,
    pub user_id: i32,
    pub token_hash: String,
    pub expires_at: i64,
    pub used: bool,
}

/// Une session ouverte : un appareil connecté. Seule l'empreinte SHA-256 du
/// jeton est stockée ; le jeton lui-même ne vit que chez le client.
#[model(table = "sessions")]
pub struct Session {
    pub id: i64,
    pub user_id: i32,
    pub token_hash: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
    pub device: String,
}

/// Les échecs de connexion récents d'un identifiant (en minuscules), pour
/// freiner qui devine un mot de passe sans bloquer tout un réseau mobile.
#[model(table = "echecs_connexion")]
pub struct EchecsConnexion {
    #[memoire(primary_key, auto_increment = false)]
    pub cle: String,
    pub echecs: i32,
    pub depuis: i64,
    pub bloque_jusqu_a: i64,
}

/// Toutes les migrations, dans l'ordre.
pub fn migrations() -> Migrator {
    Migrator::new()
        .add(
            // Le schéma SeaORM m0001 → m0009, rejoué à l'identique.
            Migration::new(1, "schema_historique")
                .sql(
                    "CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY, \
                     username TEXT UNIQUE NOT NULL, password_hash TEXT NOT NULL)",
                )
                .sql(
                    "CREATE TABLE IF NOT EXISTS positions (id SERIAL PRIMARY KEY, \
                     lat DOUBLE PRECISION NOT NULL, lon DOUBLE PRECISION NOT NULL, \
                     label TEXT NOT NULL)",
                )
                .sql("ALTER TABLE positions ADD COLUMN IF NOT EXISTS user_id INTEGER NOT NULL DEFAULT 0")
                .sql(
                    "CREATE TABLE IF NOT EXISTS trips (id SERIAL PRIMARY KEY, label TEXT NOT NULL, \
                     distance_km DOUBLE PRECISION NOT NULL DEFAULT 0, \
                     duration_min DOUBLE PRECISION NOT NULL DEFAULT 0, \
                     polyline TEXT NOT NULL DEFAULT '', created_at BIGINT NOT NULL DEFAULT 0, \
                     user_id INTEGER NOT NULL DEFAULT 0)",
                )
                .sql("CREATE INDEX IF NOT EXISTS trips_user_id_idx ON trips (user_id)")
                .sql(
                    "CREATE TABLE IF NOT EXISTS searches (id SERIAL PRIMARY KEY, name TEXT NOT NULL, \
                     subtitle TEXT NOT NULL DEFAULT '', lat DOUBLE PRECISION NOT NULL, \
                     lon DOUBLE PRECISION NOT NULL, created_at BIGINT NOT NULL DEFAULT 0, \
                     user_id INTEGER NOT NULL DEFAULT 0)",
                )
                .sql("CREATE INDEX IF NOT EXISTS searches_user_id_idx ON searches (user_id)")
                .sql("CREATE INDEX IF NOT EXISTS positions_user_id_idx ON positions (user_id)")
                .sql(
                    "CREATE TABLE IF NOT EXISTS alerts (id SERIAL PRIMARY KEY, category TEXT NOT NULL, \
                     label TEXT NOT NULL DEFAULT '', lat DOUBLE PRECISION NOT NULL, \
                     lon DOUBLE PRECISION NOT NULL, created_at BIGINT NOT NULL DEFAULT 0, \
                     expires_at BIGINT NOT NULL DEFAULT 0, confirms INTEGER NOT NULL DEFAULT 0, \
                     denies INTEGER NOT NULL DEFAULT 0, user_id INTEGER NOT NULL DEFAULT 0)",
                )
                .sql("CREATE INDEX IF NOT EXISTS alerts_expires_at_idx ON alerts (expires_at)")
                .sql(
                    "CREATE TABLE IF NOT EXISTS reset_tokens (id SERIAL PRIMARY KEY, \
                     user_id INTEGER NOT NULL, token_hash TEXT UNIQUE NOT NULL, \
                     expires_at BIGINT NOT NULL, used BOOLEAN NOT NULL DEFAULT FALSE)",
                )
                .sql(
                    "ALTER TABLE users ADD COLUMN IF NOT EXISTS created_at BIGINT NOT NULL \
                     DEFAULT CAST(EXTRACT(EPOCH FROM NOW()) AS BIGINT)",
                )
                .sql("ALTER TABLE users ADD COLUMN IF NOT EXISTS prefs TEXT")
                .sql("ALTER TABLE trips ADD COLUMN IF NOT EXISTS pro BOOLEAN NOT NULL DEFAULT FALSE")
                .sql("ALTER TABLE trips ADD COLUMN IF NOT EXISTS motif TEXT NOT NULL DEFAULT ''"),
        )
        .add(
            // La connexion fiable : des sessions révocables, la révocation des
            // anciens JWT, l'identifiant sans casse, le frein aux devinettes.
            Migration::new(2, "sessions")
                .sql(
                    "CREATE TABLE IF NOT EXISTS sessions (id BIGSERIAL PRIMARY KEY, \
                     user_id INTEGER NOT NULL, token_hash TEXT UNIQUE NOT NULL, \
                     created_at BIGINT NOT NULL, last_seen_at BIGINT NOT NULL, \
                     expires_at BIGINT NOT NULL, device TEXT NOT NULL DEFAULT '')",
                )
                .sql("CREATE INDEX IF NOT EXISTS sessions_user_id_idx ON sessions (user_id)")
                .sql("CREATE INDEX IF NOT EXISTS sessions_expires_at_idx ON sessions (expires_at)")
                .sql(
                    "ALTER TABLE users ADD COLUMN IF NOT EXISTS jetons_revoques_avant BIGINT \
                     NOT NULL DEFAULT 0",
                )
                .sql("CREATE INDEX IF NOT EXISTS users_username_lower_idx ON users (lower(username))")
                .sql("CREATE INDEX IF NOT EXISTS reset_tokens_user_id_idx ON reset_tokens (user_id)")
                .sql(
                    "CREATE TABLE IF NOT EXISTS echecs_connexion (cle TEXT PRIMARY KEY, \
                     echecs INTEGER NOT NULL DEFAULT 0, depuis BIGINT NOT NULL DEFAULT 0, \
                     bloque_jusqu_a BIGINT NOT NULL DEFAULT 0)",
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_url_adds_sslmode_for_remote_only() {
        assert_eq!(
            normalise_url_pg("postgres://u:p@localhost:5432/db"),
            "postgres://u:p@localhost:5432/db"
        );
        assert_eq!(
            normalise_url_pg("postgres://u:p@host.example:5432/db"),
            "postgres://u:p@host.example:5432/db?sslmode=require"
        );
        assert_eq!(
            normalise_url_pg("postgres://u:p@h/db?x=1"),
            "postgres://u:p@h/db?x=1&sslmode=require"
        );
        assert_eq!(
            normalise_url_pg("postgres://u:p@h/db?sslmode=disable"),
            "postgres://u:p@h/db?sslmode=disable"
        );
    }
}
