//! La base SQLite : les tables, leurs modèles, et les migrations.
//!
//! Un fichier, `data/moncap.db` par défaut, ouvert par Paquets dans le
//! processus même : ni serveur de base à faire tourner, ni connexion réseau à
//! payer à chaque requête. Le mode WAL (posé par memoire-orm) laisse lire
//! pendant qu'on écrit, et le fichier se sauvegarde d'une simple copie.
//!
//! Le schéma repart de zéro : il n'hérite pas de l'ancienne base Postgres,
//! qui n'existe plus. Les migrations tiennent leur registre dans
//! `_memoire_migrations`.

use std::time::{SystemTime, UNIX_EPOCH};

use paquets::orm::{Migration, Migrator};
use paquets::prelude::*;

/// L'emplacement de la base quand rien n'est configuré.
pub const BASE_PAR_DEFAUT: &str = "sqlite://data/moncap.db";

/// Secondes Unix.
pub fn maintenant() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Une URL de base SQLite ? `sqlite:…`, ou un chemin en `.db`, `.sqlite`,
/// `.sqlite3` — les formes que Paquets ouvre en SQLite.
pub fn est_sqlite(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.starts_with("sqlite:")
        || (!url.contains("://")
            && [".db", ".sqlite", ".sqlite3"]
                .iter()
                .any(|ext| url.ends_with(ext)))
}

/// L'URL de la base : `DATABASE_URL` s'il désigne un fichier SQLite, sinon
/// `data/moncap.db`. Une ancienne URL Postgres restée dans la configuration
/// est écartée avec un avertissement plutôt que d'empêcher le démarrage.
pub fn url_base() -> String {
    match std::env::var("DATABASE_URL") {
        Ok(url) if est_sqlite(&url) => url.trim().to_owned(),
        Ok(url) if !url.trim().is_empty() => {
            tracing::warn!("DATABASE_URL n'est pas une base SQLite : {BASE_PAR_DEFAUT} à la place");
            BASE_PAR_DEFAUT.to_owned()
        }
        _ => BASE_PAR_DEFAUT.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Modèles. `user_id` n'est jamais renvoyé aux clients (sauf dans l'export du
// compte, qui passe par ses propres structures).
// ---------------------------------------------------------------------------

#[model(table = "users")]
pub struct Utilisateur {
    pub id: i32,
    pub username: String,
    /// L'identifiant sans casse ni espaces autour (`connexion::cle_identifiant`) :
    /// unique, c'est lui qui fait qu'un compte ne se crée qu'une fois.
    #[serde(skip_serializing)]
    pub cle: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub created_at: i64,
    pub prefs: Option<String>,
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

/// Une destination choisie : **toutes** sont gardées, une ligne par lieu.
/// Y retourner remonte la ligne en tête (`created_at`) et compte la visite
/// (`fois`) au lieu d'en créer une autre.
#[model(table = "searches")]
pub struct Recherche {
    pub id: i32,
    pub name: String,
    pub subtitle: String,
    pub lat: f64,
    pub lon: f64,
    /// La dernière fois que ce lieu a été choisi.
    pub created_at: i64,
    #[serde(skip)]
    pub user_id: i32,
    /// Le lieu, au nom et à une dizaine de mètres près (`trajets::recent_key`).
    #[serde(skip)]
    pub cle: String,
    /// Combien de fois il a été choisi.
    pub fois: i32,
    /// L'ordre des choix, croissant par compte : « le plus récent d'abord »
    /// sans dépendre de l'horloge, même pour deux choix dans la même seconde.
    #[serde(skip)]
    pub rang: i64,
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

/// Les échecs de connexion récents d'un identifiant (sa clé), pour freiner
/// qui devine un mot de passe sans bloquer tout un réseau mobile.
#[model(table = "echecs_connexion")]
pub struct EchecsConnexion {
    #[memoire(primary_key, auto_increment = false)]
    pub cle: String,
    pub echecs: i32,
    pub depuis: i64,
    pub bloque_jusqu_a: i64,
}

/// Toutes les migrations, dans l'ordre.
///
/// Les données d'un compte disparaissent avec lui (`ON DELETE CASCADE`) : la
/// suppression du compte les efface déjà une à une, la base y veille aussi.
/// `AUTOINCREMENT` : un identifiant libéré n'est jamais redonné, si bien
/// qu'une vieille référence ne désigne jamais la ligne de quelqu'un d'autre.
pub fn migrations() -> Migrator {
    Migrator::new().add(
        Migration::new(1, "schema")
            .sql(
                "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 username TEXT NOT NULL, cle TEXT NOT NULL UNIQUE, \
                 password_hash TEXT NOT NULL, created_at INTEGER NOT NULL, prefs TEXT)",
            )
            .sql(
                "CREATE TABLE sessions (id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE, \
                 token_hash TEXT NOT NULL UNIQUE, created_at INTEGER NOT NULL, \
                 last_seen_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, \
                 device TEXT NOT NULL DEFAULT '')",
            )
            .sql("CREATE INDEX sessions_user_id_idx ON sessions (user_id)")
            .sql("CREATE INDEX sessions_expires_at_idx ON sessions (expires_at)")
            .sql(
                "CREATE TABLE reset_tokens (id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE, \
                 token_hash TEXT NOT NULL UNIQUE, expires_at INTEGER NOT NULL, \
                 used INTEGER NOT NULL DEFAULT 0)",
            )
            .sql("CREATE INDEX reset_tokens_user_id_idx ON reset_tokens (user_id)")
            .sql(
                "CREATE TABLE echecs_connexion (cle TEXT PRIMARY KEY, \
                 echecs INTEGER NOT NULL DEFAULT 0, depuis INTEGER NOT NULL DEFAULT 0, \
                 bloque_jusqu_a INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID",
            )
            .sql(
                "CREATE TABLE positions (id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 lat REAL NOT NULL, lon REAL NOT NULL, label TEXT NOT NULL, \
                 user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE)",
            )
            .sql("CREATE INDEX positions_user_id_idx ON positions (user_id, id)")
            .sql(
                "CREATE TABLE trips (id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 label TEXT NOT NULL, distance_km REAL NOT NULL DEFAULT 0, \
                 duration_min REAL NOT NULL DEFAULT 0, polyline TEXT NOT NULL DEFAULT '', \
                 created_at INTEGER NOT NULL DEFAULT 0, \
                 user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE, \
                 pro INTEGER NOT NULL DEFAULT 0, motif TEXT NOT NULL DEFAULT '')",
            )
            .sql("CREATE INDEX trips_user_id_idx ON trips (user_id, created_at)")
            .sql(
                "CREATE TABLE searches (id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 name TEXT NOT NULL, subtitle TEXT NOT NULL DEFAULT '', \
                 lat REAL NOT NULL, lon REAL NOT NULL, created_at INTEGER NOT NULL DEFAULT 0, \
                 user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE, \
                 cle TEXT NOT NULL, fois INTEGER NOT NULL DEFAULT 1, \
                 rang INTEGER NOT NULL DEFAULT 0, UNIQUE (user_id, cle))",
            )
            .sql("CREATE INDEX searches_recentes_idx ON searches (user_id, rang)")
            .sql(
                "CREATE TABLE alerts (id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 category TEXT NOT NULL, label TEXT NOT NULL DEFAULT '', \
                 lat REAL NOT NULL, lon REAL NOT NULL, created_at INTEGER NOT NULL DEFAULT 0, \
                 expires_at INTEGER NOT NULL DEFAULT 0, confirms INTEGER NOT NULL DEFAULT 0, \
                 denies INTEGER NOT NULL DEFAULT 0, \
                 user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE)",
            )
            .sql("CREATE INDEX alerts_expires_at_idx ON alerts (expires_at)")
            .sql("CREATE INDEX alerts_user_id_idx ON alerts (user_id, created_at)"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seules_les_bases_sqlite_sont_retenues() {
        for oui in [
            "sqlite://data/moncap.db",
            "sqlite::memory:",
            "sqlite:///var/data/moncap.db",
            "moncap.db",
            " data/app.SQLITE3 ",
        ] {
            assert!(est_sqlite(oui), "{oui}");
        }
        for non in [
            "postgres://u:p@localhost:5432/moncap",
            "postgresql://h/moncap.db",
            "redis://h",
            "",
            "moncap",
        ] {
            assert!(!est_sqlite(non), "{non}");
        }
    }
}
