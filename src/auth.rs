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
