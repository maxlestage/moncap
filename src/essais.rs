//! Essais de bout en bout, sur une vraie base SQLite.
//!
//! Chaque essai a la sienne, en mémoire, toute neuve, avec ses migrations :
//! rien à installer, rien à configurer, et ils tournent tous, en parallèle,
//! à chaque `cargo test`.

use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};

use paquets::test::TestClient;
use serde_json::{Value, json};

use crate::app::{Reglages, application};
use crate::cache;

/// Un client sur une application neuve, et la base qu'elle utilise — les
/// essais y lisent ou y préparent ce que l'API ne montre pas.
pub struct Essai {
    client: TestClient,
    pub db: paquets::Db,
}

impl Deref for Essai {
    type Target = TestClient;

    fn deref(&self) -> &TestClient {
        &self.client
    }
}

/// Une application neuve, sur une base SQLite en mémoire qui n'appartient
/// qu'à cet essai.
pub async fn client() -> Essai {
    let db = paquets::Db::connect("sqlite::memory:")
        .await
        .expect("SQLite en mémoire");
    let client = application(Reglages {
        base: db.clone().into(),
        cache: cache::ouvre("sqlite::memory:"),
        limites: false,
        taches: false,
    })
    .test()
    .await;
    Essai { client, db }
}

/// Un nom de compte que personne d'autre n'a.
pub fn nom_unique(prefixe: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{prefixe}-{t}-{}@essai.fr",
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// Inscrit un compte ; renvoie (nom, jeton).
pub async fn inscrit(c: &TestClient, prefixe: &str) -> (String, String) {
    let nom = nom_unique(prefixe);
    let r = c
        .post("/auth/signup")
        .json(&json!({"username": nom, "password": "secret1"}))
        .await;
    assert_eq!(r.status(), 200, "{}", r.text());
    let v: Value = r.json();
    (nom, v["token"].as_str().unwrap().to_owned())
}

pub fn bearer(jeton: &str) -> String {
    format!("Bearer {jeton}")
}

mod connexion {
    use super::*;

    #[tokio::test]
    async fn inscription_puis_compte_puis_401_sans_jeton() {
        let c = client().await;
        let (nom, jeton) = inscrit(&c, "insc").await;
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.json::<Value>()["username"], nom.as_str());
        let r = c.get("/auth/me").await;
        assert_eq!(r.status(), 401);
        assert_eq!(r.text(), "authentification requise");
        let r = c
            .get("/auth/me")
            .header("authorization", "Bearer mc_faux")
            .await;
        assert_eq!(r.status(), 401);
    }

    #[tokio::test]
    async fn la_connexion_ignore_la_casse_et_les_espaces() {
        let c = client().await;
        let (nom, _) = inscrit(&c, "Casse").await;
        let r = c
            .post("/auth/login")
            .json(&json!({"username": format!("  {} ", nom.to_uppercase()), "password": "secret1"}))
            .await;
        assert_eq!(r.status(), 200, "{}", r.text());
        let v: Value = r.json();
        assert_eq!(v["username"], nom.as_str());
        assert!(v["token"].as_str().unwrap().starts_with("mc_"));
        assert!(v["expires_at"].as_i64().unwrap() > 0);
    }

    #[tokio::test]
    async fn un_doublon_a_la_casse_pres_est_refuse() {
        let c = client().await;
        let (nom, _) = inscrit(&c, "dup").await;
        let r = c
            .post("/auth/signup")
            .json(&json!({"username": nom.to_uppercase(), "password": "secret1"}))
            .await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.text(), "nom d'utilisateur déjà pris");
    }

    #[tokio::test]
    async fn mauvais_mot_de_passe_refuse_avec_un_message_clair() {
        let c = client().await;
        let (nom, _) = inscrit(&c, "mdp").await;
        let r = c
            .post("/auth/login")
            .json(&json!({"username": nom, "password": "mauvais"}))
            .await;
        assert_eq!(r.status(), 401);
        assert_eq!(r.text(), "identifiant ou mot de passe incorrect");
        let r = c
            .post("/auth/login")
            .json(&json!({"username": nom_unique("inconnu"), "password": "secret1"}))
            .await;
        assert_eq!(r.status(), 401);
    }

    #[tokio::test]
    async fn les_devinettes_sont_freinees_par_compte() {
        let c = client().await;
        let (nom, _) = inscrit(&c, "frein").await;
        let mut dernier = 0;
        for _ in 0..10 {
            let r = c
                .post("/auth/login")
                .json(&json!({"username": nom, "password": "mauvais"}))
                .await;
            dernier = r.status().as_u16();
        }
        assert_eq!(dernier, 429);
        // Même le bon mot de passe attend la fin du blocage.
        let r = c
            .post("/auth/login")
            .json(&json!({"username": nom, "password": "secret1"}))
            .await;
        assert_eq!(r.status(), 429);
        assert!(r.header("retry-after").is_some());
        assert!(r.text().contains("réessaie dans"));
        // Un autre compte n'est pas touché.
        let (autre, _) = inscrit(&c, "frein-autre").await;
        let r = c
            .post("/auth/login")
            .json(&json!({"username": autre, "password": "secret1"}))
            .await;
        assert_eq!(r.status(), 200);
    }

    #[tokio::test]
    async fn champs_invalides() {
        let c = client().await;
        let r = c
            .post("/auth/signup")
            .json(&json!({"username": "ab", "password": "secret1"}))
            .await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.text(), "nom (≥3) et mot de passe (≥6) requis");
        let r = c
            .post("/auth/signup")
            .json(&json!({"username": nom_unique("long"), "password": "p".repeat(500)}))
            .await;
        assert_eq!(r.status(), 400);
        let r = c
            .post("/auth/signup")
            .json(&json!({"username": "abc"}))
            .await;
        assert_eq!(r.status(), 422);
        let r = c
            .post("/auth/signup")
            .header("content-type", "application/json")
            .body("{pas du json")
            .await;
        assert_eq!(r.status(), 400);
    }

    #[tokio::test]
    async fn la_deconnexion_revoque_vraiment_le_jeton() {
        let c = client().await;
        let (nom, jeton) = inscrit(&c, "logout").await;
        let r = c
            .post("/auth/login")
            .json(&json!({"username": nom, "password": "secret1"}))
            .await;
        let autre = r.json::<Value>()["token"].as_str().unwrap().to_owned();
        let r = c
            .post("/auth/logout")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 401);
        // L'autre appareil reste connecté.
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&autre))
            .await;
        assert_eq!(r.status(), 200);
    }

    #[tokio::test]
    async fn sessions_listees_revoquees_et_deconnexion_partout() {
        let c = client().await;
        let (nom, jeton) = inscrit(&c, "sessions").await;
        let r = c
            .post("/auth/login")
            .header("user-agent", "MonCapGPS/2 iPhone")
            .json(&json!({"username": nom, "password": "secret1"}))
            .await;
        let deux = r.json::<Value>()["token"].as_str().unwrap().to_owned();
        let r = c
            .get("/auth/sessions")
            .header("authorization", &bearer(&jeton))
            .await;
        let liste: Vec<Value> = r.json();
        assert_eq!(liste.len(), 2);
        assert_eq!(liste.iter().filter(|s| s["current"] == true).count(), 1);
        let iphone = liste
            .iter()
            .find(|s| s["device"] == "MonCapGPS/2 iPhone")
            .unwrap();
        let r = c
            .delete(&format!("/auth/sessions/{}", iphone["id"]))
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&deux))
            .await;
        assert_eq!(r.status(), 401);
        let r = c
            .post("/auth/logout-all")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 401);
    }

    #[tokio::test]
    async fn changer_de_mot_de_passe_coupe_les_autres_sessions() {
        let c = client().await;
        let (nom, jeton) = inscrit(&c, "chg").await;
        let r = c
            .post("/auth/password")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"current_password": "faux", "new_password": "nouveau1"}))
            .await;
        assert_eq!(r.status(), 401);
        assert_eq!(r.text(), "mot de passe actuel incorrect");
        let r = c
            .post("/auth/password")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"current_password": "secret1", "new_password": "nouveau1"}))
            .await;
        assert_eq!(r.status(), 200);
        let neuf = r.json::<Value>()["token"].as_str().unwrap().to_owned();
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 401);
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&neuf))
            .await;
        assert_eq!(r.status(), 200);
        let r = c
            .post("/auth/login")
            .json(&json!({"username": nom, "password": "nouveau1"}))
            .await;
        assert_eq!(r.status(), 200);
    }

    #[tokio::test]
    async fn forgot_sans_smtp_repond_503() {
        let c = client().await;
        if crate::connexion::smtp_configure() {
            return;
        }
        let r = c.post("/auth/forgot").json(&json!({"username": "x"})).await;
        assert_eq!(r.status(), 503);
    }

    #[tokio::test]
    async fn la_reinitialisation_change_le_mot_de_passe_une_seule_fois() {
        use paquets::orm::{Executor, params};
        let c = client().await;
        let (nom, jeton_session) = inscrit(&c, "reinit").await;
        // Le lien qu'aurait reçu le courriel.
        let jeton = crate::connexion::jeton_aleatoire();
        c.db.raw_execute(
            "INSERT INTO reset_tokens (user_id, token_hash, expires_at, used) \
             SELECT id, ?2, ?3, FALSE FROM users WHERE username = ?1",
            &params![
                nom.clone(),
                crate::connexion::empreinte(&jeton),
                crate::base::maintenant() + 3600
            ],
        )
        .await
        .unwrap();

        let r = c.get(&format!("/reinitialisation?jeton={jeton}")).await;
        assert_eq!(r.status(), 200);
        assert!(r.text().contains("Nouveau mot de passe"));
        let r = c
            .post("/auth/reset")
            .form(&[
                ("jeton", "0".repeat(32)),
                ("mot_de_passe", "nouveau1".into()),
            ])
            .await;
        assert_eq!(r.status(), 400);
        let r = c
            .post("/auth/reset")
            .form(&[("jeton", jeton.clone()), ("mot_de_passe", "court".into())])
            .await;
        assert_eq!(r.status(), 400);
        let r = c
            .post("/auth/reset")
            .form(&[
                ("jeton", jeton.clone()),
                ("mot_de_passe", "nouveau1".into()),
            ])
            .await;
        assert_eq!(r.status(), 200, "{}", r.text());
        // La session ouverte avant la réinitialisation est coupée.
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&jeton_session))
            .await;
        assert_eq!(r.status(), 401);
        let r = c
            .post("/auth/login")
            .json(&json!({"username": nom, "password": "secret1"}))
            .await;
        assert_eq!(r.status(), 401);
        let r = c
            .post("/auth/login")
            .json(&json!({"username": nom, "password": "nouveau1"}))
            .await;
        assert_eq!(r.status(), 200);
        let r = c
            .post("/auth/reset")
            .form(&[("jeton", jeton), ("mot_de_passe", "encoreun".into())])
            .await;
        assert_eq!(r.status(), 400);
    }

    #[tokio::test]
    async fn health_ok_quand_la_base_repond() {
        let c = client().await;
        let r = c.get("/health").await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.text(), "ok");
    }

    /// Un jeton qui n'est pas une session — un ancien JWT, par exemple — ne
    /// passe jamais : la base d'avant n'existe plus, et ses identifiants
    /// désigneraient aujourd'hui d'autres comptes.
    #[tokio::test]
    async fn un_jeton_qui_n_est_pas_une_session_est_refuse() {
        let c = client().await;
        inscrit(&c, "jwt").await;
        let ancien = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOjEsImV4cCI6OTk5OTk5OTk5OX0.c2lnbmF0dXJl";
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(ancien))
            .await;
        assert_eq!(r.status(), 401);
        assert_eq!(r.text(), "authentification requise");
    }

    /// Le frein aux devinettes : au dixième échec en un quart d'heure, le
    /// compte attend, même avec le bon mot de passe ; une inscription
    /// simultanée du même nom ne crée qu'un compte.
    #[tokio::test]
    async fn le_frein_aux_devinettes_bloque_au_dixieme_echec() {
        let c = client().await;
        let (nom, _) = inscrit(&c, "frein").await;
        for i in 1..=9 {
            let r = c
                .post("/auth/login")
                .json(&json!({"username": nom, "password": "mauvais"}))
                .await;
            assert_eq!(r.status(), 401, "échec {i}");
        }
        let r = c
            .post("/auth/login")
            .json(&json!({"username": nom.to_uppercase(), "password": "mauvais"}))
            .await;
        assert_eq!(r.status(), 429);
        assert!(r.header("retry-after").is_some());
        let r = c
            .post("/auth/login")
            .json(&json!({"username": nom, "password": "secret1"}))
            .await;
        assert_eq!(r.status(), 429, "le bon mot de passe attend aussi");
    }

    #[tokio::test]
    async fn deux_inscriptions_simultanees_ne_font_qu_un_compte() {
        let c = client().await;
        let nom = nom_unique("course");
        let corps = json!({"username": nom, "password": "secret1"});
        let (a, b) = tokio::join!(
            c.post("/auth/signup").json(&corps),
            c.post("/auth/signup").json(&corps)
        );
        let mut statuts = [a.status(), b.status()];
        statuts.sort();
        assert_eq!(statuts, [200, 400]);
        use paquets::orm::{Executor, params};
        let n: Vec<(i64,)> =
            c.db.raw(
                "SELECT count(*) FROM users WHERE cle = ?1",
                &params![crate::connexion::cle_identifiant(&nom)],
            )
            .await
            .unwrap();
        assert_eq!(n[0].0, 1);
    }

    #[tokio::test]
    async fn les_sessions_se_listent_et_se_revoquent() {
        let c = client().await;
        let (nom, un) = inscrit(&c, "sessions").await;
        let r = c
            .post("/auth/login")
            .header("user-agent", "MonCapGPS/2 iPad")
            .json(&json!({"username": nom, "password": "secret1"}))
            .await;
        let deux = r.json::<Value>()["token"].as_str().unwrap().to_owned();
        let r = c
            .get("/auth/sessions")
            .header("authorization", &bearer(&un))
            .await;
        let liste: Vec<Value> = r.json();
        assert_eq!(liste.len(), 2);
        assert_eq!(liste.iter().filter(|s| s["current"] == true).count(), 1);
        let ipad = liste
            .iter()
            .find(|s| s["device"] == "MonCapGPS/2 iPad")
            .expect("la session de l'iPad");
        let r = c
            .delete(&format!("/auth/sessions/{}", ipad["id"]))
            .header("authorization", &bearer(&un))
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&deux))
            .await;
        assert_eq!(r.status(), 401);
        let r = c
            .post("/auth/logout")
            .header("authorization", &bearer(&un))
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&un))
            .await;
        assert_eq!(r.status(), 401);
    }

    #[tokio::test]
    async fn le_websocket_exige_un_jeton_valide() {
        let c = client().await;
        let r = c.get("/ws").await;
        assert_eq!(r.status(), 400);
        let r = c.get("/ws?token=mc_faux").await;
        assert_eq!(r.status(), 401);
        assert_eq!(r.text(), "authentification requise");
    }

    #[tokio::test]
    async fn les_erreurs_du_bouclier_sont_du_texte() {
        let c = client().await;
        let r = c.get("/?q=1%27%20OR%201%3D1--").await;
        assert_eq!(r.status(), 403);
        assert_eq!(r.header("content-type"), Some("text/plain; charset=utf-8"));
    }
}
