//! Les données venues d'ailleurs, relayées sans authentification : les feux
//! (NASA FIRMS), la qualité de l'air (Open-Meteo), les transports en commun
//! (Transitous), le trafic (TomTom) et le courtier des paquets hors ligne
//! (GitHub Releases).
//!
//! Les clés et le jeton restent **sur le serveur** : aucun client ne les voit,
//! et chacun profite des données sans rien configurer. Chaque source a son
//! cache mémoire, borné, pour ne pas payer l'amont à chaque requête.

mod air;
mod feux;
mod hors_ligne;
mod trafic;
mod transit;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use paquets::prelude::*;

pub fn routes(app: &mut App) {
    app.get("/fires", feux::liste);
    app.get("/air", air::liste);
    app.get("/transit", transit::liste);
    app.get("/traffic", trafic::liste);
    app.get("/offline/regions", hors_ligne::liste);
    app.get("/offline/regions/:tag/:nature", hors_ligne::fichier);
}

/// Les tâches de fond des données externes : avec une clé FIRMS, le monde
/// entier est mis en cache dès le démarrage, pour que le premier visiteur ne
/// trouve pas une carte vide.
pub fn demarre(_db: &'static Db) {
    feux::demarre();
}

/// Client HTTP sortant partagé (pool de connexions + pile TLS construits une
/// seule fois), avec un délai de 15 s et sans User-Agent par défaut. Chaque
/// appel peut allonger ou raccourcir ce délai pour lui-même.
pub fn client_http() -> &'static reqwest::Client {
    static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
    HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default()
    })
}

/// Attente bornée : au-delà, on rend la main et le fond prend le relais.
///
/// Cinq secondes. Douze laissaient le temps à la donnée d'arriver, mais une
/// carte qui ne réagit pas pendant douze secondes passe pour figée — et
/// l'attente était payée à chaque nouveau cadrage. Passé ce délai le
/// rafraîchissement continue en fond, et le client redemande : mieux vaut deux
/// allers-retours courts qu'un seul qu'on croit bloqué.
async fn attends<T>(travail: impl Future<Output = Option<T>>) -> Option<T> {
    tokio::time::timeout(Duration::from_secs(5), travail)
        .await
        .ok()
        .flatten()
}

/// Un verrou, même empoisonné : les caches ne portent que des copies de
/// données, une tâche qui a paniqué n'y a rien laissé d'incohérent.
fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// Caches par zone, bornés.
// ---------------------------------------------------------------------------

/// Une zone en cache : ses données et leur date.
struct Entree<T> {
    fetched_at: Instant,
    data: Vec<T>,
}

/// Un cache par zone demandée. Le verrou n'est jamais tenu pendant un appel
/// réseau : on lit, on relâche, on va chercher, on range.
struct Zones<T>(LazyLock<Mutex<HashMap<String, Entree<T>>>>);

impl<T: Clone> Zones<T> {
    const fn new() -> Self {
        Zones(LazyLock::new(|| Mutex::new(HashMap::new())))
    }

    /// Les données d'une zone, et si elles sont périmées.
    fn lit(&self, zone: &str, ttl: Duration) -> Option<(Vec<T>, bool)> {
        verrou(&self.0)
            .get(zone)
            .map(|c| (c.data.clone(), c.fetched_at.elapsed() >= ttl))
    }

    /// Range une zone, en faisant de la place au besoin : sans borne, une
    /// carte parcourue longuement finirait par tout garder en mémoire. La
    /// plus ancienne entrée cède la sienne.
    fn range(&self, max: usize, zone: &str, data: Vec<T>) {
        let mut table = verrou(&self.0);
        if table.len() >= max
            && !table.contains_key(zone)
            && let Some(vieille) = table
                .iter()
                .min_by_key(|(_, c)| c.fetched_at)
                .map(|(k, _)| k.clone())
        {
            table.remove(&vieille);
        }
        table.insert(
            zone.to_owned(),
            Entree {
                fetched_at: Instant::now(),
                data,
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Rafraîchissements de fond, une zone à la fois.
// ---------------------------------------------------------------------------

/// Les zones dont le rafraîchissement est en cours. Une zone à la fois, mais
/// plusieurs zones en parallèle : un seul verrou global ferait attendre
/// l'Australie derrière la Californie.
struct EnCours(LazyLock<Mutex<HashSet<String>>>);

impl EnCours {
    const fn new() -> Self {
        EnCours(LazyLock::new(|| Mutex::new(HashSet::new())))
    }

    #[cfg(test)]
    fn contient(&self, zone: &str) -> bool {
        verrou(&self.0).contains(zone)
    }
}

/// La marque « en cours » d'une zone. Elle s'efface quand la tâche se
/// termine, **quelle que soit la façon** : un retour normal, une erreur, une
/// panique ou l'arrêt du runtime. Effacée à la main en fin de tâche, comme
/// avant, une panique l'aurait laissée en place, et la zone ne se serait plus
/// jamais rafraîchie.
struct Marque {
    en_cours: &'static EnCours,
    zone: String,
}

impl Drop for Marque {
    fn drop(&mut self) {
        verrou(&self.en_cours.0).remove(&self.zone);
    }
}

/// Lance `travail` en tâche de fond, sauf si cette zone a déjà le sien en
/// cours. Renvoie `true` s'il est lancé.
fn lance_unique(
    en_cours: &'static EnCours,
    zone: &str,
    travail: impl Future<Output = ()> + Send + 'static,
) -> bool {
    if !verrou(&en_cours.0).insert(zone.to_owned()) {
        return false;
    }
    let marque = Marque {
        en_cours,
        zone: zone.to_owned(),
    };
    tokio::spawn(async move {
        // Tenue jusqu'au bout, et lâchée même si `travail` panique : le
        // déroulement de la pile passe par ici.
        let _marque = marque;
        travail.await;
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une tâche qui panique ne doit pas bloquer sa zone pour toujours.
    #[tokio::test]
    async fn une_tache_qui_panique_libere_sa_zone() {
        static EN_COURS: EnCours = EnCours::new();
        assert!(lance_unique(&EN_COURS, "z", async {
            panic!("boum (attendu par le test)")
        }));
        for _ in 0..200 {
            if !EN_COURS.contient("z") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(!EN_COURS.contient("z"), "la zone est restée bloquée");
        assert!(lance_unique(&EN_COURS, "z", async {}));
    }

    /// Une zone à la fois : le second rafraîchissement attend que le premier
    /// ait fini, une autre zone part tout de suite.
    #[tokio::test]
    async fn une_seule_tache_par_zone() {
        static EN_COURS: EnCours = EnCours::new();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        assert!(lance_unique(&EN_COURS, "a", async {
            let _ = rx.await;
        }));
        assert!(!lance_unique(&EN_COURS, "a", async {}));
        assert!(lance_unique(&EN_COURS, "b", async {}));
        tx.send(()).unwrap();
        for _ in 0..200 {
            if !EN_COURS.contient("a") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(lance_unique(&EN_COURS, "a", async {}));
    }

    #[test]
    fn le_cache_evince_la_plus_ancienne_zone() {
        static ZONES: Zones<u8> = Zones::new();
        ZONES.range(2, "a", vec![1]);
        ZONES.range(2, "b", vec![2]);
        // Remplacer une zone connue ne chasse personne.
        ZONES.range(2, "b", vec![3]);
        assert_eq!(
            ZONES.lit("a", Duration::from_secs(60)),
            Some((vec![1], false))
        );
        ZONES.range(2, "c", vec![4]);
        assert!(ZONES.lit("a", Duration::from_secs(60)).is_none());
        assert_eq!(
            ZONES.lit("b", Duration::from_secs(60)),
            Some((vec![3], false))
        );
        assert_eq!(ZONES.lit("c", Duration::ZERO), Some((vec![4], true)));
    }

    // --- De bout en bout, sans réseau ------------------------------------

    use crate::essais::client_ou_rien;
    use serde_json::Value;

    #[tokio::test]
    async fn une_zone_mal_formee_est_refusee() {
        let c = client_ou_rien!();
        for route in ["/fires", "/air"] {
            let r = c.get(&format!("{route}?bbox=10,41,-5,51")).await;
            assert_eq!(r.status(), 400, "{route}");
            assert_eq!(r.text(), "bbox attendue : ouest,sud,est,nord");
        }
    }

    #[tokio::test]
    async fn sans_cle_les_feux_sont_vides() {
        let c = client_ou_rien!();
        if feux::fires_key().is_some() {
            return;
        }
        let r = c.get("/fires?bbox=-5,41,10,51").await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.json::<Value>(), serde_json::json!([]));
    }

    #[tokio::test]
    async fn le_transit_exige_deux_points() {
        let c = client_ou_rien!();
        for q in ["", "?from=44.88,-0.6", "?from=91,0&to=44.8,-0.6"] {
            let r = c.get(&format!("/transit{q}")).await;
            assert_eq!(r.status(), 400, "{q}");
            assert_eq!(r.text(), "from et to attendus : lat,lon");
        }
    }

    #[tokio::test]
    async fn le_trafic_exige_une_emprise_et_se_tait_sans_cle() {
        let c = client_ou_rien!();
        let r = c.get("/traffic").await;
        assert_eq!(r.status(), 400);
        if trafic::tomtom_key().is_some() {
            return;
        }
        let r = c.get("/traffic?bbox=-0.7,44.8,-0.5,44.9").await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.json::<Value>(), serde_json::json!([]));
    }

    /// Sans jeton, le courtier doit le **dire**. La panne d'origine tenait à un
    /// message qui parlait de réseau alors que le réseau allait bien : l'app
    /// affichait « il faut du réseau une fois » en pleine 5G.
    #[tokio::test]
    async fn sans_jeton_le_courtier_l_annonce() {
        // Le test ne vaut que sans jeton dans l'environnement — sinon le
        // serveur appellerait GitHub pour de bon.
        if hors_ligne::paquets_token().is_some() {
            return;
        }
        let c = client_ou_rien!();
        for chemin in [
            "/offline/regions",
            "/offline/regions/tuiles-eu-monaco/archive",
        ] {
            let r = c.get(chemin).await;
            assert_eq!(r.status(), 503, "{chemin}");
            assert!(r.text().contains("GITHUB_TOKEN"), "{}", r.text());
        }
    }

    /// Une nature de fichier inconnue ne doit pas atteindre GitHub.
    #[tokio::test]
    async fn une_nature_inconnue_est_refusee() {
        let c = client_ou_rien!();
        let r = c.get("/offline/regions/tuiles-eu-monaco/README").await;
        // Sans jeton la route s'arrête avant ; avec jeton, la nature est
        // refusée. Dans les deux cas, rien n'est servi.
        assert!(r.status() == 404 || r.status() == 503, "{}", r.status());
        // Une étiquette hors du préfixe non plus.
        let r = c.get("/offline/regions/v1.1.0/archive").await;
        assert!(r.status() == 404 || r.status() == 503, "{}", r.status());
    }
}
