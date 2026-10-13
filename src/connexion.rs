//! La connexion : comptes, sessions, mots de passe.
//!
//! # Ce qui la rend fiable
//!
//! - **Des sessions, plus des jetons à durée fixe.** À la connexion, le
//!   serveur remet un jeton opaque de 256 bits (`mc_…`) et n'en garde que
//!   l'empreinte SHA-256, dans la table `sessions`. La session vit tant qu'on
//!   s'en sert : chaque usage repousse son échéance de 180 jours. Plus de
//!   déconnexion surprise au bout d'un mois, ni au changement d'un secret.
//! - **Une vraie déconnexion.** Se déconnecter supprime la session côté
//!   serveur ; « déconnecter partout », changer ou réinitialiser son mot de
//!   passe, supprimer son compte coupent toutes les sessions — un jeton volé
//!   ne survit plus à rien de tout cela.
//! - **Un identifiant sans casse.** `Max@Mail.fr` et `max@mail.fr` sont le
//!   même compte : la majuscule que le clavier de l'iPhone ajoute au début
//!   ne fait plus échouer la connexion.
//! - **Des refus qui disent pourquoi**, en texte (l'app iOS affiche le corps
//!   tel quel) : identifiants incorrects, trop de tentatives et dans combien
//!   de temps réessayer, champs trop courts ou trop longs.
//! - **Un frein aux devinettes par compte**, en base (partagé entre dynos,
//!   survit aux redémarrages) plutôt qu'une limite par adresse IP, qui
//!   bloquait d'un coup tout un réseau mobile derrière la même IP.
//! - **Argon2 hors de la boucle d'événements** : un calcul de hachage ne
//!   gèle plus les autres requêtes.
//! - **La transition sans couture** : les jetons JWT déjà distribués restent
//!   acceptés jusqu'à leur échéance (si `JWT_SECRET` est défini), mais ils
//!   sont révocables eux aussi — via `users.jetons_revoques_avant`.

use std::sync::OnceLock;

use argon2::Argon2;
use argon2::password_hash::{
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    rand_core::{OsRng, RngCore},
};
use paquets::orm::{Database, Executor, Transaction, params};
use paquets::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::base::{Utilisateur, maintenant};
use crate::erreur::{Erreur, Resultat, corps_json, param};

/// Une session vit 180 jours après son dernier usage.
pub const DUREE_SESSION: i64 = 180 * 24 * 3600;
/// On n'écrit `last_seen_at` qu'une fois par heure et par session.
const RAFRAICHISSEMENT: i64 = 3600;
/// Préfixe des jetons de session (les distingue des anciens JWT).
const PREFIXE: &str = "mc_";
/// Durée de vie d'un ancien JWT : son `exp` moins cela donne sa date d'émission.
const DUREE_JWT: i64 = 30 * 24 * 3600;

/// Échecs tolérés sur un identifiant avant de le freiner…
const ECHECS_MAX: i32 = 10;
/// … dans cette fenêtre…
const FENETRE_ECHECS: i64 = 15 * 60;
/// … et pour ce temps.
const BLOCAGE: i64 = 15 * 60;

const NOM_MIN: usize = 3;
const NOM_MAX: usize = 254;
const MDP_MIN: usize = 6;
const MDP_MAX: usize = 128;

const REFUS_IDENTIFIANTS: &str = "identifiant ou mot de passe incorrect";

pub fn routes(app: &mut App) {
    app.post("/auth/signup", inscription);
    app.post("/auth/login", connexion);
    app.post("/auth/logout", deconnexion);
    app.post("/auth/logout-all", deconnexion_partout);
    app.get("/auth/me", moi);
    app.get("/auth/sessions", liste_sessions);
    app.delete("/auth/sessions/:id", revoque_session);
    app.post("/auth/password", change_mot_de_passe);
    app.post("/auth/forgot", mot_de_passe_oublie);
    app.post("/auth/reset", reinitialise);
    app.get("/reinitialisation", page_reinitialisation);
}

// ---------------------------------------------------------------------------
// Briques : jetons, empreintes, mots de passe.
// ---------------------------------------------------------------------------

fn hex(octets: &[u8]) -> String {
    octets.iter().map(|b| format!("{b:02x}")).collect()
}

/// Jeton aléatoire non devinable de 128 bits (32 hexadécimaux) : liens de
/// réinitialisation et de partage.
pub fn jeton_aleatoire() -> String {
    let mut octets = [0u8; 16];
    OsRng.fill_bytes(&mut octets);
    hex(&octets)
}

/// Un nouveau jeton de session : `mc_` et 256 bits en hexadécimal.
fn jeton_session() -> String {
    let mut octets = [0u8; 32];
    OsRng.fill_bytes(&mut octets);
    format!("{PREFIXE}{}", hex(&octets))
}

/// Empreinte SHA-256 hexadécimale : c'est elle qui est stockée, jamais le jeton.
pub fn empreinte(jeton: &str) -> String {
    hex(&Sha256::digest(jeton.as_bytes()))
}

fn hache_sync(mdp: &str) -> Result<String, String> {
    let sel = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(mdp.as_bytes(), &sel)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

fn verifie_sync(mdp: &str, empreinte: &str) -> bool {
    PasswordHash::new(empreinte)
        .map(|h| {
            Argon2::default()
                .verify_password(mdp.as_bytes(), &h)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Empreinte factice, pour qu'un identifiant inconnu coûte autant qu'un vrai :
/// le temps de réponse ne dit pas si le compte existe.
fn empreinte_factice() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| hache_sync("moncap-dummy-timing-guard").unwrap_or_default())
}

/// Hache un mot de passe (Argon2id), sur un fil à part.
pub async fn hache(mdp: &str) -> Resultat<String> {
    let mdp = mdp.to_owned();
    tokio::task::spawn_blocking(move || hache_sync(&mdp))
        .await
        .map_err(Erreur::interne)?
        .map_err(Erreur::interne)
}

/// Vérifie un mot de passe contre plusieurs empreintes candidates (l'ordre
/// compte) ; renvoie l'indice de la première qui correspond. Sans candidate,
/// vérifie quand même l'empreinte factice. Sur un fil à part.
async fn verifie_parmi(mdp: &str, candidates: Vec<String>) -> Option<usize> {
    let mdp = mdp.to_owned();
    tokio::task::spawn_blocking(move || {
        if candidates.is_empty() {
            let _ = verifie_sync(&mdp, empreinte_factice());
            return None;
        }
        candidates.iter().position(|h| verifie_sync(&mdp, h))
    })
    .await
    .ok()
    .flatten()
}

/// Vérifie un mot de passe, sur un fil à part.
pub async fn verifie(mdp: &str, empreinte: &str) -> bool {
    verifie_parmi(mdp, vec![empreinte.to_owned()]).await == Some(0)
}

/// La clé d'un identifiant : sans espaces autour, en minuscules.
pub fn cle_identifiant(nom: &str) -> String {
    nom.trim().to_lowercase()
}

/// Valide le couple identifiant / mot de passe d'une inscription.
fn valide_identifiants(nom: &str, mdp: &str) -> Resultat<()> {
    let n = nom.chars().count();
    let m = mdp.chars().count();
    if n < NOM_MIN || m < MDP_MIN {
        return Err(Erreur::requete("nom (≥3) et mot de passe (≥6) requis"));
    }
    if n > NOM_MAX {
        return Err(Erreur::requete("nom trop long (254 caractères au plus)"));
    }
    if m > MDP_MAX {
        return Err(Erreur::requete(
            "mot de passe trop long (128 caractères au plus)",
        ));
    }
    if nom.chars().any(char::is_control) {
        return Err(Erreur::requete("nom invalide"));
    }
    Ok(())
}

fn valide_nouveau_mdp(mdp: &str) -> Resultat<()> {
    let m = mdp.chars().count();
    if m < MDP_MIN {
        return Err(Erreur::requete(
            "mot de passe trop court (6 caractères au moins)",
        ));
    }
    if m > MDP_MAX {
        return Err(Erreur::requete(
            "mot de passe trop long (128 caractères au plus)",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// L'utilisateur d'une requête.
// ---------------------------------------------------------------------------

/// Qui fait la requête.
#[derive(Clone, Copy, Debug)]
pub struct Authentifie {
    pub user_id: i32,
    /// La session, ou `None` pour un ancien jeton JWT.
    pub session: Option<i64>,
}

/// Le jeton `Authorization: Bearer …` de la requête.
fn jeton_bearer(req: &Request) -> Option<&str> {
    let h = req.header("authorization")?;
    let (schema, jeton) = h.split_once(' ')?;
    schema
        .eq_ignore_ascii_case("bearer")
        .then(|| jeton.trim())
        .filter(|j| !j.is_empty())
}

/// L'utilisateur de la requête (en-tête `Authorization: Bearer`), ou 401.
pub async fn utilisateur(req: &Request) -> Resultat<Authentifie> {
    let jeton = jeton_bearer(req).ok_or(Erreur::NonAuthentifie)?;
    depuis_jeton(req.db(), jeton).await
}

/// L'utilisateur d'un jeton (en-tête, ou `?token=` pour le WebSocket et
/// l'export GPX), ou 401. Le compte doit encore exister.
pub async fn depuis_jeton(db: &Db, jeton: &str) -> Resultat<Authentifie> {
    let jeton = jeton.trim();
    if jeton.starts_with(PREFIXE) {
        return depuis_session(db, jeton).await;
    }
    depuis_jwt(db, jeton).await
}

async fn depuis_session(db: &Db, jeton: &str) -> Resultat<Authentifie> {
    let maintenant = maintenant();
    let lignes: Vec<(i64, i32, i64)> = db
        .raw(
            "SELECT s.id, s.user_id, s.last_seen_at FROM sessions s \
             JOIN users u ON u.id = s.user_id \
             WHERE s.token_hash = $1 AND s.expires_at > $2",
            &params![empreinte(jeton), maintenant],
        )
        .await?;
    let Some(&(id, user_id, vu)) = lignes.first() else {
        return Err(Erreur::NonAuthentifie);
    };
    if maintenant - vu >= RAFRAICHISSEMENT {
        // L'échéance glisse : une session dont on se sert ne meurt pas.
        db.raw_execute(
            "UPDATE sessions SET last_seen_at = $1, expires_at = $2 WHERE id = $3",
            &params![maintenant, maintenant + DUREE_SESSION, id],
        )
        .await?;
    }
    Ok(Authentifie {
        user_id,
        session: Some(id),
    })
}

/// Le secret des anciens jetons JWT. Sans `JWT_SECRET`, ils sont refusés :
/// le secret par défaut de l'ancien serveur était public, n'importe qui
/// pouvait forger un jeton avec.
pub(crate) fn secret_jwt() -> Option<&'static [u8]> {
    static SECRET: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    SECRET
        .get_or_init(|| {
            std::env::var("JWT_SECRET")
                .ok()
                .filter(|s| !s.is_empty() && s != "moncap-dev-secret-change-me")
                .map(String::into_bytes)
        })
        .as_deref()
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Claims {
    pub(crate) sub: i32,
    pub(crate) exp: i64,
}

/// L'identifiant et la date d'émission d'un ancien JWT valide.
fn lit_jwt(jeton: &str, secret: &[u8]) -> Option<(i32, i64)> {
    use jsonwebtoken::{DecodingKey, Validation, decode};
    let data = decode::<Claims>(
        jeton,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )
    .ok()?;
    Some((data.claims.sub, data.claims.exp - DUREE_JWT))
}

async fn depuis_jwt(db: &Db, jeton: &str) -> Resultat<Authentifie> {
    let secret = secret_jwt().ok_or(Erreur::NonAuthentifie)?;
    let (user_id, emis) = lit_jwt(jeton, secret).ok_or(Erreur::NonAuthentifie)?;
    let u = Utilisateur::find(db, user_id)
        .await?
        .ok_or(Erreur::NonAuthentifie)?;
    if emis < u.jetons_revoques_avant {
        return Err(Erreur::NonAuthentifie);
    }
    Ok(Authentifie {
        user_id,
        session: None,
    })
}

/// Coupe toutes les sessions d'un compte et rend caducs ses anciens JWT.
pub async fn coupe_tout<E: Executor>(db: &E, user_id: i32) -> Resultat<()> {
    db.raw_execute("DELETE FROM sessions WHERE user_id = $1", &params![user_id])
        .await?;
    db.raw_execute(
        "UPDATE users SET jetons_revoques_avant = $1 WHERE id = $2",
        &params![maintenant() + 1, user_id],
    )
    .await?;
    Ok(())
}

/// Ce que la connexion garde d'un compte, à effacer avec lui.
pub async fn efface_traces<E: Executor>(db: &E, user_id: i32) -> Resultat<()> {
    db.raw_execute("DELETE FROM sessions WHERE user_id = $1", &params![user_id])
        .await?;
    db.raw_execute(
        "DELETE FROM reset_tokens WHERE user_id = $1",
        &params![user_id],
    )
    .await?;
    Ok(())
}

/// Le ménage périodique : sessions mortes, liens échus, échecs oubliés.
pub async fn menage(db: &Db) -> Resultat<()> {
    let t = maintenant();
    db.raw_execute("DELETE FROM sessions WHERE expires_at <= $1", &params![t])
        .await?;
    db.raw_execute(
        "DELETE FROM reset_tokens WHERE expires_at <= $1",
        &params![t - 24 * 3600],
    )
    .await?;
    db.raw_execute(
        "DELETE FROM echecs_connexion WHERE depuis <= $1 AND bloque_jusqu_a <= $2",
        &params![t - FENETRE_ECHECS, t],
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Les routes.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Identifiants {
    username: String,
    password: String,
}

/// Réponse d'une connexion réussie. `token` et `username` sont ce que les
/// clients existants lisent ; le reste s'ajoute sans les gêner.
#[derive(Serialize)]
struct ReponseConnexion {
    token: String,
    username: String,
    expires_at: i64,
}

/// L'appareil d'une session, d'après son `User-Agent` (pour la liste des
/// sessions).
fn appareil(req: &Request) -> String {
    req.header("user-agent")
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(160)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Ouvre une session et la renvoie.
async fn ouvre_session<E: Executor>(
    db: &E,
    user_id: i32,
    username: String,
    device: String,
) -> Resultat<Response> {
    let jeton = jeton_session();
    let t = maintenant();
    let expires_at = t + DUREE_SESSION;
    db.raw_execute(
        "INSERT INTO sessions (user_id, token_hash, created_at, last_seen_at, expires_at, device) \
         VALUES ($1, $2, $3, $3, $4, $5)",
        &params![user_id, empreinte(&jeton), t, expires_at, device],
    )
    .await?;
    Ok(Response::new().json(ReponseConnexion {
        token: jeton,
        username,
        expires_at,
    }))
}

/// POST /auth/signup — crée un compte et ouvre une session.
async fn inscription(req: Request) -> Resultat<Response> {
    let c: Identifiants = corps_json(&req).await?;
    let username = c.username.trim().to_owned();
    valide_identifiants(&username, &c.password)?;
    let hash = hache(&c.password).await?;

    let db = req.db();
    let tx = db.begin().await?;
    // Deux inscriptions simultanées du même nom (à la casse près) passent
    // l'une après l'autre : le verrou tient jusqu'à la fin de la transaction.
    tx.raw_execute(
        "SELECT pg_advisory_xact_lock(hashtext(lower($1)))",
        &params![username.clone()],
    )
    .await?;
    let pris: Vec<(i32,)> = tx
        .raw(
            "SELECT id FROM users WHERE lower(username) = lower($1) LIMIT 1",
            &params![username.clone()],
        )
        .await?;
    if !pris.is_empty() {
        return Err(Erreur::requete("nom d'utilisateur déjà pris"));
    }
    let ids: Vec<(i32,)> = tx
        .raw(
            "INSERT INTO users (username, password_hash) VALUES ($1, $2) \
             ON CONFLICT (username) DO NOTHING RETURNING id",
            &params![username.clone(), hash],
        )
        .await?;
    let Some(&(id,)) = ids.first() else {
        return Err(Erreur::requete("nom d'utilisateur déjà pris"));
    };
    let res = ouvre_session(&tx, id, username, appareil(&req)).await?;
    tx.commit().await?;
    Ok(res)
}

/// Où en est un identifiant : bloqué (et pour combien de secondes) ?
async fn blocage(db: &Db, cle: &str) -> Resultat<Option<i64>> {
    let lignes: Vec<(i64,)> = db
        .raw(
            "SELECT bloque_jusqu_a FROM echecs_connexion WHERE cle = $1",
            &params![cle.to_owned()],
        )
        .await?;
    let t = maintenant();
    Ok(lignes
        .first()
        .map(|&(jusqu_a,)| jusqu_a - t)
        .filter(|&reste| reste > 0))
}

/// Compte un échec ; au dixième dans le quart d'heure, l'identifiant est
/// freiné un quart d'heure.
async fn note_echec(db: &Db, cle: &str) -> Resultat<()> {
    let t = maintenant();
    db.raw_execute(
        "INSERT INTO echecs_connexion AS e (cle, echecs, depuis, bloque_jusqu_a) \
         VALUES ($1, 1, $2, 0) \
         ON CONFLICT (cle) DO UPDATE SET \
           echecs = CASE WHEN e.depuis <= $2 - $3 THEN 1 ELSE e.echecs + 1 END, \
           depuis = CASE WHEN e.depuis <= $2 - $3 THEN $2 ELSE e.depuis END, \
           bloque_jusqu_a = CASE \
             WHEN e.depuis > $2 - $3 AND e.echecs + 1 >= $4 THEN $2 + $5 \
             ELSE e.bloque_jusqu_a END",
        &params![
            cle.to_owned(),
            t,
            FENETRE_ECHECS,
            ECHECS_MAX as i64,
            BLOCAGE
        ],
    )
    .await?;
    Ok(())
}

fn trop_de_tentatives(secondes: i64) -> Erreur {
    let minutes = (secondes + 59) / 60;
    Erreur::TropDeTentatives {
        message: format!("trop de tentatives pour ce compte, réessaie dans {minutes} min"),
        secondes: secondes.max(1) as u64,
    }
}

/// POST /auth/login — vérifie les identifiants et ouvre une session.
async fn connexion(req: Request) -> Resultat<Response> {
    let c: Identifiants = corps_json(&req).await?;
    let nom = c.username.trim();
    if nom.is_empty() || c.password.is_empty() {
        return Err(Erreur::requete("identifiant et mot de passe requis"));
    }
    if nom.chars().count() > NOM_MAX || c.password.chars().count() > MDP_MAX {
        return Err(Erreur::Refus(REFUS_IDENTIFIANTS.into()));
    }
    let db = req.db();
    let cle = cle_identifiant(nom);
    if let Some(reste) = blocage(db, &cle).await? {
        return Err(trop_de_tentatives(reste));
    }

    // Le compte au nom exact d'abord ; puis ceux qui ne diffèrent que par la
    // casse (d'anciens comptes peuvent n'être distincts que par elle).
    let comptes: Vec<(i32, String, String)> = db
        .raw(
            "SELECT id, username, password_hash FROM users \
             WHERE lower(username) = lower($1) \
             ORDER BY (username = $1) DESC, id LIMIT 5",
            &params![nom.to_owned()],
        )
        .await?;
    let empreintes = comptes.iter().map(|(_, _, h)| h.clone()).collect();
    match verifie_parmi(&c.password, empreintes).await {
        Some(i) => {
            let (id, username, _) = comptes[i].clone();
            db.raw_execute("DELETE FROM echecs_connexion WHERE cle = $1", &params![cle])
                .await?;
            ouvre_session(db, id, username, appareil(&req)).await
        }
        None => {
            note_echec(db, &cle).await?;
            if let Some(reste) = blocage(db, &cle).await? {
                return Err(trop_de_tentatives(reste));
            }
            Err(Erreur::Refus(REFUS_IDENTIFIANTS.into()))
        }
    }
}

/// POST /auth/logout — ferme la session de la requête.
async fn deconnexion(req: Request) -> Resultat<Response> {
    let qui = utilisateur(&req).await?;
    let db = req.db();
    match qui.session {
        Some(id) => {
            db.raw_execute("DELETE FROM sessions WHERE id = $1", &params![id])
                .await?;
        }
        None => {
            // Un ancien JWT ne se révoque pas seul : on les révoque tous.
            db.raw_execute(
                "UPDATE users SET jetons_revoques_avant = $1 WHERE id = $2",
                &params![maintenant() + 1, qui.user_id],
            )
            .await?;
        }
    }
    Ok(Response::new().status(204))
}

/// POST /auth/logout-all — ferme toutes les sessions du compte.
async fn deconnexion_partout(req: Request) -> Resultat<Response> {
    let qui = utilisateur(&req).await?;
    coupe_tout(req.db(), qui.user_id).await?;
    Ok(Response::new().status(204))
}

#[derive(Serialize)]
struct Moi {
    username: String,
    created_at: i64,
}

/// GET /auth/me — la session est-elle encore bonne ? et pour qui ?
async fn moi(req: Request) -> Resultat<Response> {
    let qui = utilisateur(&req).await?;
    let u = Utilisateur::find(req.db(), qui.user_id)
        .await?
        .ok_or(Erreur::NonAuthentifie)?;
    Ok(Response::new().json(Moi {
        username: u.username,
        created_at: u.created_at,
    }))
}

#[derive(Serialize)]
struct SessionVue {
    id: i64,
    device: String,
    created_at: i64,
    last_seen_at: i64,
    current: bool,
}

/// GET /auth/sessions — les appareils connectés au compte.
async fn liste_sessions(req: Request) -> Resultat<Response> {
    let qui = utilisateur(&req).await?;
    let lignes: Vec<(i64, String, i64, i64)> = req
        .db()
        .raw(
            "SELECT id, device, created_at, last_seen_at FROM sessions \
             WHERE user_id = $1 AND expires_at > $2 ORDER BY last_seen_at DESC, id DESC",
            &params![qui.user_id, maintenant()],
        )
        .await?;
    let vues: Vec<SessionVue> = lignes
        .into_iter()
        .map(|(id, device, created_at, last_seen_at)| SessionVue {
            id,
            device,
            created_at,
            last_seen_at,
            current: qui.session == Some(id),
        })
        .collect();
    Ok(Response::new().json(vues))
}

/// DELETE /auth/sessions/:id — déconnecte un appareil.
async fn revoque_session(req: Request) -> Resultat<Response> {
    let qui = utilisateur(&req).await?;
    let id: i64 = param(&req, "id")?;
    let n = req
        .db()
        .raw_execute(
            "DELETE FROM sessions WHERE id = $1 AND user_id = $2",
            &params![id, qui.user_id],
        )
        .await?;
    if n == 0 {
        return Err(Erreur::introuvable("session introuvable"));
    }
    Ok(Response::new().status(204))
}

#[derive(Deserialize)]
struct ChangeMdp {
    current_password: String,
    new_password: String,
}

/// POST /auth/password — change le mot de passe (l'actuel est exigé), coupe
/// toutes les sessions et en ouvre une neuve pour l'appareil qui l'a fait.
async fn change_mot_de_passe(req: Request) -> Resultat<Response> {
    let qui = utilisateur(&req).await?;
    let c: ChangeMdp = corps_json(&req).await?;
    valide_nouveau_mdp(&c.new_password)?;
    let db = req.db();
    let u = Utilisateur::find(db, qui.user_id)
        .await?
        .ok_or(Erreur::NonAuthentifie)?;
    if !verifie(&c.current_password, &u.password_hash).await {
        return Err(Erreur::Refus("mot de passe actuel incorrect".into()));
    }
    let hash = hache(&c.new_password).await?;
    let tx = db.begin().await?;
    tx.raw_execute(
        "UPDATE users SET password_hash = $1 WHERE id = $2",
        &params![hash, u.id],
    )
    .await?;
    coupe_tout(&tx, u.id).await?;
    let res = ouvre_session(&tx, u.id, u.username, appareil(&req)).await?;
    tx.commit().await?;
    Ok(res)
}

// ---------------------------------------------------------------------------
// Mot de passe oublié.
// ---------------------------------------------------------------------------

/// L'adresse publique du service, pour les liens envoyés par courriel.
fn url_publique() -> String {
    std::env::var("PUBLIC_URL")
        .map(|u| u.trim().trim_end_matches('/').to_owned())
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "https://moncap-c41a5aaf07e8.herokuapp.com".into())
}

/// Le SMTP est-il configuré ? SMTP_HOST, SMTP_USER et SMTP_PASS suffisent
/// (l'expéditeur SMTP_FROM retombe sur SMTP_USER, le port SMTP_PORT sur 587).
pub fn smtp_configure() -> bool {
    ["SMTP_HOST", "SMTP_USER", "SMTP_PASS"]
        .iter()
        .all(|v| std::env::var(v).map(|s| !s.is_empty()).unwrap_or(false))
}

/// Envoie un courriel en texte brut via le SMTP configuré.
async fn envoie_courriel(dest: &str, sujet: &str, corps: String) -> Result<(), String> {
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    let hote = std::env::var("SMTP_HOST").map_err(|_| "SMTP_HOST absent".to_string())?;
    let utilisateur = std::env::var("SMTP_USER").map_err(|_| "SMTP_USER absent".to_string())?;
    let secret = std::env::var("SMTP_PASS").map_err(|_| "SMTP_PASS absent".to_string())?;
    let de = std::env::var("SMTP_FROM").unwrap_or_else(|_| utilisateur.clone());
    let port: u16 = std::env::var("SMTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(587);

    let message = Message::builder()
        .from(
            de.parse()
                .map_err(|e| format!("expéditeur invalide : {e}"))?,
        )
        .to(dest
            .parse()
            .map_err(|e| format!("destinataire invalide : {e}"))?)
        .subject(sujet)
        .header(ContentType::TEXT_PLAIN)
        .body(corps)
        .map_err(|e| format!("construction du courriel : {e}"))?;

    let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&hote)
        .map_err(|e| format!("SMTP : {e}"))?
        .port(port)
        .credentials(Credentials::new(utilisateur, secret))
        .timeout(Some(std::time::Duration::from_secs(20)))
        .build();
    transport
        .send(message)
        .await
        .map(|_| ())
        .map_err(|e| format!("envoi : {e}"))
}

#[derive(Deserialize)]
struct Oubli {
    username: String,
}

/// Trois liens par heure et par compte, pas plus.
const LIENS_PAR_HEURE: i64 = 3;

/// POST /auth/forgot — envoie un lien de réinitialisation si un compte
/// existe pour cette adresse.
///
/// Répond 204 dans tous les cas, et vite : le courriel part en arrière-plan,
/// si bien que ni le statut ni le temps de réponse ne disent si l'adresse a
/// un compte. Le seul autre statut, 503, part avant toute recherche : l'envoi
/// n'est simplement pas configuré.
async fn mot_de_passe_oublie(req: Request) -> Resultat<Response> {
    if !smtp_configure() {
        return Ok(Response::new().status(503));
    }
    let o: Oubli = corps_json(&req).await?;
    let nom = o.username.trim();
    if nom.is_empty() || nom.chars().count() > NOM_MAX {
        return Ok(Response::new().status(204));
    }
    let db = req.db();
    let comptes: Vec<(i32, String)> = db
        .raw(
            "SELECT id, username FROM users WHERE lower(username) = lower($1) \
             ORDER BY (username = $1) DESC, id LIMIT 1",
            &params![nom.to_owned()],
        )
        .await?;
    if let Some((user_id, username)) = comptes.into_iter().next() {
        let t = maintenant();
        // Les liens vivent une heure : ceux encore vivants ont tous moins d'une heure.
        let recents: Vec<(i64,)> = db
            .raw(
                "SELECT count(*) FROM reset_tokens WHERE user_id = $1 AND expires_at > $2",
                &params![user_id, t],
            )
            .await?;
        if recents.first().map_or(0, |r| r.0) < LIENS_PAR_HEURE {
            let jeton = jeton_aleatoire();
            let tx = db.begin().await?;
            // Un nouveau lien rend les précédents caducs.
            tx.raw_execute(
                "UPDATE reset_tokens SET used = TRUE WHERE user_id = $1 AND used = FALSE",
                &params![user_id],
            )
            .await?;
            tx.raw_execute(
                "INSERT INTO reset_tokens (user_id, token_hash, expires_at, used) \
                 VALUES ($1, $2, $3, FALSE)",
                &params![user_id, empreinte(&jeton), t + 3600],
            )
            .await?;
            tx.commit().await?;
            let lien = format!("{}/reinitialisation?jeton={jeton}", url_publique());
            let corps = format!(
                "Bonjour,\n\n\
                 Quelqu'un — sans doute toi — a demandé à réinitialiser le mot de \
                 passe MonCap GPS du compte {username}.\n\n\
                 Ouvre ce lien pour en choisir un nouveau (valable une heure, une \
                 seule fois) :\n{lien}\n\n\
                 Si tu n'as rien demandé, ignore ce message : rien ne changera.\n\n\
                 — MonCap GPS"
            );
            tokio::spawn(async move {
                if let Err(e) = envoie_courriel(
                    &username,
                    "Réinitialisation du mot de passe MonCap GPS",
                    corps,
                )
                .await
                {
                    tracing::warn!("courriel de réinitialisation non parti : {e}");
                }
            });
        }
    }
    Ok(Response::new().status(204))
}

/// Un jeton de lien vaut 32 hexadécimaux minuscules : tout le reste est
/// rejeté avant d'être regardé — ou réfléchi dans du HTML.
pub fn jeton_valide(j: &str) -> bool {
    j.len() == 32 && j.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Le squelette HTML des pages de réinitialisation.
fn page_reinit(statut: u16, corps: &str) -> Response {
    let html = format!(
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"robots\" content=\"noindex\">\
         <title>Réinitialisation — MonCap GPS</title><style>\
         body{{font-family:-apple-system,system-ui,sans-serif;background:#0e2a5e;\
         color:#16213a;display:flex;min-height:100vh;align-items:center;\
         justify-content:center;margin:0;padding:16px}}\
         .carte{{background:#fff;border-radius:16px;padding:28px;max-width:380px;\
         width:100%;box-shadow:0 8px 30px rgb(0 0 0/.25)}}\
         h1{{font-size:1.2rem;margin:0 0 10px}}p{{margin:0 0 14px;line-height:1.5}}\
         input{{width:100%;box-sizing:border-box;padding:12px;font-size:1rem;\
         border:1px solid #dfe5f0;border-radius:10px;margin-bottom:12px}}\
         button{{width:100%;padding:13px;font-size:1rem;font-weight:700;\
         color:#fff;background:#1a57e6;border:0;border-radius:999px}}\
         .doux{{color:#55617a;font-size:.85rem}}\
         </style></head><body><div class=\"carte\">{corps}</div></body></html>"
    );
    Response::new()
        .status(statut)
        .header(
            "content-security-policy",
            "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; \
             frame-ancestors 'none'; base-uri 'none'",
        )
        .header("cache-control", "no-store")
        .header("referrer-policy", "no-referrer")
        .html(html)
}

const LIEN_MORT: &str = "<h1>Lien expiré</h1><p>Ce lien de réinitialisation n'est plus \
     valable — il a servi, ou son heure est passée.</p>\
     <p class=\"doux\">Redemande un lien depuis l'écran de \
     connexion de l'application (« Mot de passe oublié ? »).</p>";

/// Le lien est-il encore vivant ?
async fn lien_vivant(db: &Db, jeton: &str) -> Resultat<bool> {
    let lignes: Vec<(i32,)> = db
        .raw(
            "SELECT id FROM reset_tokens WHERE token_hash = $1 AND used = FALSE AND expires_at > $2",
            &params![empreinte(jeton), maintenant()],
        )
        .await?;
    Ok(!lignes.is_empty())
}

/// GET /reinitialisation?jeton=… — la page du lien reçu par courriel.
async fn page_reinitialisation(req: Request) -> Resultat<Response> {
    let jeton = req
        .query("jeton")
        .map(|j| j.into_owned())
        .unwrap_or_default();
    if !jeton_valide(&jeton) || !lien_vivant(req.db(), &jeton).await? {
        return Ok(page_reinit(200, LIEN_MORT));
    }
    Ok(page_reinit(
        200,
        &format!(
            "<h1>Nouveau mot de passe</h1>\
             <p class=\"doux\">Pour ton compte MonCap GPS. Six caractères minimum.</p>\
             <form method=\"post\" action=\"/auth/reset\">\
             <input type=\"hidden\" name=\"jeton\" value=\"{jeton}\">\
             <input type=\"password\" name=\"mot_de_passe\" minlength=\"6\" maxlength=\"128\" \
             required autocomplete=\"new-password\" placeholder=\"Nouveau mot de passe\">\
             <button>Changer le mot de passe</button></form>"
        ),
    ))
}

#[derive(Deserialize, Default)]
struct FormReinit {
    #[serde(default)]
    jeton: String,
    #[serde(default)]
    mot_de_passe: String,
}

/// POST /auth/reset — le formulaire de la page : brûle le jeton (une seule
/// fois, même sous deux envois simultanés), change le mot de passe et coupe
/// toutes les sessions du compte. Tout ou rien, dans une transaction.
async fn reinitialise(req: Request) -> Resultat<Response> {
    let f: FormReinit = req.form().await.unwrap_or_default();
    let mort = "<h1>Lien expiré</h1><p>Ce lien n'est plus valable — il a servi, ou \
                son heure est passée. Redemande-en un depuis l'application.</p>";
    if !jeton_valide(&f.jeton) {
        return Ok(page_reinit(400, mort));
    }
    let n = f.mot_de_passe.chars().count();
    if n < MDP_MIN {
        return Ok(page_reinit(
            400,
            "<h1>Mot de passe trop court</h1><p>Six caractères minimum — \
             reviens en arrière et réessaie.</p>",
        ));
    }
    if n > MDP_MAX {
        return Ok(page_reinit(
            400,
            "<h1>Mot de passe trop long</h1><p>128 caractères au plus — \
             reviens en arrière et réessaie.</p>",
        ));
    }
    let hash = hache(&f.mot_de_passe).await?;
    let db = req.db();
    let tx = db.begin().await?;
    let brules: Vec<(i32,)> = tx
        .raw(
            "UPDATE reset_tokens SET used = TRUE \
             WHERE token_hash = $1 AND used = FALSE AND expires_at > $2 RETURNING user_id",
            &params![empreinte(&f.jeton), maintenant()],
        )
        .await?;
    let Some(&(user_id,)) = brules.first() else {
        return Ok(page_reinit(400, mort));
    };
    let n = tx
        .raw_execute(
            "UPDATE users SET password_hash = $1 WHERE id = $2",
            &params![hash, user_id],
        )
        .await?;
    if n == 0 {
        return Ok(page_reinit(400, mort));
    }
    coupe_tout(&tx, user_id).await?;
    tx.raw_execute(
        "DELETE FROM echecs_connexion WHERE cle = (SELECT lower(username) FROM users WHERE id = $1)",
        &params![user_id],
    )
    .await?;
    tx.commit().await?;
    Ok(page_reinit(
        200,
        "<h1>C'est fait ✅</h1><p>Ton mot de passe est changé, et tous tes \
         appareils ont été déconnectés. Retourne dans l'application et \
         connecte-toi avec le nouveau.</p>",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jeton_de_reinitialisation_strict() {
        assert!(jeton_valide(&jeton_aleatoire()));
        for faux in [
            "",
            "abc",
            &"A".repeat(32),
            "<script>alert(1)</script>xxxxxx",
            &"a".repeat(33),
        ] {
            assert!(!jeton_valide(faux), "{faux}");
        }
    }

    #[test]
    fn empreinte_de_jeton_stable_et_distincte() {
        let a = empreinte("abc");
        assert_eq!(a, empreinte("abc"));
        assert_ne!(a, empreinte("abd"));
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn les_jetons_de_session_sont_longs_et_uniques() {
        let a = jeton_session();
        assert!(a.starts_with("mc_"));
        assert_eq!(a.len(), 3 + 64);
        assert_ne!(a, jeton_session());
    }

    #[test]
    fn password_hash_round_trip() {
        let h = hache_sync("s3cret!").unwrap();
        assert!(verifie_sync("s3cret!", &h));
        assert!(!verifie_sync("mauvais", &h));
        assert!(!verifie_sync("x", "pas-un-hash"));
    }

    #[test]
    fn les_anciens_jwt_se_lisent_encore() {
        use jsonwebtoken::{EncodingKey, Header, encode};
        let secret = b"un-secret";
        let exp = maintenant() + DUREE_JWT;
        let jwt = encode(
            &Header::default(),
            &Claims { sub: 42, exp },
            &EncodingKey::from_secret(secret),
        )
        .unwrap();
        let (sub, emis) = lit_jwt(&jwt, secret).unwrap();
        assert_eq!(sub, 42);
        assert!((emis - maintenant()).abs() <= 1);
        assert!(lit_jwt(&jwt, b"autre").is_none());
        assert!(lit_jwt("pas-un-jeton", secret).is_none());
        let perime = encode(
            &Header::default(),
            &Claims { sub: 7, exp: 1 },
            &EncodingKey::from_secret(secret),
        )
        .unwrap();
        assert!(lit_jwt(&perime, secret).is_none());
    }

    #[test]
    fn identifiants_valides() {
        assert!(valide_identifiants("max", "secret").is_ok());
        assert!(valide_identifiants("ma", "secret").is_err());
        assert!(valide_identifiants("max", "court").is_err());
        // Six caractères, même accentués (l'ancien serveur comptait les octets).
        assert!(valide_identifiants("max", "éééééé").is_ok());
        assert!(valide_identifiants(&"m".repeat(255), "secret").is_err());
        assert!(valide_identifiants("max", &"p".repeat(129)).is_err());
        assert!(valide_identifiants("ma\nx", "secret").is_err());
    }

    #[test]
    fn la_cle_ignore_casse_et_espaces() {
        assert_eq!(cle_identifiant("  Max@Mail.FR "), "max@mail.fr");
    }
}
