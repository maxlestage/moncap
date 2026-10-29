//! Le compte : ses chiffres, son export (RGPD), ses préférences, sa
//! suppression ; le classement des contributeurs ; les rapports de plantage.

use paquets::orm::{Database, Executor, Transaction, params};
use paquets::prelude::*;
use serde::Serialize;
use serde_json::Value;

use crate::base::{Position, Recherche, Signalement, Trajet, Utilisateur};
use crate::connexion;
use crate::erreur::{Erreur, Resultat};
use crate::temps_reel::{Evenement, hub};

/// Taille maximale des préférences, en octets.
const MAX_PREFS: usize = 4096;
/// Ce qu'on garde d'un rapport de plantage, en caractères.
const MAX_DIAGNOSTIC: usize = 16_000;

pub fn routes(app: &mut App) {
    app.get("/account", infos);
    app.delete("/account", supprime);
    app.get("/account/export", export);
    app.get("/prefs", lit_prefs);
    app.put("/prefs", ecrit_prefs);
    app.get("/leaderboard", classement);
    app.post("/diagnostics", diagnostics);
}

#[derive(Serialize)]
struct AccountInfo {
    username: String,
    points: i64,
    alerts: u64,
    trips: u64,
    positions: u64,
    searches: u64,
    /// MonCap Plus offert par la maison : le pseudo figure dans la liste
    /// d'invités (variable PLUS_OFFERT). Une vieille app ignore le champ.
    plus_offert: bool,
    /// Secondes Unix de la création du compte : l'ancre de l'essai de
    /// quatorze jours, tenue côté serveur — l'horloge d'un téléphone se
    /// triche, pas la nôtre. Une vieille app ignore le champ.
    created_at: i64,
}

/// Le pseudo figure-t-il dans une liste d'invités ? Pseudos séparés par des
/// virgules, espaces tolérés, casse ignorée — la liste est tapée à la main
/// dans la configuration Heroku, elle doit pardonner.
fn est_invite(liste: &str, username: &str) -> bool {
    liste.split(',').any(|p| {
        let p = p.trim();
        !p.is_empty() && p.eq_ignore_ascii_case(username.trim())
    })
}

/// Les invités de la maison : MonCap Plus offert, sans achat. La liste vit
/// dans la variable d'environnement PLUS_OFFERT — ajouter un ami, c'est
/// ajouter son pseudo dans la configuration, sans déploiement.
fn plus_offert(username: &str) -> bool {
    est_invite(&std::env::var("PLUS_OFFERT").unwrap_or_default(), username)
}

/// Barème des points : signaler rapporte le plus (c'est ce qui aide les autres).
fn score(alerts: u64, trips: u64, positions: u64, searches: u64) -> i64 {
    (alerts * 10 + trips * 5 + positions * 2 + searches) as i64
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

/// Le compte de la requête, ou 401 s'il n'existe plus.
async fn compte(req: &Request) -> Resultat<Utilisateur> {
    let qui = connexion::utilisateur(req).await?;
    Utilisateur::find(req.db(), qui.user_id)
        .await?
        .ok_or(Erreur::NonAuthentifie)
}

/// GET /account — infos du compte et points de contribution.
async fn infos(req: Request) -> Resultat<Response> {
    let u = compte(&req).await?;
    // Les quatre décomptes en une seule requête, chacun sur son index.
    let comptes: Vec<(i64, i64, i64, i64)> = req
        .db()
        .raw(
            "SELECT (SELECT count(*) FROM alerts WHERE user_id = ?1), \
             (SELECT count(*) FROM trips WHERE user_id = ?1), \
             (SELECT count(*) FROM positions WHERE user_id = ?1), \
             (SELECT count(*) FROM searches WHERE user_id = ?1)",
            &params![u.id],
        )
        .await?;
    let (alerts, trips, positions, searches) = comptes.first().copied().unwrap_or_default();
    let (alerts, trips, positions, searches) = (
        alerts.max(0) as u64,
        trips.max(0) as u64,
        positions.max(0) as u64,
        searches.max(0) as u64,
    );
    Ok(Response::new().json(AccountInfo {
        plus_offert: plus_offert(&u.username),
        created_at: u.created_at,
        username: u.username,
        points: score(alerts, trips, positions, searches),
        alerts,
        trips,
        positions,
        searches,
    }))
}

/// DELETE /account — supprime le compte et TOUTES ses données (RGPD /
/// exigence App Store), d'un seul tenant : une panne au milieu ne laisse
/// plus un compte à moitié effacé. Ses sessions et liens de
/// réinitialisation partent avec lui. Les signalements supprimés sont
/// notifiés en direct, une fois la suppression acquise.
async fn supprime(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let uid = qui.user_id;
    let tx = req.db().begin().await?;
    let alertes: Vec<(i32,)> = tx
        .raw(
            "DELETE FROM alerts WHERE user_id = ?1 RETURNING id",
            &params![uid],
        )
        .await?;
    let positions = tx
        .raw_execute("DELETE FROM positions WHERE user_id = ?1", &params![uid])
        .await?;
    tx.raw_execute("DELETE FROM trips WHERE user_id = ?1", &params![uid])
        .await?;
    tx.raw_execute("DELETE FROM searches WHERE user_id = ?1", &params![uid])
        .await?;
    connexion::efface_traces(&tx, uid).await?;
    tx.raw_execute("DELETE FROM users WHERE id = ?1", &params![uid])
        .await?;
    tx.commit().await?;

    let hub = hub(&req);
    for (id,) in alertes {
        hub.diffuse(&Evenement::AlertGone { id: id as i64 });
    }
    if positions > 0 {
        hub.diffuse(&Evenement::PositionsChanged);
    }
    Ok(Response::new().status(204))
}

/// GET /account/export — la portabilité RGPD : tout ce que le compte
/// détient, en un seul JSON — positions, trajets, recherches,
/// signalements, préférences. Sans le hachage du mot de passe : un secret
/// dérivé n'appartient à personne, pas même à son propriétaire.
async fn export(req: Request) -> Resultat<Response> {
    let u = compte(&req).await?;
    let db = req.db();
    let positions = Position::query()
        .filter(Position::USER_ID.eq(u.id))
        .order_by(Position::ID.asc())
        .all(db)
        .await?;
    let trajets = Trajet::query()
        .filter(Trajet::USER_ID.eq(u.id))
        .order_by(Trajet::ID.asc())
        .all(db)
        .await?;
    let recherches = Recherche::query()
        .filter(Recherche::USER_ID.eq(u.id))
        .order_by(Recherche::ID.asc())
        .all(db)
        .await?;
    let signalements = Signalement::query()
        .filter(Signalement::USER_ID.eq(u.id))
        .order_by(Signalement::ID.asc())
        .all(db)
        .await?;
    Ok(Response::new().json(serde_json::json!({
        "compte": {
            "username": u.username,
            "created_at": u.created_at,
            "prefs": u
                .prefs
                .and_then(|p| serde_json::from_str::<Value>(&p).ok()),
        },
        "positions": positions,
        "trajets": trajets,
        "recherches": recherches,
        "signalements": signalements,
    })))
}

/// GET /prefs — les préférences du compte (Domicile, Travail…), telles que
/// l'app les a poussées. Un objet vide tant que rien n'a été poussé.
async fn lit_prefs(req: Request) -> Resultat<Response> {
    let u = compte(&req).await?;
    Ok(Response::new()
        .content_type("application/json")
        .send(u.prefs.unwrap_or_else(|| "{}".into())))
}

/// PUT /prefs — remplace les préférences du compte. Un objet JSON borné,
/// quel que soit le type annoncé : le serveur garde le texte tel quel, c'est
/// l'app qui sait le lire — les favoris appartiennent au compte, pas à
/// l'appareil.
async fn ecrit_prefs(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let corps = req.text().await?;
    if corps.len() > MAX_PREFS {
        return Err(Erreur::requete("préférences trop longues"));
    }
    if !matches!(serde_json::from_str::<Value>(&corps), Ok(Value::Object(_))) {
        return Err(Erreur::requete("préférences invalides"));
    }
    let n = req
        .db()
        .raw_execute(
            "UPDATE users SET prefs = ?1 WHERE id = ?2",
            &params![corps, qui.user_id],
        )
        .await?;
    if n == 0 {
        return Err(Erreur::NonAuthentifie);
    }
    Ok(Response::new().status(204))
}

/// Une ligne du classement des contributeurs.
#[derive(Serialize)]
struct LeaderEntry {
    name: String,
    points: i64,
}

/// GET /leaderboard — top 10 des contributeurs (noms pseudonymisés).
async fn classement(req: Request) -> Resultat<Response> {
    connexion::utilisateur(&req).await?;
    // Le score se calcule en SQL et seuls les dix premiers sortent de la
    // base (à égalité, les comptes les plus anciens d'abord) : plus besoin de
    // rapatrier tous les comptes pour en garder dix.
    let lignes: Vec<(String, i64, i64, i64, i64)> = req
        .db()
        .raw(
            "SELECT username, alerts, trips, positions, searches FROM (\
               SELECT u.id, u.username, \
                 (SELECT count(*) FROM alerts a WHERE a.user_id = u.id) AS alerts, \
                 (SELECT count(*) FROM trips t WHERE t.user_id = u.id) AS trips, \
                 (SELECT count(*) FROM positions p WHERE p.user_id = u.id) AS positions, \
                 (SELECT count(*) FROM searches s WHERE s.user_id = u.id) AS searches \
               FROM users u) \
             ORDER BY alerts * 10 + trips * 5 + positions * 2 + searches DESC, id \
             LIMIT 10",
            &[],
        )
        .await?;
    let entries: Vec<LeaderEntry> = lignes
        .iter()
        .map(
            |(username, alerts, trips, positions, searches)| LeaderEntry {
                name: public_name(username),
                points: score(
                    *alerts as u64,
                    *trips as u64,
                    *positions as u64,
                    *searches as u64,
                ),
            },
        )
        .collect();
    Ok(Response::new().json(entries))
}

/// POST /diagnostics — les rapports de plantage MetricKit de l'app.
///
/// Anonymes et bornés : pas d'identifiant, pas de position — la pile
/// d'appels que le système a relevée, journalisée telle quelle. `heroku
/// logs` devient le tableau de bord des plantages, sans service tiers.
/// Un plantage arrive volontiers avant la connexion : pas d'auth ici.
async fn diagnostics(req: Request) -> Resultat<Response> {
    let corps = req.text().await?;
    // Seize mille caractères suffisent à lire une pile ; le reste est du lest.
    let extrait: String = corps.chars().take(MAX_DIAGNOSTIC).collect();
    tracing::warn!(octets = corps.len(), "diagnostic app : {extrait}");
    Ok(Response::new().status(204))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liste_invites_pardonne_la_saisie() {
        assert!(est_invite("paul,marie", "paul"));
        assert!(est_invite(" Paul , marie ", "paul"));
        assert!(est_invite("paul", "PAUL"));
        assert!(!est_invite("paul,marie", "pau"));
        assert!(!est_invite("paulette", "paul"));
        assert!(!est_invite("", "paul"));
        assert!(!est_invite(",,", ""));
    }

    #[test]
    fn public_name_masks_email() {
        // Adresses fictives : ce fichier est destiné à être lu par d'autres,
        // et une vraie adresse dans un test finit indexée par les moissonneurs.
        assert_eq!(public_name("marceline@exemple.fr"), "marce…");
        assert_eq!(public_name("bob@x.fr"), "bob");
        assert_eq!(public_name("sanschien"), "sansc…");
    }

    #[test]
    fn score_weights_contributions() {
        // 1 signalement (10) + 1 trajet (5) + 2 positions (4) + 1 recherche (1).
        assert_eq!(score(1, 1, 2, 1), 20);
        assert_eq!(score(0, 0, 0, 0), 0);
    }
}

#[cfg(test)]
mod essais {
    use paquets::orm::{Executor, params};
    use serde_json::{Value, json};

    use crate::essais::{bearer, client, inscrit};

    #[tokio::test]
    async fn signup_then_authed_and_401_without_token() {
        let c = client().await;
        let (nom, jeton) = inscrit(&c, "acct-auth").await;
        let r = c
            .get("/account")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        let v: Value = r.json();
        assert_eq!(v["username"], nom.as_str());
        assert_eq!(v["plus_offert"], false);
        assert!(v["created_at"].as_i64().unwrap() > 0);
        let r = c.get("/account").await;
        assert_eq!(r.status(), 401);
        assert_eq!(r.text(), "authentification requise");
    }

    #[tokio::test]
    async fn account_info_counts_are_correct() {
        let c = client().await;
        let (_, jeton) = inscrit(&c, "acct").await;
        // Deux positions ajoutées, aucun trajet.
        for i in 0..2 {
            c.post("/positions")
                .header("authorization", &bearer(&jeton))
                .json(&json!({"lat": 48.0 + f64::from(i) * 0.1, "lon": 2.0, "label": "P"}))
                .await;
        }
        let r = c
            .get("/account")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        let info: Value = r.json();
        assert_eq!(info["positions"].as_u64(), Some(2));
        assert_eq!(info["trips"].as_u64(), Some(0));
        assert_eq!(info["alerts"].as_u64(), Some(0));
        assert_eq!(info["searches"].as_u64(), Some(0));
        assert_eq!(info["points"].as_i64(), Some(4));
    }

    #[tokio::test]
    async fn account_deletion_removes_data_and_frees_email() {
        let c = client().await;
        let (nom, jeton) = inscrit(&c, "del").await;
        // Un deuxième appareil, et un peu de tout rattaché au compte.
        let r = c
            .post("/auth/login")
            .json(&json!({"username": nom, "password": "secret1"}))
            .await;
        let deux = r.json::<Value>()["token"].as_str().unwrap().to_owned();
        c.post("/positions")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"lat": 1.0, "lon": 2.0, "label": "temp"}))
            .await;
        c.post("/trips")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"points": [{"lat": 1.0, "lon": 2.0}, {"lat": 1.1, "lon": 2.0}]}))
            .await;
        c.post("/searches")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"name": "Gare", "lat": 1.0, "lon": 2.0}))
            .await;
        let db = &c.db;
        let uid: Vec<(i32,)> = db
            .raw(
                "SELECT id FROM users WHERE username = ?1",
                &params![nom.clone()],
            )
            .await
            .unwrap();
        let uid = uid[0].0;
        db.raw_execute(
            "INSERT INTO alerts (category, label, lat, lon, created_at, expires_at, user_id) \
             VALUES ('police', '', 1.0, 2.0, ?2, ?3, ?1)",
            &params![
                uid,
                crate::base::maintenant(),
                crate::base::maintenant() + 600
            ],
        )
        .await
        .unwrap();
        db.raw_execute(
            "INSERT INTO reset_tokens (user_id, token_hash, expires_at, used) VALUES (?1, ?2, ?3, FALSE)",
            &params![
                uid,
                crate::connexion::empreinte(&crate::connexion::jeton_aleatoire()),
                crate::base::maintenant() + 3600
            ],
        )
        .await
        .unwrap();

        let r = c.delete("/account").await;
        assert_eq!(r.status(), 401);
        let r = c
            .delete("/account")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 204);

        // Plus rien du compte en base.
        let restes: Vec<(i64,)> = db
            .raw(
                "SELECT (SELECT count(*) FROM positions WHERE user_id = ?1) \
                 + (SELECT count(*) FROM trips WHERE user_id = ?1) \
                 + (SELECT count(*) FROM searches WHERE user_id = ?1) \
                 + (SELECT count(*) FROM alerts WHERE user_id = ?1) \
                 + (SELECT count(*) FROM sessions WHERE user_id = ?1) \
                 + (SELECT count(*) FROM reset_tokens WHERE user_id = ?1) \
                 + (SELECT count(*) FROM users WHERE id = ?1)",
                &params![uid],
            )
            .await
            .unwrap();
        assert_eq!(restes[0].0, 0);

        // Les anciens jetons, des deux appareils, ne valent plus rien.
        for j in [&jeton, &deux] {
            let r = c.get("/account").header("authorization", &bearer(j)).await;
            assert_eq!(r.status(), 401);
            let r = c
                .post("/positions")
                .header("authorization", &bearer(j))
                .json(&json!({"lat": 1.0, "lon": 2.0, "label": "orpheline"}))
                .await;
            assert_eq!(r.status(), 401);
        }

        // L'e-mail est de nouveau libre : ré-inscription possible.
        let r = c
            .post("/auth/signup")
            .json(&json!({"username": nom, "password": "s3cret!"}))
            .await;
        assert_eq!(r.status(), 200);
    }

    #[tokio::test]
    async fn leaderboard_returns_array() {
        let c = client().await;
        let (_, jeton) = inscrit(&c, "leader").await;
        c.post("/positions")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"lat": 1.0, "lon": 2.0, "label": "P"}))
            .await;
        let r = c
            .get("/leaderboard")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        let v: Value = r.json();
        let liste = v.as_array().unwrap();
        assert!(liste.len() <= 10);
        assert!(
            liste
                .windows(2)
                .all(|w| w[0]["points"].as_i64() >= w[1]["points"].as_i64())
        );
        let r = c.get("/leaderboard").await;
        assert_eq!(r.status(), 401);
    }

    #[tokio::test]
    async fn export_du_compte_complet_et_sans_secret() {
        let c = client().await;
        let (nom, jeton) = inscrit(&c, "export").await;
        c.post("/positions")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"lat": 48.0, "lon": 2.0, "label": "Chez Paul"}))
            .await;
        let r = c
            .get("/account/export")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        let corps: Value = r.json();
        assert_eq!(corps["compte"]["username"], nom.as_str());
        assert_eq!(corps["compte"]["prefs"], Value::Null);
        assert_eq!(corps["positions"].as_array().map(|p| p.len()), Some(1));
        assert_eq!(corps["trajets"], json!([]));
        assert_eq!(corps["recherches"], json!([]));
        assert_eq!(corps["signalements"], json!([]));
        // Le hachage du mot de passe ne voyage jamais, même chez son
        // propriétaire.
        assert!(!corps.to_string().contains("password"));
    }

    #[tokio::test]
    async fn prefs_aller_retour() {
        let c = client().await;
        let (_, jeton) = inscrit(&c, "prefs").await;

        // Vide au départ : un objet, pas une erreur.
        let r = c
            .get("/prefs")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.header("content-type"), Some("application/json"));
        assert_eq!(r.json::<Value>(), json!({}));

        // On pousse Domicile, on le relit tel quel (texte brut, sans type).
        let brut = r#"{"home": {"lat": 48.1, "lon": -1.6, "label": "Maison"}}"#;
        let r = c
            .put("/prefs")
            .header("authorization", &bearer(&jeton))
            .body(brut)
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .get("/prefs")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.text(), brut);

        // Un tableau n'est pas un objet de préférences.
        let r = c
            .put("/prefs")
            .header("authorization", &bearer(&jeton))
            .json(&json!([1, 2]))
            .await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.text(), "préférences invalides");
        let r = c
            .put("/prefs")
            .header("authorization", &bearer(&jeton))
            .body(format!(r#"{{"x": "{}"}}"#, "a".repeat(5000)))
            .await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.text(), "préférences trop longues");
    }

    #[tokio::test]
    async fn diagnostics_sans_compte() {
        let c = client().await;
        let r = c.post("/diagnostics").body("pile d'appels").await;
        assert_eq!(r.status(), 204);
    }
}
