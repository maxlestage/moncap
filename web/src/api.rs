//! Le client de l'API MonCap — les routes de `src/main.rs`.

use gloo_net::http::{Request, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::session::{self, Session};
use crate::types::{Alerte, Coord, Itineraire, Jeton, NouvellePosition, Position, Stats};

/// Ce qui peut mal tourner en parlant au serveur.
#[derive(Clone, Debug, PartialEq)]
pub enum Erreur {
    /// Jeton absent, expiré ou révoqué : la session doit se refermer.
    NonAutorise,
    /// Le serveur a répondu, mais pas ce qu'on attendait.
    Statut(u16, String),
    /// Le serveur n'a pas répondu du tout, ou dans une langue inconnue.
    Reseau,
}

impl Erreur {
    /// Le message à montrer, en français.
    pub fn message(&self) -> String {
        match self {
            Erreur::NonAutorise => "Session expirée : reconnecte-toi.".into(),
            Erreur::Statut(429, _) => "Trop de tentatives — réessaie dans une minute.".into(),
            Erreur::Statut(503, _) => "Service momentanément indisponible.".into(),
            Erreur::Statut(_, texte) if !texte.trim().is_empty() => texte.trim().to_string(),
            Erreur::Statut(code, _) => format!("Le serveur a répondu {code}."),
            Erreur::Reseau => format!("Impossible de joindre l'API ({}).", nom_du_serveur()),
        }
    }
}

fn nom_du_serveur() -> String {
    let base = session::base_api();
    if base.is_empty() {
        "même serveur".into()
    } else {
        base
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
        _ => r.json::<T>().await.map_err(|_| Erreur::Reseau),
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

// --- Authentification -------------------------------------------------------

#[derive(Serialize)]
struct Identifiants<'a> {
    username: &'a str,
    password: &'a str,
}

/// Connexion ou inscription : la même requête, deux adresses.
pub async fn authentifie(
    inscription: bool,
    email: &str,
    mot_de_passe: &str,
) -> Result<Session, String> {
    let chemin = if inscription {
        "/auth/signup"
    } else {
        "/auth/login"
    };
    let corps = Identifiants {
        username: email,
        password: mot_de_passe,
    };
    let r = envoie(json_vers(Request::post(&url(chemin)), &corps))
        .await
        .map_err(|e| e.message())?;
    match r.status() {
        // À la connexion, 401 veut dire « mauvais identifiants », pas
        // « session expirée » : on le dit sans révéler lequel des deux.
        401 => Err("E-mail ou mot de passe incorrect.".into()),
        s if !(200..300).contains(&s) => {
            Err(Erreur::Statut(s, r.text().await.unwrap_or_default()).message())
        }
        _ => {
            let j: Jeton = r.json().await.map_err(|_| Erreur::Reseau.message())?;
            Ok(Session {
                jeton: j.token,
                utilisateur: j.username,
            })
        }
    }
}

/// Demande un lien de réinitialisation. Le serveur répond 204 que le compte
/// existe ou non — la réponse ne dit jamais si une adresse est inscrite —
/// et 503 quand l'envoi de courriels n'est pas configuré.
pub async fn mot_de_passe_oublie(email: &str) -> Result<(), String> {
    #[derive(Serialize)]
    struct Demande<'a> {
        username: &'a str,
    }
    let r = envoie(json_vers(
        Request::post(&url("/auth/forgot")),
        &Demande { username: email },
    ))
    .await
    .map_err(|e| e.message())?;
    match r.status() {
        503 => Err("La réinitialisation n'est pas encore configurée.".into()),
        s if !(200..300).contains(&s) => Err(Erreur::Statut(s, String::new()).message()),
        _ => Ok(()),
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
