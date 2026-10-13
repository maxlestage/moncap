//! Les types échangés avec l'API — ceux de `src/main.rs`, vus du client.
//!
//! Ils étaient en TypeScript, recopiés à la main depuis le serveur ; une
//! faute de frappe dans un nom de champ y produisait un `undefined` muet.
//! Ici, un champ attendu qui manque fait échouer la lecture, et l'écran le
//! dit au lieu d'afficher « undefined km ».

use serde::{Deserialize, Serialize};

/// Un point, en degrés.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Coord {
    pub lat: f64,
    pub lon: f64,
}

/// Une position enregistrée.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Position {
    pub id: i64,
    pub lat: f64,
    pub lon: f64,
    pub label: String,
}

impl Position {
    pub fn coord(&self) -> Coord {
        Coord {
            lat: self.lat,
            lon: self.lon,
        }
    }
}

/// Ce qu'on envoie pour créer ou modifier une position.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NouvellePosition {
    pub lat: f64,
    pub lon: f64,
    pub label: String,
}

/// La réponse de `POST /route/multi`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Itineraire {
    pub total_km: f64,
    pub legs_km: Vec<f64>,
    pub duration_min: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Cadre {
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
}

/// La réponse de `GET /stats`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Stats {
    pub count: u64,
    pub total_km: f64,
    pub bbox: Option<Cadre>,
    pub centroid: Option<Coord>,
}

/// Un signalement communautaire, avec ses votes.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Alerte {
    pub id: i64,
    pub category: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub label: String,
    /// Création, en millisecondes depuis l'époque Unix.
    pub ts: f64,
    #[serde(default)]
    pub confirms: i32,
    #[serde(default)]
    pub denies: i32,
}

impl Alerte {
    pub fn coord(&self) -> Coord {
        Coord {
            lat: self.lat,
            lon: self.lon,
        }
    }
}

/// Un autre conducteur, vu en direct.
#[derive(Clone, Debug, PartialEq)]
pub struct Voiture {
    pub id: u64,
    pub lat: f64,
    pub lon: f64,
    pub label: String,
    pub avatar: String,
    /// Dernière nouvelle reçue, en millisecondes.
    pub vu_ms: f64,
}

impl Voiture {
    pub fn coord(&self) -> Coord {
        Coord {
            lat: self.lat,
            lon: self.lon,
        }
    }
}

/// La réponse de `/auth/login`, `/auth/signup` et `/auth/password` : une
/// session neuve.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Jeton {
    pub token: String,
    /// L'identifiant tel que le serveur l'a enregistré. Absent d'une
    /// réponse plus ancienne : on garde alors celui qu'on a tapé.
    #[serde(default)]
    pub username: String,
    /// L'expiration de la session, en secondes Unix (0 si inconnue). Le
    /// serveur la fait respecter : le client ne s'en sert pas pour décider.
    #[serde(default)]
    pub expires_at: i64,
}

/// La réponse de `GET /auth/me`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Moi {
    pub username: String,
    #[serde(default)]
    pub created_at: i64,
}

/// Une session ouverte sur un appareil (`GET /auth/sessions`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Appareil {
    pub id: i64,
    #[serde(default)]
    pub device: String,
    /// Ouverture et dernière activité, en secondes Unix.
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub last_seen_at: i64,
    /// La session de cet appareil-ci.
    #[serde(default)]
    pub current: bool,
}

/// Un lieu trouvé par le géocodage.
#[derive(Clone, Debug, PartialEq)]
pub struct Lieu {
    pub nom: String,
    pub coord: Coord,
}

impl Lieu {
    /// Le nom court : les trois premiers éléments de l'adresse, comme
    /// l'ancien bandeau. « 12 Rue X, Quartier, Ville » plutôt que
    /// l'adresse complète jusqu'au pays.
    pub fn nom_court(&self) -> String {
        self.nom
            .split(',')
            .take(3)
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_alerte_sans_votes_se_lit() {
        let a: Alerte = serde_json::from_str(
            r#"{"id":3,"category":"police","lat":45.0,"lon":1.0,"label":"","ts":1700000000000}"#,
        )
        .unwrap();
        assert_eq!((a.confirms, a.denies), (0, 0));
    }

    #[test]
    fn les_stats_d_une_liste_vide_se_lisent() {
        let s: Stats =
            serde_json::from_str(r#"{"count":0,"total_km":0.0,"bbox":null,"centroid":null}"#)
                .unwrap();
        assert_eq!(s.count, 0);
        assert!(s.bbox.is_none());
    }

    #[test]
    fn une_session_se_lit_avec_ou_sans_expiration() {
        let j: Jeton =
            serde_json::from_str(r#"{"token":"t","username":"a@b.fr","expires_at":1760000000}"#)
                .unwrap();
        assert_eq!((j.token.as_str(), j.expires_at), ("t", 1_760_000_000));
        let ancien: Jeton = serde_json::from_str(r#"{"token":"t"}"#).unwrap();
        assert_eq!((ancien.username.as_str(), ancien.expires_at), ("", 0));
        let a: Vec<Appareil> = serde_json::from_str(
            r#"[{"id":3,"device":"Firefox · Linux","created_at":1,"last_seen_at":2,"current":true}]"#,
        )
        .unwrap();
        assert!(a[0].current);
    }

    #[test]
    fn le_nom_court_garde_trois_elements() {
        let l = Lieu {
            nom: "Place de la Bourse, Saint-Pierre, Bordeaux, Gironde, France".into(),
            coord: Coord { lat: 0.0, lon: 0.0 },
        };
        assert_eq!(l.nom_court(), "Place de la Bourse, Saint-Pierre, Bordeaux");
    }
}
