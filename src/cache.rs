//! Le cache, dans SQLite : un fichier à part (`data/cache.db`), jetable.
//!
//! Il porte ce qui coûte cher à redemander : les tuiles de carte (relayées ou
//! lues dans l'archive PMTiles), les feux, la qualité de l'air et la liste des
//! régions hors ligne. Les tuiles y gagnent le plus : la mémoire en gardait
//! 1500 (27 Mo de RAM), le disque en garde des dizaines de milliers, et une
//! tuile d'archive ne coûte plus une requête de plage à chaque affichage.
//!
//! Un fichier à part de la base, pour trois raisons : ses écritures (une
//! tuile par requête au pire) ne prennent pas le verrou de la base des
//! comptes, son journal ne la fait pas gonfler, et on peut l'effacer à tout
//! moment sans rien perdre.
//!
//! Tout y est au mieux : une erreur de cache se journalise et compte comme un
//! défaut de cache, jamais comme une panne.

use paquets::orm::{Executor, Migration, Migrator, params};
use paquets::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::base::maintenant;

/// Ce qui a plus de trente jours part au ménage.
pub const AGE_MAX: i64 = 30 * 24 * 3600;

/// Le cache, rangé dans l'état de l'application.
pub struct CacheRef(pub &'static Db);

/// Le cache de l'application.
pub fn cache(req: &Request) -> &'static Db {
    req.state::<CacheRef>().0
}

/// L'URL du cache : `CACHE_URL`, sinon `data/cache.db` à côté de la base.
pub fn url_cache() -> String {
    std::env::var("CACHE_URL")
        .ok()
        .map(|u| u.trim().to_owned())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "sqlite://data/cache.db".to_owned())
}

/// Le poids maximal du cache, en octets (`CACHE_MAX_MO`, 512 Mo par défaut).
pub fn octets_max() -> i64 {
    std::env::var("CACHE_MAX_MO")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|&mo| mo > 0)
        .unwrap_or(512)
        * 1024
        * 1024
}

/// Ouvre le cache. Un fichier illisible ou impossible à créer ne doit pas
/// empêcher le serveur de démarrer : le cache tombe alors en mémoire.
pub fn ouvre(url: &str) -> Db {
    match paquets::orm::block_on(Db::connect(url)) {
        Ok(db) => db,
        Err(e) => {
            tracing::warn!("cache {url} inutilisable ({e}) : cache en mémoire");
            paquets::orm::block_on(Db::connect("sqlite::memory:")).expect("SQLite en mémoire")
        }
    }
}

/// Crée la table du cache. La place libérée par le ménage est rendue au
/// disque au fil de l'eau (`auto_vacuum`), ce qui doit être décidé avant la
/// première table.
pub async fn prepare(db: &Db) -> Result<(), paquets::orm::Error> {
    db.raw_execute("PRAGMA auto_vacuum = INCREMENTAL", &[])
        .await?;
    Migrator::new()
        .add(
            Migration::new(1, "cache")
                .sql(
                    "CREATE TABLE IF NOT EXISTS cache (cle TEXT PRIMARY KEY, \
                     valeur BLOB NOT NULL, genre TEXT NOT NULL DEFAULT '', \
                     range_a INTEGER NOT NULL)",
                )
                .sql("CREATE INDEX IF NOT EXISTS cache_range_a_idx ON cache (range_a)"),
        )
        .run(db)
        .await?;
    Ok(())
}

/// Une entrée lue : son contenu, son genre (un type MIME, le plus souvent)
/// et son âge en secondes.
pub struct Entree {
    pub octets: Vec<u8>,
    pub genre: String,
    pub age: i64,
}

/// L'entrée d'une clé, si elle est en cache.
pub async fn lit(db: &Db, cle: &str) -> Option<Entree> {
    let lignes: Result<Vec<(Vec<u8>, String, i64)>, _> = db
        .raw(
            "SELECT valeur, genre, range_a FROM cache WHERE cle = ?1",
            &params![cle.to_owned()],
        )
        .await;
    match lignes {
        Ok(lignes) => lignes.into_iter().next().map(|(octets, genre, t)| Entree {
            octets,
            genre,
            age: (maintenant() - t).max(0),
        }),
        Err(e) => {
            tracing::warn!("cache, lecture de {cle} : {e}");
            None
        }
    }
}

/// Range une entrée (et remplace l'ancienne).
pub async fn range(db: &Db, cle: &str, octets: Vec<u8>, genre: &str) {
    let fait = db
        .raw_execute(
            "INSERT INTO cache (cle, valeur, genre, range_a) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (cle) DO UPDATE SET valeur = excluded.valeur, \
             genre = excluded.genre, range_a = excluded.range_a",
            &params![cle.to_owned(), octets, genre.to_owned(), maintenant()],
        )
        .await;
    if let Err(e) = fait {
        tracing::warn!("cache, écriture de {cle} : {e}");
    }
}

/// Une valeur JSON en cache, et son âge en secondes.
pub async fn lit_json<T: DeserializeOwned>(db: &Db, cle: &str) -> Option<(T, i64)> {
    let entree = lit(db, cle).await?;
    let valeur = serde_json::from_slice(&entree.octets).ok()?;
    Some((valeur, entree.age))
}

/// Range une valeur en JSON.
pub async fn range_json<T: Serialize + ?Sized>(db: &Db, cle: &str, valeur: &T) {
    if let Ok(octets) = serde_json::to_vec(valeur) {
        range(db, cle, octets, "application/json").await;
    }
}

/// Le ménage : ce qui a plus de `age_max` secondes, puis les entrées les plus
/// anciennes tant que le cache dépasse `octets_max`. Rend le nombre
/// d'entrées retirées.
pub async fn menage(db: &Db, age_max: i64, octets_max: i64) -> Result<u64, paquets::orm::Error> {
    let mut retirees = db
        .raw_execute(
            "DELETE FROM cache WHERE range_a < ?1",
            &params![maintenant() - age_max],
        )
        .await?;
    // Le cumul des tailles, des plus récentes aux plus anciennes : tout ce
    // qui dépasse la borne part.
    retirees += db
        .raw_execute(
            "DELETE FROM cache WHERE rowid IN (SELECT rowid FROM (SELECT rowid, \
             sum(length(valeur)) OVER (ORDER BY range_a DESC, rowid DESC) AS cumul \
             FROM cache) WHERE cumul > ?1)",
            &params![octets_max],
        )
        .await?;
    if retirees > 0 {
        db.raw_execute("PRAGMA incremental_vacuum", &[]).await?;
    }
    Ok(retirees)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn neuf() -> Db {
        let db = ouvre("sqlite::memory:");
        prepare(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn aller_retour_et_remplacement() {
        let db = neuf().await;
        assert!(lit(&db, "a").await.is_none());
        range(&db, "a", vec![1, 2, 3], "image/png").await;
        let e = lit(&db, "a").await.unwrap();
        assert_eq!((e.octets, e.genre.as_str()), (vec![1, 2, 3], "image/png"));
        assert!(e.age <= 1);
        range(&db, "a", vec![4], "image/webp").await;
        let e = lit(&db, "a").await.unwrap();
        assert_eq!((e.octets, e.genre.as_str()), (vec![4], "image/webp"));

        range_json(&db, "j", &vec!["x", "y"]).await;
        let (v, _) = lit_json::<Vec<String>>(&db, "j").await.unwrap();
        assert_eq!(v, ["x", "y"]);
        // Un contenu qui n'a pas la forme attendue est un défaut de cache.
        assert!(lit_json::<Vec<u8>>(&db, "a").await.is_none());
    }

    #[tokio::test]
    async fn le_menage_respecte_l_age_puis_le_poids() {
        let db = neuf().await;
        for (cle, age) in [("vieille", 100), ("moyenne", 20), ("jeune", 10)] {
            db.raw_execute(
                "INSERT INTO cache (cle, valeur, genre, range_a) VALUES (?1, ?2, '', ?3)",
                &params![cle, vec![0u8; 10], maintenant() - age],
            )
            .await
            .unwrap();
        }
        // Trop vieille : partie, les deux autres restent.
        assert_eq!(menage(&db, 50, 1_000).await.unwrap(), 1);
        assert!(lit(&db, "vieille").await.is_none());
        // Vingt octets pour une borne de quinze : la plus ancienne cède.
        assert_eq!(menage(&db, 50, 15).await.unwrap(), 1);
        assert!(lit(&db, "moyenne").await.is_none());
        assert!(lit(&db, "jeune").await.is_some());
        assert_eq!(menage(&db, 50, 15).await.unwrap(), 0);
    }
}
