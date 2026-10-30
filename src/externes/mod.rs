//! Les données venues d'ailleurs, relayées sans authentification : les feux
//! (NASA FIRMS), la qualité de l'air (Open-Meteo), les transports en commun
//! (Transitous), le trafic (TomTom) et le courtier des paquets hors ligne
//! (GitHub Releases).
//!
//! Les clés et le jeton restent **sur le serveur** : aucun client ne les voit,
//! et chacun profite des données sans rien configurer. Chaque source a son
//! cache mémoire, borné, pour ne pas payer l'amont à chaque requête. Les
//! feux, l'air et la liste des régions sont aussi gardés dans le cache SQLite
//! (voir `crate::cache`) : après un redémarrage, la carte se remplit aussitôt
//! de la dernière donnée connue, rafraîchie derrière. Le trafic et les
//! horaires vivent une minute ou deux : la mémoire leur suffit.

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
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::cache;

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
pub fn demarre(cache: &'static Db) {
    feux::demarre(cache);
}

/// Un constructeur de client HTTP sortant, TLS prêt : reqwest 0.13 ne choisit
/// plus son fournisseur cryptographique quand on écarte aws-lc, on installe
/// donc `ring` — le même que le reste du serveur — une fois pour toutes.
pub fn constructeur_http() -> reqwest::ClientBuilder {
    // Déjà installé (un autre client, un autre essai) : rien à faire.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
}

/// Client HTTP sortant partagé (pool de connexions + pile TLS construits une
/// seule fois), avec un délai de 15 s et sans User-Agent par défaut. Chaque
/// appel peut allonger ou raccourcir ce délai pour lui-même.
pub fn client_http() -> &'static reqwest::Client {
    static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
    HTTP.get_or_init(|| {
        constructeur_http()
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
struct Zones<T> {
    table: LazyLock<Mutex<HashMap<String, Entree<T>>>>,
    /// Le nom de la source, qui préfixe ses clés dans le cache SQLite.
    nom: &'static str,
}

/// Une zone gardée sur disque depuis plus d'un jour ne vaut plus d'être
/// montrée, même le temps d'un rafraîchissement.
const AGE_MAX_DISQUE: i64 = 24 * 3600;

impl<T: Clone> Zones<T> {
    const fn new(nom: &'static str) -> Self {
        Zones {
            table: LazyLock::new(|| Mutex::new(HashMap::new())),
            nom,
        }
    }

    /// Les données d'une zone, et si elles sont périmées.
    fn lit(&self, zone: &str, ttl: Duration) -> Option<(Vec<T>, bool)> {
        verrou(&self.table)
            .get(zone)
            .map(|c| (c.data.clone(), c.fetched_at.elapsed() >= ttl))
    }

    /// Range une zone, en faisant de la place au besoin : sans borne, une
    /// carte parcourue longuement finirait par tout garder en mémoire. La
    /// plus ancienne entrée cède la sienne.
    fn range(&self, max: usize, zone: &str, data: Vec<T>) {
        self.range_du(max, zone, data, Instant::now());
    }

    /// Range une zone relevée à `fetched_at`.
    fn range_du(&self, max: usize, zone: &str, data: Vec<T>, fetched_at: Instant) {
        let mut table = verrou(&self.table);
        if table.len() >= max
            && !table.contains_key(zone)
            && let Some(vieille) = table
                .iter()
                .min_by_key(|(_, c)| c.fetched_at)
                .map(|(k, _)| k.clone())
        {
            table.remove(&vieille);
        }
        table.insert(zone.to_owned(), Entree { fetched_at, data });
    }

    fn cle(&self, zone: &str) -> String {
        format!("{}:{zone}", self.nom)
    }
}

impl<T: Clone + Serialize + DeserializeOwned> Zones<T> {
    /// Les données d'une zone, de la mémoire ou à défaut du cache SQLite (et
    /// alors remises en mémoire avec leur âge), et si elles sont périmées.
    async fn lit_ou_disque(
        &self,
        disque: &Db,
        max: usize,
        zone: &str,
        ttl: Duration,
    ) -> Option<(Vec<T>, bool)> {
        if let Some(trouve) = self.lit(zone, ttl) {
            return Some(trouve);
        }
        let (data, age) = cache::lit_json::<Vec<T>>(disque, &self.cle(zone)).await?;
        if age > AGE_MAX_DISQUE {
            return None;
        }
        let age = Duration::from_secs(age.max(0) as u64);
        let fetched_at = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);
        self.range_du(max, zone, data.clone(), fetched_at);
        Some((data, age >= ttl))
    }

    /// Range une zone en mémoire et dans le cache SQLite.
    async fn range_et_disque(&self, disque: &Db, max: usize, zone: &str, data: Vec<T>) {
        cache::range_json(disque, &self.cle(zone), &data).await;
        self.range(max, zone, data);
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
        static ZONES: Zones<u8> = Zones::new("essai");
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

    /// Une zone gardée sur disque revient en mémoire avec son âge : périmée
    /// si elle a passé sa durée de vie, ignorée au-delà d'un jour.
    #[tokio::test]
    async fn une_zone_revient_du_disque_avec_son_age() {
        static ZONES: Zones<u8> = Zones::new("essai-disque");
        let disque = cache::ouvre("sqlite::memory:");
        cache::prepare(&disque).await.unwrap();
        assert!(
            ZONES
                .lit_ou_disque(&disque, 4, "z", Duration::from_secs(60))
                .await
                .is_none()
        );
        ZONES.range_et_disque(&disque, 4, "z", vec![7]).await;
        // La mémoire perdue (un redémarrage), le disque répond.
        verrou(&ZONES.table).clear();
        assert_eq!(
            ZONES
                .lit_ou_disque(&disque, 4, "z", Duration::from_secs(60))
                .await,
            Some((vec![7], false))
        );
        verrou(&ZONES.table).clear();
        assert_eq!(
            ZONES.lit_ou_disque(&disque, 4, "z", Duration::ZERO).await,
            Some((vec![7], true))
        );
        // Vieille de deux jours : plus montrée.
        verrou(&ZONES.table).clear();
        use paquets::orm::{Executor, params};
        disque
            .raw_execute(
                "UPDATE cache SET range_a = ?1",
                &params![crate::base::maintenant() - 2 * AGE_MAX_DISQUE],
            )
            .await
            .unwrap();
        assert!(
            ZONES
                .lit_ou_disque(&disque, 4, "z", Duration::from_secs(60))
                .await
                .is_none()
        );
    }

    // --- De bout en bout, sans réseau ------------------------------------

    use crate::essais::client;
    use serde_json::Value;

    #[tokio::test]
    async fn une_zone_mal_formee_est_refusee() {
        let c = client().await;
        for route in ["/fires", "/air"] {
            let r = c.get(&format!("{route}?bbox=10,41,-5,51")).await;
            assert_eq!(r.status(), 400, "{route}");
            assert_eq!(r.text(), "bbox attendue : ouest,sud,est,nord");
        }
    }

    #[tokio::test]
    async fn sans_cle_les_feux_sont_vides() {
        let c = client().await;
        if feux::fires_key().is_some() {
            return;
        }
        let r = c.get("/fires?bbox=-5,41,10,51").await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.json::<Value>(), serde_json::json!([]));
    }

    #[tokio::test]
    async fn le_transit_exige_deux_points() {
        let c = client().await;
        for q in ["", "?from=44.88,-0.6", "?from=91,0&to=44.8,-0.6"] {
            let r = c.get(&format!("/transit{q}")).await;
            assert_eq!(r.status(), 400, "{q}");
            assert_eq!(r.text(), "from et to attendus : lat,lon");
        }
    }

    #[tokio::test]
    async fn le_trafic_exige_une_emprise_et_se_tait_sans_cle() {
        let c = client().await;
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
        let c = client().await;
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
        let c = client().await;
        let r = c.get("/offline/regions/tuiles-eu-monaco/README").await;
        // Sans jeton la route s'arrête avant ; avec jeton, la nature est
        // refusée. Dans les deux cas, rien n'est servi.
        assert!(r.status() == 404 || r.status() == 503, "{}", r.status());
        // Une étiquette hors du préfixe non plus.
        let r = c.get("/offline/regions/v1.1.0/archive").await;
        assert!(r.status() == 404 || r.status() == 503, "{}", r.status());
    }
}
