//! La sauvegarde de la base SQLite dans une Release GitHub privée.
//!
//! Le disque d'un dyno Heroku est éphémère : il repart vide à chaque
//! redémarrage — au moins une fois par jour, et à chaque déploiement. La base
//! `data/moncap.db` partait avec lui : comptes, sessions, trajets, partages.
//!
//! Avec un jeton GitHub qui écrit dans les Releases de `SAUVEGARDE_DEPOT`
//! (par défaut `maxlestage/moncap`, privé) — `SAUVEGARDE_TOKEN`, à défaut le
//! `GITHUB_TOKEN` déjà posé pour les régions hors ligne —, le serveur :
//!
//! - au **démarrage**, quand la base n'existe pas encore sur le disque, la
//!   restaure depuis la dernière copie ;
//! - **chaque minute**, en prend un cliché cohérent (`VACUUM INTO`) et
//!   l'envoie s'il a changé depuis le dernier envoi ;
//! - à l'**arrêt** (`SIGTERM`, qu'Heroku envoie trente secondes avant de
//!   couper), envoie un dernier cliché.
//!
//! Les copies sont des pièces jointes `moncap-<secondes>.db.gz` de la Release
//! `sauvegarde-base` ; les deux plus récentes sont gardées. Sans jeton, rien
//! de tout cela ne tourne, et la base vit comme avant sur le disque.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use paquets::prelude::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::base;

/// L'étiquette de la Release qui porte les copies.
const ETIQUETTE: &str = "sauvegarde-base";

/// Le préfixe et le suffixe des pièces jointes.
const PREFIXE: &str = "moncap-";
const SUFFIXE: &str = ".db.gz";

/// Combien de copies garder : la dernière, et celle d'avant au cas où.
const GARDEES: usize = 2;

/// GitHub a refusé le jeton (401, 403, 404 en écriture) : il ne sert à rien
/// de réessayer chaque minute, ni d'en remplir les journaux.
static REFUSE: AtomicBool = AtomicBool::new(false);

/// Note un refus du jeton, et le dit une seule fois.
fn refus(statut: reqwest::StatusCode, quoi: &str) -> String {
    if matches!(statut.as_u16(), 401 | 403 | 404) && !REFUSE.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "sauvegarde arrêtée : GitHub refuse le jeton pour {quoi} ({statut}). \
             Il faut SAUVEGARDE_TOKEN, « Contents: Read and write » sur le dépôt."
        );
    }
    format!("{quoi} : GitHub répond {statut}")
}

/// Le délai d'un appel à GitHub.
const DELAI: Duration = Duration::from_secs(20);

/// Ce qu'il faut pour sauvegarder.
#[derive(Clone, Debug)]
pub struct Reglage {
    depot: String,
    jeton: String,
    /// L'API GitHub et son hôte d'envoi ; `SAUVEGARDE_API` remplace les deux
    /// par un faux GitHub, pour un essai en local.
    api: String,
    envois: String,
}

impl Reglage {
    /// `SAUVEGARDE_TOKEN` (à défaut `GITHUB_TOKEN`) et `SAUVEGARDE_DEPOT` ;
    /// `None` sans jeton.
    pub fn depuis_env() -> Option<Self> {
        let jeton = std::env::var("SAUVEGARDE_TOKEN")
            .ok()
            .filter(|j| !j.trim().is_empty())
            .or_else(|| std::env::var("GITHUB_TOKEN").ok())?;
        let jeton = jeton.trim().trim_matches(['"', '\'']).trim().to_owned();
        if jeton.is_empty() {
            return None;
        }
        let depot = std::env::var("SAUVEGARDE_DEPOT")
            .ok()
            .map(|d| d.trim().to_owned())
            .filter(|d| depot_valide(d))
            .unwrap_or_else(|| "maxlestage/moncap".to_owned());
        let (api, envois) = match std::env::var("SAUVEGARDE_API") {
            Ok(a) if !a.trim().is_empty() => {
                let a = a.trim().trim_end_matches('/').to_owned();
                (a.clone(), a)
            }
            _ => (
                "https://api.github.com".to_owned(),
                "https://uploads.github.com".to_owned(),
            ),
        };
        Some(Reglage {
            depot,
            jeton,
            api,
            envois,
        })
    }
}

/// `propriétaire/dépôt`, sans rien qui sorte de l'URL de l'API.
fn depot_valide(depot: &str) -> bool {
    let mut parts = depot.split('/');
    let ok = |p: Option<&str>| {
        p.is_some_and(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    };
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

/// Le fichier d'une URL SQLite (`sqlite://data/moncap.db`, `sqlite:x.db`,
/// `x.db`) ; `None` pour une base en mémoire.
pub fn chemin_sqlite(url: &str) -> Option<PathBuf> {
    let url = url.trim();
    let chemin = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
        .unwrap_or(url);
    let chemin = chemin.split('?').next().unwrap_or_default();
    if chemin.is_empty() || chemin.starts_with(":memory:") {
        None
    } else {
        Some(PathBuf::from(chemin))
    }
}

/// Le nom de la pièce jointe d'un cliché pris à `secondes`.
fn nom_copie(secondes: i64) -> String {
    format!("{PREFIXE}{secondes:012}{SUFFIXE}")
}

/// Les copies, de la plus récente à la plus ancienne. Le nom porte l'heure,
/// sur douze chiffres : l'ordre des noms est celui du temps.
fn copies_triees(pieces: &[Piece]) -> Vec<&Piece> {
    let mut copies: Vec<&Piece> = pieces
        .iter()
        .filter(|p| p.name.starts_with(PREFIXE) && p.name.ends_with(SUFFIXE))
        .collect();
    copies.sort_by(|a, b| b.name.cmp(&a.name));
    copies
}

fn compresse(octets: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(octets)?;
    gz.finish()
}

fn decompresse(octets: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut sortie = Vec::new();
    flate2::read::GzDecoder::new(octets).read_to_end(&mut sortie)?;
    Ok(sortie)
}

/// Un cliché cohérent de la base, compressé : `VACUUM INTO` écrit une copie
/// complète, sans le journal WAL à côté, pendant que la base sert.
pub async fn cliche(db: &Db) -> Result<Vec<u8>, String> {
    let fichier = std::env::temp_dir().join(format!("moncap-cliche-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&fichier);
    let nom = fichier.to_string_lossy().replace('\'', "''");
    db.raw_execute(&format!("VACUUM INTO '{nom}'"), &[])
        .await
        .map_err(|e| format!("VACUUM INTO : {e}"))?;
    let octets = std::fs::read(&fichier).map_err(|e| format!("lecture du cliché : {e}"));
    let _ = std::fs::remove_file(&fichier);
    compresse(&octets?).map_err(|e| format!("compression : {e}"))
}

/// Écrit une copie téléchargée à la place de la base : d'abord à côté, puis
/// renommée, pour qu'un arrêt en plein milieu ne laisse pas un demi-fichier.
fn installe(chemin: &Path, copie: &[u8]) -> Result<(), String> {
    let octets = decompresse(copie).map_err(|e| format!("copie illisible : {e}"))?;
    if !octets.starts_with(b"SQLite format 3\0") {
        return Err("la copie n'est pas une base SQLite".into());
    }
    if let Some(dossier) = chemin.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dossier).map_err(|e| format!("dossier : {e}"))?;
    }
    let provisoire = chemin.with_extension("restauree");
    std::fs::write(&provisoire, &octets).map_err(|e| format!("écriture : {e}"))?;
    std::fs::rename(&provisoire, chemin).map_err(|e| format!("renommage : {e}"))
}

// ---------------------------------------------------------------------------
// GitHub
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Release {
    id: u64,
    #[serde(default)]
    assets: Vec<Piece>,
}

#[derive(Debug, Deserialize)]
struct Piece {
    id: u64,
    name: String,
}

fn client() -> reqwest::Client {
    crate::externes::constructeur_http()
        .timeout(DELAI)
        .user_agent("moncap-sauvegarde")
        .build()
        .unwrap_or_default()
}

impl Reglage {
    fn api(&self, chemin: &str) -> String {
        format!("{}/repos/{}{chemin}", self.api, self.depot)
    }

    fn requete(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.jeton)
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    /// La Release des copies ; `None` si elle n'existe pas encore.
    async fn release(&self, http: &reqwest::Client) -> Result<Option<Release>, String> {
        let r = self
            .requete(http.get(self.api(&format!("/releases/tags/{ETIQUETTE}"))))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| format!("GitHub injoignable : {e}"))?;
        match r.status().as_u16() {
            404 => Ok(None),
            s if (200..300).contains(&s) => r
                .json()
                .await
                .map(Some)
                .map_err(|e| format!("réponse de GitHub : {e}")),
            _ => Err(refus(r.status(), "la lecture de la Release")),
        }
    }

    async fn cree_release(&self, http: &reqwest::Client) -> Result<Release, String> {
        let corps = serde_json::json!({
            "tag_name": ETIQUETTE,
            "name": "Sauvegarde de la base",
            "body": "Copies de data/moncap.db prises par le serveur (voir src/sauvegarde.rs). Ne pas publier.",
            "prerelease": true,
        });
        let r = self
            .requete(http.post(self.api("/releases")))
            .json(&corps)
            .send()
            .await
            .map_err(|e| format!("GitHub injoignable : {e}"))?;
        if !r.status().is_success() {
            return Err(refus(r.status(), "la création de la Release"));
        }
        r.json()
            .await
            .map_err(|e| format!("réponse de GitHub : {e}"))
    }

    /// Envoie une copie, puis retire les plus anciennes.
    pub async fn envoie(&self, copie: Vec<u8>) -> Result<(), String> {
        let http = client();
        let release = match self.release(&http).await? {
            Some(r) => r,
            None => self.cree_release(&http).await?,
        };
        let nom = nom_copie(base::maintenant());
        let r = self
            .requete(http.post(format!(
                "{}/repos/{}/releases/{}/assets?name={nom}",
                self.envois, self.depot, release.id
            )))
            .header("Content-Type", "application/gzip")
            .body(copie)
            .send()
            .await
            .map_err(|e| format!("envoi : {e}"))?;
        if !r.status().is_success() {
            return Err(refus(r.status(), "l'envoi"));
        }
        // La nouvelle n'est pas dans la liste lue avant l'envoi : on en garde
        // donc une de moins parmi les anciennes.
        for vieille in copies_triees(&release.assets)
            .into_iter()
            .filter(|p| p.name != nom)
            .skip(GARDEES - 1)
        {
            let _ = self
                .requete(http.delete(self.api(&format!("/releases/assets/{}", vieille.id))))
                .send()
                .await;
        }
        Ok(())
    }

    /// La copie la plus récente ; `None` s'il n'y en a aucune.
    pub async fn derniere(&self) -> Result<Option<Vec<u8>>, String> {
        let http = client();
        let Some(release) = self.release(&http).await? else {
            return Ok(None);
        };
        let Some(piece) = copies_triees(&release.assets).first().map(|p| p.id) else {
            return Ok(None);
        };
        // GitHub redirige vers son stockage ; reqwest suit et ne renvoie pas
        // le jeton à un autre hôte.
        let r = self
            .requete(http.get(self.api(&format!("/releases/assets/{piece}"))))
            .header("Accept", "application/octet-stream")
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| format!("téléchargement : {e}"))?;
        if !r.status().is_success() {
            return Err(format!("téléchargement : GitHub répond {}", r.status()));
        }
        r.bytes()
            .await
            .map(|b| Some(b.to_vec()))
            .map_err(|e| format!("téléchargement : {e}"))
    }
}

// ---------------------------------------------------------------------------
// Le cycle de vie
// ---------------------------------------------------------------------------

/// Au démarrage, avant que la base ne s'ouvre : si son fichier manque (un
/// dyno neuf), la dernière copie le remplace. Une base déjà sur le disque
/// n'est jamais écrasée.
pub fn restaure(url: &str) {
    let (Some(reglage), Some(chemin)) = (Reglage::depuis_env(), chemin_sqlite(url)) else {
        return;
    };
    if chemin.exists() {
        return;
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("sauvegarde : pas de runtime pour restaurer : {e}");
            return;
        }
    };
    match runtime.block_on(reglage.derniere()) {
        Ok(Some(copie)) => match installe(&chemin, &copie) {
            Ok(()) => tracing::info!(
                "base restaurée depuis {} ({} Ko compressés)",
                reglage.depot,
                copie.len() / 1024
            ),
            Err(e) => tracing::warn!("sauvegarde : restauration refusée : {e}"),
        },
        Ok(None) => tracing::info!("sauvegarde : aucune copie encore, base neuve"),
        Err(e) => tracing::warn!("sauvegarde : restauration impossible : {e}"),
    }
}

/// Un envoi d'arrêt en cours : `main` l'attend avant de rendre la main.
static DERNIER_ENVOI: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// Les envois : chaque minute si la base a changé, et une dernière fois à
/// `SIGTERM`.
pub fn demarre(db: &'static Db) {
    let Some(reglage) = Reglage::depuis_env() else {
        return;
    };
    tracing::info!("sauvegarde de la base vers {}", reglage.depot);
    let dernier: &'static Mutex<Option<[u8; 32]>> = Box::leak(Box::new(Mutex::new(None)));

    async fn tour(reglage: &Reglage, db: &Db, dernier: &Mutex<Option<[u8; 32]>>) {
        if REFUSE.load(Ordering::Relaxed) {
            return;
        }
        let copie = match cliche(db).await {
            Ok(c) => c,
            Err(e) => return tracing::warn!("sauvegarde : {e}"),
        };
        // L'empreinte de la base elle-même, pas de l'archive : gzip y glisse
        // des octets qui varient d'un cliché à l'autre.
        let empreinte: [u8; 32] = match decompresse(&copie) {
            Ok(brut) => Sha256::digest(&brut).into(),
            Err(_) => return,
        };
        if *dernier.lock().unwrap_or_else(|e| e.into_inner()) == Some(empreinte) {
            return;
        }
        match reglage.envoie(copie).await {
            Ok(()) => *dernier.lock().unwrap_or_else(|e| e.into_inner()) = Some(empreinte),
            Err(e) if !REFUSE.load(Ordering::Relaxed) => tracing::warn!("sauvegarde : {e}"),
            Err(_) => {}
        }
    }

    let r = reglage.clone();
    tokio::spawn(async move {
        let mut minute = tokio::time::interval(Duration::from_secs(60));
        loop {
            minute.tick().await;
            tour(&r, db, dernier).await;
        }
    });

    #[cfg(unix)]
    tokio::spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let Ok(mut arret) = signal(SignalKind::terminate()) else {
            return;
        };
        arret.recv().await;
        *DERNIER_ENVOI.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
        tour(&reglage, db, dernier).await;
        *DERNIER_ENVOI.0.lock().unwrap_or_else(|e| e.into_inner()) = false;
        DERNIER_ENVOI.1.notify_all();
    });
}

/// Attend, au plus `delai`, que l'envoi d'arrêt soit fini : sans cela le
/// processus sort dès que le serveur s'est arrêté, en plein envoi.
pub fn attends_dernier_envoi(delai: Duration) {
    let (verrou, signal) = &DERNIER_ENVOI;
    let en_cours = verrou.lock().unwrap_or_else(|e| e.into_inner());
    let _ = signal.wait_timeout_while(en_cours, delai, |en_cours| *en_cours);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_fichier_se_lit_dans_l_url() {
        assert_eq!(
            chemin_sqlite("sqlite://data/moncap.db"),
            Some(PathBuf::from("data/moncap.db"))
        );
        assert_eq!(
            chemin_sqlite("sqlite:base.db?mode=rwc"),
            Some(PathBuf::from("base.db"))
        );
        assert_eq!(
            chemin_sqlite("/srv/moncap.sqlite"),
            Some(PathBuf::from("/srv/moncap.sqlite"))
        );
        assert_eq!(chemin_sqlite("sqlite::memory:"), None);
        assert_eq!(chemin_sqlite("sqlite://"), None);
    }

    #[test]
    fn seul_un_depot_simple_est_accepte() {
        assert!(depot_valide("maxlestage/moncap"));
        assert!(depot_valide("a-b/c.d_e"));
        for non in ["moncap", "a/b/c", "../x", "a/..", "a/b?x", "a /b", "/b"] {
            assert!(!depot_valide(non), "{non}");
        }
    }

    #[test]
    fn la_plus_recente_vient_d_abord() {
        let pieces: Vec<Piece> = [
            nom_copie(99),
            "autre.txt".into(),
            nom_copie(1_000),
            nom_copie(5),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, name)| Piece { id: i as u64, name })
        .collect();
        let noms: Vec<&str> = copies_triees(&pieces)
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(noms, [nom_copie(1_000), nom_copie(99), nom_copie(5)]);
    }

    #[test]
    fn un_cliche_se_restaure_en_base_complete() {
        paquets::orm::block_on(async {
            let db = Db::connect("sqlite::memory:").await.unwrap();
            db.raw_execute("CREATE TABLE t (x TEXT)", &[])
                .await
                .unwrap();
            db.raw_execute("INSERT INTO t VALUES ('Zoé')", &[])
                .await
                .unwrap();
            let copie = cliche(&db).await.unwrap();

            let dossier = std::env::temp_dir().join(format!("moncap-essai-{}", std::process::id()));
            let chemin = dossier.join("sous/moncap.db");
            installe(&chemin, &copie).unwrap();
            let url = format!("sqlite://{}", chemin.display());
            let relue = Db::connect(&url).await.unwrap();
            let n = relue
                .raw_execute("UPDATE t SET x = 'Zoé' WHERE x = 'Zoé'", &[])
                .await
                .unwrap();
            assert_eq!(n, 1);
            let _ = std::fs::remove_dir_all(dossier);
        });
    }

    #[test]
    fn une_copie_qui_n_est_pas_une_base_est_refusee() {
        let chemin = std::env::temp_dir().join("moncap-refus.db");
        let faux = compresse(b"pas une base").unwrap();
        assert!(installe(&chemin, &faux).is_err());
        assert!(installe(&chemin, b"pas du gzip").is_err());
        assert!(!chemin.exists());
    }
}
