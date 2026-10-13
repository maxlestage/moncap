//! Les erreurs de l'API et leur forme sur le fil.
//!
//! Le corps d'une erreur est du **texte brut**, jamais du JSON : l'app iOS
//! affiche tel quel le corps des refus de connexion et d'inscription, et les
//! autres clients ne lisent que le statut. Les messages restent en français.

use paquets::orm;
use paquets::vitesse::{self, IntoResponse, Request, Response, StatusCode};
use serde::de::DeserializeOwned;

/// Message des 401 : l'app iOS le compare pour revenir à l'écran de connexion.
pub const AUTH_REQUISE: &str = "authentification requise";

#[derive(Debug)]
pub enum Erreur {
    /// 400, avec un message lisible.
    Requete(String),
    /// 401 `authentification requise`.
    NonAuthentifie,
    /// 401 avec un message précis (identifiants refusés…).
    Refus(String),
    /// 404, avec un message (vide : corps vide).
    Introuvable(String),
    /// 413.
    TropGros,
    /// 415.
    TypeNonSupporte,
    /// 422 : un JSON bien formé mais qui n'a pas la forme attendue.
    Invalide(String),
    /// 429, et dans combien de secondes réessayer.
    TropDeTentatives { message: String, secondes: u64 },
    /// 503, avec un message (vide : corps vide).
    Indisponible(String),
    /// 500 `erreur interne` ; le détail part dans le journal, pas au client.
    Interne(String),
    /// N'importe quel statut, avec un corps texte.
    Statut(StatusCode, String),
}

pub type Resultat<T> = Result<T, Erreur>;

impl Erreur {
    pub fn requete(message: impl Into<String>) -> Self {
        Erreur::Requete(message.into())
    }

    pub fn introuvable(message: impl Into<String>) -> Self {
        Erreur::Introuvable(message.into())
    }

    pub fn interne(detail: impl std::fmt::Display) -> Self {
        Erreur::Interne(detail.to_string())
    }

    pub fn statut(&self) -> StatusCode {
        match self {
            Erreur::Requete(_) => StatusCode::BAD_REQUEST,
            Erreur::NonAuthentifie | Erreur::Refus(_) => StatusCode::UNAUTHORIZED,
            Erreur::Introuvable(_) => StatusCode::NOT_FOUND,
            Erreur::TropGros => StatusCode::PAYLOAD_TOO_LARGE,
            Erreur::TypeNonSupporte => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Erreur::Invalide(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Erreur::TropDeTentatives { .. } => StatusCode::TOO_MANY_REQUESTS,
            Erreur::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
            Erreur::Interne(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Erreur::Statut(s, _) => *s,
        }
    }
}

/// Une réponse texte brut, comme toutes les erreurs de l'API.
pub fn texte(statut: StatusCode, corps: impl Into<String>) -> Response {
    let corps = corps.into();
    let res = Response::new().status(statut);
    if corps.is_empty() {
        res
    } else {
        res.content_type("text/plain; charset=utf-8").send(corps)
    }
}

impl IntoResponse for Erreur {
    fn into_response(self) -> Response {
        let statut = self.statut();
        match self {
            Erreur::Requete(m)
            | Erreur::Refus(m)
            | Erreur::Introuvable(m)
            | Erreur::Invalide(m)
            | Erreur::Indisponible(m)
            | Erreur::Statut(_, m) => texte(statut, m),
            Erreur::NonAuthentifie => texte(statut, AUTH_REQUISE),
            Erreur::TropGros => texte(statut, "corps trop volumineux"),
            Erreur::TypeNonSupporte => texte(statut, "type de contenu non pris en charge"),
            Erreur::TropDeTentatives { message, secondes } => {
                texte(statut, message).header("retry-after", secondes.max(1).to_string())
            }
            Erreur::Interne(detail) => {
                tracing::error!(%detail, "erreur interne");
                texte(statut, "erreur interne")
            }
        }
    }
}

impl From<orm::Error> for Erreur {
    fn from(e: orm::Error) -> Self {
        Erreur::Interne(format!("base de données : {e}"))
    }
}

impl From<vitesse::Error> for Erreur {
    fn from(e: vitesse::Error) -> Self {
        match e.status() {
            StatusCode::PAYLOAD_TOO_LARGE => Erreur::TropGros,
            s if s.is_server_error() => Erreur::Interne(e.message().to_owned()),
            s => Erreur::Statut(s, e.message().to_owned()),
        }
    }
}

/// Le corps JSON de la requête, avec les statuts que les clients connaissent :
/// 415 sans `Content-Type: application/json`, 400 pour un JSON mal formé,
/// 422 pour un JSON bien formé qui n'a pas la forme attendue.
pub async fn corps_json<T: DeserializeOwned>(req: &Request) -> Resultat<T> {
    let json = req
        .content_type()
        .map(|t| {
            let t = t.split(';').next().unwrap_or("").trim();
            t.eq_ignore_ascii_case("application/json")
                || (t.starts_with("application/") && t.ends_with("+json"))
        })
        .unwrap_or(false);
    if !json {
        return Err(Erreur::TypeNonSupporte);
    }
    let octets = req.bytes().await?;
    serde_json::from_slice(&octets).map_err(|e| {
        use serde_json::error::Category;
        match e.classify() {
            Category::Data => Erreur::Invalide(format!("corps JSON invalide : {e}")),
            _ => Erreur::Requete(format!("corps JSON mal formé : {e}")),
        }
    })
}

/// Les paramètres de la requête (`?a=…&b=…`) ; 400 s'ils ne collent pas.
pub fn requete_query<T: DeserializeOwned>(req: &Request) -> Resultat<T> {
    req.query_as::<T>()
        .map_err(|e| Erreur::Requete(format!("paramètres invalides : {}", e.message())))
}

/// Un paramètre de chemin numérique ; 400 s'il ne se lit pas.
pub fn param<T: std::str::FromStr>(req: &Request, nom: &str) -> Resultat<T> {
    req.param(nom)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| Erreur::Requete(format!("paramètre « {nom} » invalide")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_erreurs_sont_du_texte() {
        let r = Erreur::NonAuthentifie.into_response();
        assert_eq!(r.status_code(), 401);
        assert_eq!(
            r.get_header("content-type"),
            Some("text/plain; charset=utf-8")
        );
    }

    #[test]
    fn trop_de_tentatives_dit_quand_revenir() {
        let r = Erreur::TropDeTentatives {
            message: "attends".into(),
            secondes: 42,
        }
        .into_response();
        assert_eq!(r.status_code(), 429);
        assert_eq!(r.get_header("retry-after"), Some("42"));
    }

    #[test]
    fn un_introuvable_vide_n_a_pas_de_corps() {
        let r = Erreur::introuvable("").into_response();
        assert_eq!(r.status_code(), 404);
        assert_eq!(r.get_header("content-type"), None);
    }
}
