use std::time::{SystemTime, UNIX_EPOCH};

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::AppError;

/// Contenu du jeton JWT.
#[derive(Serialize, Deserialize)]
struct Claims {
    sub: i32,
    exp: usize,
}

/// Secret de signature des jetons (variable d'env JWT_SECRET en production).
fn secret() -> Vec<u8> {
    std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "moncap-dev-secret-change-me".to_string())
        .into_bytes()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Crée un jeton valable 30 jours pour l'utilisateur.
pub fn make_token(user_id: i32) -> Result<String, AppError> {
    let claims = Claims {
        sub: user_id,
        exp: (now_secs() + 60 * 60 * 24 * 30) as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(&secret()),
    )
    .map_err(|_| AppError::Internal)
}

/// Renvoie l'id utilisateur si le jeton est valide.
pub fn verify_token(token: &str) -> Option<i32> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(&secret()),
        &Validation::default(),
    )
    .ok()
    .map(|data| data.claims.sub)
}

/// Hache un mot de passe (Argon2).
pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| AppError::Internal)
}

/// Vérifie un mot de passe contre son hachage.
pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Empreinte factice, calculée une seule fois, pour comparer un mot de passe
/// même quand l'utilisateur n'existe pas : le temps de réponse de la connexion
/// reste identique, ce qui empêche de deviner les comptes existants par
/// mesure du temps (énumération d'utilisateurs).
fn dummy_hash() -> &'static str {
    static H: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    H.get_or_init(|| hash_password("moncap-dummy-timing-guard").unwrap_or_default())
}

/// Consomme le même temps qu'une vérification réelle, sans rien authentifier.
pub fn dummy_verify(password: &str) {
    let _ = verify_password(password, dummy_hash());
}

/// Extracteur : exige un en-tête `Authorization: Bearer <jwt>` valide.
pub(crate) struct AuthUser(pub(crate) i32);

#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        verify_token(token)
            .map(AuthUser)
            .ok_or(AppError::Unauthorized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_round_trip() {
        let t = make_token(42).unwrap();
        assert_eq!(verify_token(&t), Some(42));
    }

    #[test]
    fn rejects_garbage_token() {
        assert_eq!(verify_token("pas-un-jeton"), None);
        assert_eq!(verify_token(""), None);
    }

    #[test]
    fn rejects_expired_token() {
        // Jeton déjà expiré (exp en 1970) signé avec le bon secret.
        let claims = Claims { sub: 7, exp: 1 };
        let t = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&secret()),
        )
        .unwrap();
        assert_eq!(verify_token(&t), None);
    }

    #[test]
    fn password_hash_round_trip() {
        let h = hash_password("s3cret!").unwrap();
        assert!(verify_password("s3cret!", &h));
        assert!(!verify_password("mauvais", &h));
    }

    #[test]
    fn verify_password_tolerates_bad_hash() {
        assert!(!verify_password("x", "pas-un-hash"));
    }

    #[test]
    fn dummy_verify_does_not_panic() {
        dummy_verify("n'importe quoi");
    }
}
