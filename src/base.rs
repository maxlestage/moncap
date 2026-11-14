//! La base : ses tables, et l'endroit où elle vit.
//!
//! En production, **Nuage DB** : un serveur toujours allumé, qui chiffre tout
//! et se sauvegarde sur S3 (`NUAGE_URL` et `NUAGE_TOKEN`). Le disque d'un dyno
//! Heroku est effacé au moins une fois par jour ; une base SQLite posée dessus
//! oubliait comptes, trajets et destinations à chaque fois. Sur Nuage DB, ils
//! restent.
//!
//! Sans `NUAGE_URL`, la base reste un fichier sur la machine : `DATABASE_URL`
//! s'il désigne un fichier SQLite, sinon le mode local de Nuage DB, dans
//! `data/` — le même moteur que le serveur, pour développer.
//!
//! Nuage DB n'exécute pas le SQL brut de Memoire : toutes les requêtes passent
//! par les modèles et le constructeur de requêtes, et les tables se créent
//! d'après les modèles (`tables`), unicités et index compris. Le même code
//! tourne ainsi sur Nuage DB, sur son mode local et sur SQLite.

use std::time::{SystemTime, UNIX_EPOCH};

use paquets::prelude::*;

/// L'emplacement de la base quand rien n'est configuré : le mode local de
/// Nuage DB, dans `data/`.
pub const BASE_PAR_DEFAUT: &str = "nuage:data";

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

/// L'adresse d'un serveur Nuage DB (`https://…`, `http://…`), ou le dossier
/// de son mode local (`nuage:data`), telle que Paquets l'ouvre.
fn cible_nuage(valeur: &str) -> String {
    let valeur = valeur.trim();
    let minuscules = valeur.to_ascii_lowercase();
    if minuscules.starts_with("https://")
        || minuscules.starts_with("http://")
        || minuscules.starts_with("nuage:")
    {
        valeur.to_owned()
    } else {
        format!("nuage:{valeur}")
    }
}

/// L'URL de la base, d'après ces deux variables : `NUAGE_URL` d'abord (un
/// serveur Nuage DB, ou un dossier), puis `DATABASE_URL` s'il désigne un
/// fichier SQLite, sinon `nuage:data`. Une ancienne URL Postgres restée dans
/// la configuration est écartée avec un avertissement plutôt que d'empêcher
/// le démarrage.
pub fn choisit_base(nuage_url: Option<&str>, database_url: Option<&str>) -> String {
    fn rempli(v: Option<&str>) -> Option<&str> {
        v.map(str::trim).filter(|v| !v.is_empty())
    }
    if let Some(cible) = rempli(nuage_url) {
        return cible_nuage(cible);
    }
    match rempli(database_url) {
        Some(url) if est_sqlite(url) => url.to_owned(),
        Some(_) => {
            tracing::warn!("DATABASE_URL n'est pas une base SQLite : {BASE_PAR_DEFAUT} à la place");
            BASE_PAR_DEFAUT.to_owned()
        }
        None => BASE_PAR_DEFAUT.to_owned(),
    }
}

/// L'URL de la base, d'après l'environnement (voir [`choisit_base`]).
pub fn url_base() -> String {
    let nuage = std::env::var("NUAGE_URL").ok();
    let database = std::env::var("DATABASE_URL").ok();
    let url = choisit_base(nuage.as_deref(), database.as_deref());
    if std::env::var("DYNO").is_ok() && !url.starts_with("http") {
        tracing::warn!(
            "NUAGE_URL absent : la base est sur le disque du dyno, effacé à chaque redémarrage. \
             Définir NUAGE_URL et NUAGE_TOKEN (le serveur Nuage DB)."
        );
    }
    url
}

/// Ce que le journal peut dire de la base : jamais un jeton ni un mot de
/// passe, seulement son genre et son hôte.
pub fn decrit_base(url: &str) -> String {
    match url.split_once("://") {
        Some((schema, reste)) if schema.starts_with("http") => {
            let hote = reste.split(['/', '?', '#']).next().unwrap_or("");
            let hote = hote.rsplit('@').next().unwrap_or(hote);
            format!("Nuage DB ({schema}://{hote})")
        }
        _ if url.starts_with("nuage:") => format!("Nuage DB, local ({})", &url[6..]),
        _ => url.to_owned(),
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
    #[memoire(unique)]
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
    #[memoire(index)]
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
    #[memoire(index)]
    pub user_id: i32,
    pub pro: bool,
    pub motif: String,
}

/// Une destination choisie : **toutes** sont gardées, une ligne par lieu.
/// Y retourner remonte la ligne en tête (`created_at`) et compte la visite
/// (`fois`) au lieu d'en créer une autre.
#[model(table = "searches")]
#[memoire(unique(user_id, cle))]
pub struct Recherche {
    pub id: i32,
    pub name: String,
    pub subtitle: String,
    pub lat: f64,
    pub lon: f64,
    /// La dernière fois que ce lieu a été choisi.
    pub created_at: i64,
    #[serde(skip)]
    #[memoire(index)]
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
    #[memoire(index)]
    pub user_id: i32,
}

#[model(table = "reset_tokens")]
pub struct JetonReinit {
    pub id: i32,
    #[memoire(index)]
    pub user_id: i32,
    #[memoire(unique)]
    pub token_hash: String,
    pub expires_at: i64,
    pub used: bool,
}

/// Une session ouverte : un appareil connecté. Seule l'empreinte SHA-256 du
/// jeton est stockée ; le jeton lui-même ne vit que chez le client.
#[model(table = "sessions")]
pub struct Session {
    pub id: i64,
    #[memoire(index)]
    pub user_id: i32,
    #[memoire(unique)]
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

/// Déclare les tables de l'application : Paquets les crée d'après les
/// modèles au démarrage, ou ajoute à une table existante les colonnes et les
/// index qui lui manquent (`push`). Pas de SQL : Nuage DB ne l'exécute pas
/// depuis Memoire, et ce sont les modèles qui portent unicités et index.
///
/// Les données d'un compte partent avec lui : la suppression du compte les
/// efface une à une, dans la même transaction (`compte::supprime`).
pub fn tables(app: &mut App) {
    app.model::<Utilisateur>()
        .model::<Session>()
        .model::<JetonReinit>()
        .model::<EchecsConnexion>()
        .model::<Position>()
        .model::<Trajet>()
        .model::<Recherche>()
        .model::<Signalement>();
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

    #[test]
    fn nuage_url_passe_avant_tout() {
        let nuage = "https://nuage-f48a8945f31c.herokuapp.com";
        assert_eq!(choisit_base(Some(nuage), None), nuage);
        assert_eq!(
            choisit_base(Some(&format!(" {nuage} ")), Some("sqlite://data/moncap.db")),
            nuage
        );
        // Un dossier : le mode local de Nuage DB.
        assert_eq!(choisit_base(Some("donnees"), None), "nuage:donnees");
        assert_eq!(choisit_base(Some("nuage:donnees"), None), "nuage:donnees");
    }

    #[test]
    fn sans_nuage_url_la_base_reste_locale() {
        assert_eq!(choisit_base(None, None), BASE_PAR_DEFAUT);
        assert_eq!(choisit_base(Some("  "), None), BASE_PAR_DEFAUT);
        assert_eq!(
            choisit_base(None, Some("sqlite://data/moncap.db")),
            "sqlite://data/moncap.db"
        );
        // Une vieille URL Postgres n'empêche pas de démarrer.
        assert_eq!(
            choisit_base(None, Some("postgres://u:p@h:5432/moncap")),
            BASE_PAR_DEFAUT
        );
    }

    #[test]
    fn le_journal_ne_montre_que_l_hote() {
        assert_eq!(
            decrit_base("https://nuage-f48a8945f31c.herokuapp.com/v1?x=1"),
            "Nuage DB (https://nuage-f48a8945f31c.herokuapp.com)"
        );
        assert_eq!(decrit_base("nuage:data"), "Nuage DB, local (data)");
        assert_eq!(
            decrit_base("sqlite://data/moncap.db"),
            "sqlite://data/moncap.db"
        );
    }
}
