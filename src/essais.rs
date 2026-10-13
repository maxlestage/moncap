//! Essais de bout en bout, contre un vrai Postgres.
//!
//! Ils tournent dès que `TEST_DATABASE_URL` (ou à défaut `DATABASE_URL`) est
//! défini, et se taisent sinon (utile en local sans base). Chaque essai crée
//! ses propres comptes, aux noms uniques : ils peuvent tourner en parallèle
//! sur la même base.

use std::sync::atomic::{AtomicU64, Ordering};

use paquets::test::TestClient;
use serde_json::{Value, json};

use crate::app::{Reglages, application};

/// Un client de test sur la base d'essai, ou `None` sans base.
pub async fn client() -> Option<TestClient> {
    let url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()?;
    Some(
        application(Reglages {
            base: url,
            limites: false,
            taches: false,
        })
        .test()
        .await,
    )
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

macro_rules! client_ou_rien {
    () => {
        match crate::essais::client().await {
            Some(c) => c,
            None => return,
        }
    };
}
pub(crate) use client_ou_rien;

mod connexion {
    use super::*;

    #[tokio::test]
    async fn inscription_puis_compte_puis_401_sans_jeton() {
        let c = client_ou_rien!();
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
        let c = client_ou_rien!();
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
        let c = client_ou_rien!();
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
        let c = client_ou_rien!();
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
        let c = client_ou_rien!();
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
        let c = client_ou_rien!();
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
        let c = client_ou_rien!();
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
        let c = client_ou_rien!();
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
        let c = client_ou_rien!();
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
        let c = client_ou_rien!();
        if crate::connexion::smtp_configure() {
            return;
        }
        let r = c.post("/auth/forgot").json(&json!({"username": "x"})).await;
        assert_eq!(r.status(), 503);
    }

    #[tokio::test]
    async fn la_reinitialisation_change_le_mot_de_passe_une_seule_fois() {
        use paquets::orm::{Executor, params};
        let c = client_ou_rien!();
        let (nom, jeton_session) = inscrit(&c, "reinit").await;
        // Le lien qu'aurait reçu le courriel.
        let db = paquets::Db::connect(
            &std::env::var("TEST_DATABASE_URL")
                .or_else(|_| std::env::var("DATABASE_URL"))
                .unwrap(),
        )
        .await
        .unwrap();
        let jeton = crate::connexion::jeton_aleatoire();
        db.raw_execute(
            "INSERT INTO reset_tokens (user_id, token_hash, expires_at, used) \
             SELECT id, $2, $3, FALSE FROM users WHERE username = $1",
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
        let c = client_ou_rien!();
        let r = c.get("/health").await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.text(), "ok");
    }

    /// Un ancien jeton JWT, comme ceux de l'ancien serveur.
    fn ancien_jwt(sub: i32, exp: i64) -> Option<String> {
        use jsonwebtoken::{EncodingKey, Header, encode};
        let secret = crate::connexion::secret_jwt()?;
        Some(
            encode(
                &Header::default(),
                &crate::connexion::Claims { sub, exp },
                &EncodingKey::from_secret(secret),
            )
            .unwrap(),
        )
    }

    async fn id_de(c: &TestClient, jeton: &str) -> i32 {
        // L'export du compte n'est pas encore là : on lit l'id en base.
        let _ = c;
        let db = paquets::Db::connect(
            &std::env::var("TEST_DATABASE_URL")
                .or_else(|_| std::env::var("DATABASE_URL"))
                .unwrap(),
        )
        .await
        .unwrap();
        use paquets::orm::{Executor, params};
        let lignes: Vec<(i32,)> = db
            .raw(
                "SELECT user_id FROM sessions WHERE token_hash = $1",
                &params![crate::connexion::empreinte(jeton)],
            )
            .await
            .unwrap();
        lignes[0].0
    }

    #[tokio::test]
    async fn les_anciens_jwt_restent_valables_et_revocables() {
        let c = client_ou_rien!();
        let (_, jeton) = inscrit(&c, "jwt").await;
        let id = id_de(&c, &jeton).await;
        let exp = crate::base::maintenant() + 29 * 24 * 3600;
        // Sans JWT_SECRET, les anciens jetons sont refusés : rien à essayer.
        let Some(jwt) = ancien_jwt(id, exp) else {
            return;
        };
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&jwt))
            .await;
        assert_eq!(r.status(), 200, "un ancien jeton doit encore passer");
        // Se déconnecter avec révoque les anciens jetons du compte…
        let r = c
            .post("/auth/logout")
            .header("authorization", &bearer(&jwt))
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&jwt))
            .await;
        assert_eq!(r.status(), 401);
        // … sans toucher aux sessions.
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        // Un JWT d'un compte qui n'existe pas ne passe pas.
        let fantome = ancien_jwt(i32::MAX, exp).unwrap();
        let r = c
            .get("/auth/me")
            .header("authorization", &bearer(&fantome))
            .await;
        assert_eq!(r.status(), 401);
    }

    #[tokio::test]
    async fn le_websocket_exige_un_jeton_valide() {
        let c = client_ou_rien!();
        let r = c.get("/ws").await;
        assert_eq!(r.status(), 400);
        let r = c.get("/ws?token=mc_faux").await;
        assert_eq!(r.status(), 401);
        assert_eq!(r.text(), "authentification requise");
    }

    #[tokio::test]
    async fn les_erreurs_du_bouclier_sont_du_texte() {
        let c = client_ou_rien!();
        let r = c.get("/?q=1%27%20OR%201%3D1--").await;
        assert_eq!(r.status(), 403);
        assert_eq!(r.header("content-type"), Some("text/plain; charset=utf-8"));
    }
}
