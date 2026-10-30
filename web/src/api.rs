//! Le client de l'API MonCap — les routes du serveur.
//!
//! Les erreurs y deviennent des phrases : le serveur répond en français
//! (`text/plain`), et ce qu'il dit est montré tel quel. Ce qui ne vient pas
//! de lui — un réseau coupé, une réponse illisible — reçoit un message à
//! nous, jamais le texte brut d'une erreur de décodage.

use gloo_net::http::{Request, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::session::{self, Session};
use crate::types::{
    Alerte, Appareil, Coord, Itineraire, Jeton, Moi, NouvellePosition, Position, Stats,
};

/// Ce que le serveur répond, au mot près, quand le jeton manque, a expiré
/// ou a été révoqué.
pub const AUTHENTIFICATION_REQUISE: &str = "authentification requise";

/// Ce qui peut mal tourner en parlant au serveur.
#[derive(Clone, Debug, PartialEq)]
pub enum Erreur {
    /// Jeton absent, expiré ou révoqué : la session doit se refermer.
    NonAutorise,
    /// Le serveur a répondu, mais pas ce qu'on attendait.
    Statut(u16, String),
    /// Le serveur n'a pas répondu du tout.
    Reseau,
    /// Le serveur a répondu 2xx, mais dans une forme inconnue — un serveur
    /// trop ancien, qui renvoie la coquille HTML pour une route qu'il ne
    /// connaît pas.
    Illisible,
}

impl Erreur {
    /// Le message à montrer, en français.
    pub fn message(&self) -> String {
        match self {
            Erreur::NonAutorise => SESSION_EXPIREE.into(),
            Erreur::Statut(code, texte) => message_statut(*code, texte, None),
            Erreur::Reseau => injoignable(),
            Erreur::Illisible => ILLISIBLE.into(),
        }
    }
}

/// Le message de la porte quand une session se referme d'elle-même.
pub const SESSION_EXPIREE: &str = "Session expirée : reconnecte-toi.";
const ILLISIBLE: &str = "Réponse inattendue du serveur — réessaie plus tard.";

fn injoignable() -> String {
    format!(
        "Serveur injoignable ({}) — vérifie ta connexion, puis réessaie.",
        nom_du_serveur()
    )
}

fn nom_du_serveur() -> String {
    let base = session::base_api();
    if base.is_empty() {
        "même serveur".into()
    } else {
        base
    }
}

/// Le texte d'une réponse d'erreur, s'il peut se montrer : sans espaces
/// autour, et jamais une page HTML — celle d'un proxy ou d'un serveur trop
/// ancien, qui n'a rien à faire dans un bandeau.
fn texte_montrable(corps: &str) -> Option<String> {
    let t = corps.trim();
    (!t.is_empty() && !t.starts_with('<')).then(|| t.to_string())
}

/// Une attente lisible : des secondes sous la minute, des minutes au-delà.
pub fn duree_attente(secondes: u32) -> String {
    if secondes < 60 {
        format!("{secondes} s")
    } else {
        format!("{} min", secondes.div_ceil(60))
    }
}

/// L'en-tête `Retry-After`, en secondes. La forme date, permise par HTTP,
/// n'est pas celle du serveur : ignorée.
pub fn secondes_retry_after(valeur: &str) -> Option<u32> {
    valeur.trim().parse().ok()
}

/// Le message d'une réponse en erreur : celui du serveur s'il en a donné
/// un, le nôtre sinon.
pub fn message_statut(code: u16, corps: &str, attente_s: Option<u32>) -> String {
    if let Some(t) = texte_montrable(corps) {
        return t;
    }
    match code {
        401 => "Identifiant ou mot de passe incorrect.".into(),
        429 => match attente_s {
            Some(s) => format!("Trop de tentatives — réessaie dans {}.", duree_attente(s)),
            None => "Trop de tentatives — réessaie dans une minute.".into(),
        },
        503 => "Service momentanément indisponible.".into(),
        code => format!("Le serveur a répondu {code}."),
    }
}

/// Un échec de la porte : le message, et l'attente imposée par le serveur
/// quand il en impose une (429).
#[derive(Clone, Debug, PartialEq)]
pub struct Echec {
    pub message: String,
    pub attente_s: Option<u32>,
}

impl From<Erreur> for Echec {
    fn from(e: Erreur) -> Self {
        Echec {
            message: e.message(),
            attente_s: None,
        }
    }
}

fn url(chemin: &str) -> String {
    format!("{}{chemin}", session::base_api())
}

/// L'équivalent de `encodeURIComponent` : tout sauf les caractères non
/// réservés est encodé, octet par octet.
pub fn encode_composant(s: &str) -> String {
    let mut sortie = String::with_capacity(s.len());
    for o in s.bytes() {
        let c = o as char;
        if c.is_ascii_alphanumeric() || "-_.!~*'()".contains(c) {
            sortie.push(c);
        } else {
            sortie.push_str(&format!("%{o:02X}"));
        }
    }
    sortie
}

/// L'adresse du WebSocket. Le jeton passe dans l'adresse : un WebSocket de
/// navigateur ne porte pas d'en-tête.
pub fn url_ws(base: &str, protocole: &str, hote: &str, jeton: &str) -> String {
    let jeton = encode_composant(jeton);
    if base.is_empty() {
        let p = if protocole == "https:" { "wss" } else { "ws" };
        format!("{p}://{hote}/ws?token={jeton}")
    } else {
        // `http` → `ws`, `https` → `wss` : seul le début change.
        let base = base
            .strip_prefix("http")
            .map(|reste| format!("ws{reste}"))
            .unwrap_or_else(|| base.to_string());
        format!("{base}/ws?token={jeton}")
    }
}

/// L'adresse de l'export GPX : un lien, donc le jeton en paramètre.
pub fn url_gpx(jeton: &str) -> String {
    url(&format!("/positions.gpx?token={}", encode_composant(jeton)))
}

fn avec_jeton(r: RequestBuilder, jeton: &str) -> RequestBuilder {
    r.header("authorization", &format!("Bearer {jeton}"))
}

async fn lis<T: DeserializeOwned>(r: Response) -> Result<T, Erreur> {
    match r.status() {
        401 => Err(Erreur::NonAutorise),
        s if !(200..300).contains(&s) => Err(Erreur::Statut(s, r.text().await.unwrap_or_default())),
        _ => r.json::<T>().await.map_err(|_| Erreur::Illisible),
    }
}

async fn vide(r: Response) -> Result<(), Erreur> {
    match r.status() {
        401 => Err(Erreur::NonAutorise),
        s if !(200..300).contains(&s) => Err(Erreur::Statut(s, r.text().await.unwrap_or_default())),
        _ => Ok(()),
    }
}

async fn envoie(r: Result<Request, gloo_net::Error>) -> Result<Response, Erreur> {
    r.map_err(|_| Erreur::Reseau)?
        .send()
        .await
        .map_err(|_| Erreur::Reseau)
}

fn json_vers<B: Serialize>(r: RequestBuilder, corps: &B) -> Result<Request, gloo_net::Error> {
    r.json(corps)
}

/// L'échec d'une requête de la porte, avec le `Retry-After` d'un 429.
async fn echec_de(r: Response) -> Echec {
    let code = r.status();
    let attente = (code == 429)
        .then(|| r.headers().get("retry-after"))
        .flatten()
        .and_then(|v| secondes_retry_after(&v));
    let corps = r.text().await.unwrap_or_default();
    Echec {
        message: message_statut(code, &corps, attente),
        attente_s: if code == 429 {
            attente.or(Some(60))
        } else {
            None
        },
    }
}

/// Une session neuve, lue dans la réponse du serveur.
async fn session_de(r: Response, tape: &str) -> Result<Session, Echec> {
    let j: Jeton = r.json().await.map_err(|_| Echec::from(Erreur::Illisible))?;
    if j.token.trim().is_empty() {
        return Err(Erreur::Illisible.into());
    }
    Ok(Session {
        jeton: j.token,
        utilisateur: if j.username.is_empty() {
            tape.to_string()
        } else {
            j.username
        },
    })
}

// --- Authentification -------------------------------------------------------

#[derive(Serialize)]
struct Identifiants<'a> {
    username: &'a str,
    password: &'a str,
}

/// L'identifiant tel qu'il part au serveur : sans espaces autour. La casse
/// est laissée telle quelle — le serveur compare sans elle.
pub fn identifiant(tape: &str) -> String {
    tape.trim().to_string()
}

/// Connexion ou inscription : la même requête, deux adresses.
pub async fn authentifie(
    inscription: bool,
    utilisateur: &str,
    mot_de_passe: &str,
) -> Result<Session, Echec> {
    let chemin = if inscription {
        "/auth/signup"
    } else {
        "/auth/login"
    };
    let utilisateur = identifiant(utilisateur);
    let corps = Identifiants {
        username: &utilisateur,
        password: mot_de_passe,
    };
    let r = envoie(json_vers(Request::post(&url(chemin)), &corps)).await?;
    if (200..300).contains(&r.status()) {
        session_de(r, &utilisateur).await
    } else {
        // À la porte, 401 veut dire « mauvais identifiants », pas « session
        // expirée » : le message du serveur le dit.
        Err(echec_de(r).await)
    }
}

/// Demande un lien de réinitialisation. Le serveur répond 204 que le compte
/// existe ou non — la réponse ne dit jamais si une adresse est inscrite —
/// et 503 quand l'envoi de courriels n'est pas configuré.
pub async fn mot_de_passe_oublie(utilisateur: &str) -> Result<(), Echec> {
    #[derive(Serialize)]
    struct Demande<'a> {
        username: &'a str,
    }
    let utilisateur = identifiant(utilisateur);
    let r = envoie(json_vers(
        Request::post(&url("/auth/forgot")),
        &Demande {
            username: &utilisateur,
        },
    ))
    .await?;
    match r.status() {
        s if (200..300).contains(&s) => Ok(()),
        503 => Err(Echec {
            message: "La réinitialisation n'est pas encore configurée.".into(),
            attente_s: None,
        }),
        _ => Err(echec_de(r).await),
    }
}

/// Vérifie qu'une session est toujours valable.
pub async fn moi(jeton: &str) -> Result<Moi, Erreur> {
    obtiens("/auth/me", jeton).await
}

/// Referme la session de cet appareil, côté serveur.
pub async fn deconnexion(jeton: &str) -> Result<(), Erreur> {
    vide(envoie(avec_jeton(Request::post(&url("/auth/logout")), jeton).build()).await?).await
}

/// Referme toutes les sessions du compte, celle-ci comprise.
pub async fn deconnexion_partout(jeton: &str) -> Result<(), Erreur> {
    vide(envoie(avec_jeton(Request::post(&url("/auth/logout-all")), jeton).build()).await?).await
}

/// Les appareils où le compte est ouvert.
pub async fn appareils(jeton: &str) -> Result<Vec<Appareil>, Erreur> {
    obtiens("/auth/sessions", jeton).await
}

/// Referme la session d'un autre appareil.
pub async fn revoque(jeton: &str, id: i64) -> Result<(), Erreur> {
    let chemin = format!("/auth/sessions/{id}");
    vide(envoie(avec_jeton(Request::delete(&url(&chemin)), jeton).build()).await?).await
}

/// Change le mot de passe. Le serveur rend une session neuve — l'ancien
/// jeton et ceux des autres appareils ne valent plus rien.
///
/// Ici, 401 a deux sens : « mot de passe actuel incorrect », qui se montre,
/// et « authentification requise », qui referme la session.
pub async fn change_mot_de_passe(
    jeton: &str,
    utilisateur: &str,
    actuel: &str,
    nouveau: &str,
) -> Result<Session, Erreur> {
    #[derive(Serialize)]
    struct Demande<'a> {
        current_password: &'a str,
        new_password: &'a str,
    }
    let r = envoie(json_vers(
        avec_jeton(Request::post(&url("/auth/password")), jeton),
        &Demande {
            current_password: actuel,
            new_password: nouveau,
        },
    ))
    .await?;
    match r.status() {
        401 => {
            let corps = r.text().await.unwrap_or_default();
            Err(erreur_401_mot_de_passe(&corps))
        }
        s if !(200..300).contains(&s) => Err(Erreur::Statut(s, r.text().await.unwrap_or_default())),
        _ => session_de(r, utilisateur)
            .await
            .map_err(|_| Erreur::Illisible),
    }
}

/// Le 401 de `/auth/password` : un mot de passe actuel refusé reste dans le
/// formulaire, une session morte referme la porte.
pub fn erreur_401_mot_de_passe(corps: &str) -> Erreur {
    let t = corps.trim();
    if t.is_empty() || t.eq_ignore_ascii_case(AUTHENTIFICATION_REQUISE) || t.starts_with('<') {
        Erreur::NonAutorise
    } else {
        Erreur::Statut(401, t.to_string())
    }
}

// --- Positions, itinéraires ----------------------------------------------------

async fn obtiens<T: DeserializeOwned>(chemin: &str, jeton: &str) -> Result<T, Erreur> {
    lis(envoie(avec_jeton(Request::get(&url(chemin)), jeton).build()).await?).await
}

pub async fn positions(jeton: &str) -> Result<Vec<Position>, Erreur> {
    obtiens("/positions", jeton).await
}

pub async fn stats(jeton: &str) -> Result<Stats, Erreur> {
    obtiens("/stats", jeton).await
}

pub async fn alertes(jeton: &str) -> Result<Vec<Alerte>, Erreur> {
    obtiens("/alerts", jeton).await
}

pub async fn ajoute(jeton: &str, p: &NouvellePosition) -> Result<Position, Erreur> {
    lis(envoie(json_vers(
        avec_jeton(Request::post(&url("/positions")), jeton),
        p,
    ))
    .await?)
    .await
}

pub async fn modifie(jeton: &str, id: i64, p: &NouvellePosition) -> Result<Position, Erreur> {
    let chemin = format!("/positions/{id}");
    lis(envoie(json_vers(avec_jeton(Request::put(&url(&chemin)), jeton), p)).await?).await
}

pub async fn supprime(jeton: &str, id: i64) -> Result<(), Erreur> {
    let chemin = format!("/positions/{id}");
    vide(envoie(avec_jeton(Request::delete(&url(&chemin)), jeton).build()).await?).await
}

pub async fn itineraire(
    jeton: &str,
    points: &[Coord],
    vitesse_kmh: f64,
) -> Result<Itineraire, Erreur> {
    #[derive(Serialize)]
    struct Demande<'a> {
        points: &'a [Coord],
        speed_kmh: f64,
    }
    let corps = Demande {
        points,
        speed_kmh: vitesse_kmh,
    };
    lis(envoie(json_vers(
        avec_jeton(Request::post(&url("/route/multi")), jeton),
        &corps,
    ))
    .await?)
    .await
}

pub async fn importe_gpx(jeton: &str, gpx: String) -> Result<Vec<Position>, Erreur> {
    let r = avec_jeton(Request::post(&url("/positions/import")), jeton)
        .header("content-type", "application/gpx+xml")
        .body(gpx);
    lis(envoie(r).await?).await
}

/// Vote sur un signalement : « toujours là » ou « plus là ». La mise à jour
/// revient par le temps réel, comme pour tout le monde.
pub async fn vote(jeton: &str, id: i64, toujours_la: bool) -> Result<(), Erreur> {
    #[derive(Serialize)]
    struct Vote {
        up: bool,
    }
    let chemin = format!("/alerts/{id}/vote");
    vide(
        envoie(json_vers(
            avec_jeton(Request::post(&url(&chemin)), jeton),
            &Vote { up: toujours_la },
        ))
        .await?,
    )
    .await
}

// --- Partage de position, sans compte ----------------------------------------

/// Un partage ouvert par le serveur.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct PartageOuvert {
    pub token: String,
    /// Absent d'un serveur trop ancien : le partage marche quand même.
    #[serde(default)]
    pub secret: String,
}

/// Ouvre un lien de suivi au nom donné. Aucun compte : le serveur ne garde
/// que le nom et la dernière position, trois heures après le dernier envoi.
pub async fn partage_ouvre(nom: &str) -> Result<PartageOuvert, Erreur> {
    #[derive(Serialize)]
    struct Demande<'a> {
        name: &'a str,
    }
    lis(envoie(json_vers(
        Request::post(&url("/live")),
        &Demande { name: nom },
    ))
    .await?)
    .await
}

/// Envoie une position au lien de suivi. `Statut(404, _)` : le partage a
/// expiré ou a été arrêté ailleurs.
pub async fn partage_envoie(jeton: &str, secret: &str, c: Coord) -> Result<(), Erreur> {
    let chemin = format!("/live/{}", encode_composant(jeton));
    vide(
        envoie(json_vers(
            Request::post(&url(&chemin)).header("x-live-secret", secret),
            &c,
        ))
        .await?,
    )
    .await
}

/// Arrête le partage : le lien ne montre plus rien.
pub async fn partage_arrete(jeton: &str, secret: &str) -> Result<(), Erreur> {
    let chemin = format!("/live/{}", encode_composant(jeton));
    vide(
        envoie(
            Request::delete(&url(&chemin))
                .header("x-live-secret", secret)
                .build(),
        )
        .await?,
    )
    .await
}

/// Le lien de suivi à envoyer : absolu, pour qu'il s'ouvre ailleurs.
/// `origine` est celle de la page (`https://hôte`), qui sert quand l'API
/// est sur ce même serveur.
pub fn url_suivi(base: &str, origine: &str, jeton: &str) -> String {
    let racine = if base.is_empty() { origine } else { base };
    format!("{racine}/live/{}", encode_composant(jeton))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_lien_de_suivi_est_absolu() {
        assert_eq!(
            url_suivi("", "https://moncap.fr", "ab12"),
            "https://moncap.fr/live/ab12"
        );
        assert_eq!(
            url_suivi("https://api.moncap.fr", "https://moncap.fr", "ab12"),
            "https://api.moncap.fr/live/ab12"
        );
    }

    #[test]
    fn l_encodage_suit_encode_uri_component() {
        assert_eq!(encode_composant("a.b-c_d~"), "a.b-c_d~");
        assert_eq!(encode_composant("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(encode_composant("é"), "%C3%A9");
    }

    #[test]
    fn le_websocket_suit_le_protocole_de_la_page() {
        assert_eq!(
            url_ws("", "https:", "moncap.app", "j.w.t"),
            "wss://moncap.app/ws?token=j.w.t"
        );
        assert_eq!(
            url_ws("", "http:", "localhost:3000", "x"),
            "ws://localhost:3000/ws?token=x"
        );
    }

    #[test]
    fn le_websocket_suit_une_api_distante() {
        assert_eq!(
            url_ws("https://api.example", "http:", "ici", "x"),
            "wss://api.example/ws?token=x"
        );
        assert_eq!(
            url_ws("http://localhost:3000", "https:", "ici", "x"),
            "ws://localhost:3000/ws?token=x"
        );
    }

    #[test]
    fn les_erreurs_parlent_francais() {
        assert!(Erreur::Statut(429, String::new())
            .message()
            .contains("minute"));
        assert_eq!(
            Erreur::Statut(400, "nom d'utilisateur déjà pris\n".into()).message(),
            "nom d'utilisateur déjà pris"
        );
        assert_eq!(
            Erreur::Statut(500, String::new()).message(),
            "Le serveur a répondu 500."
        );
    }

    #[test]
    fn la_porte_montre_le_message_du_serveur() {
        assert_eq!(
            message_statut(401, "identifiant ou mot de passe incorrect", None),
            "identifiant ou mot de passe incorrect"
        );
        assert_eq!(
            message_statut(429, "Trop de tentatives, patiente un peu.", Some(30)),
            "Trop de tentatives, patiente un peu."
        );
        // Sans message, l'attente du `Retry-After` est dite.
        assert_eq!(
            message_statut(429, "", Some(42)),
            "Trop de tentatives — réessaie dans 42 s."
        );
        assert_eq!(
            message_statut(429, " ", Some(125)),
            "Trop de tentatives — réessaie dans 3 min."
        );
        // Une page HTML n'est jamais montrée telle quelle.
        assert_eq!(
            message_statut(502, "<html><body>Bad gateway</body></html>", None),
            "Le serveur a répondu 502."
        );
    }

    #[test]
    fn le_reseau_coupe_se_dit_simplement() {
        let m = Erreur::Reseau.message();
        assert!(m.starts_with("Serveur injoignable"), "{m}");
        assert!(!Erreur::Illisible.message().contains("json"));
    }

    #[test]
    fn retry_after_en_secondes() {
        assert_eq!(secondes_retry_after(" 30 "), Some(30));
        assert_eq!(secondes_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
    }

    #[test]
    fn l_identifiant_part_sans_espaces() {
        assert_eq!(identifiant("  Toi@Exemple.fr \n"), "Toi@Exemple.fr");
    }

    #[test]
    fn un_mot_de_passe_refuse_ne_ferme_pas_la_session() {
        assert_eq!(
            erreur_401_mot_de_passe("mot de passe actuel incorrect"),
            Erreur::Statut(401, "mot de passe actuel incorrect".into())
        );
        assert_eq!(
            erreur_401_mot_de_passe("authentification requise"),
            Erreur::NonAutorise
        );
        assert_eq!(erreur_401_mot_de_passe(""), Erreur::NonAutorise);
    }
}
