//! Ce que le navigateur retient d'une visite à l'autre : l'adresse de
//! l'API, le jeton, l'identifiant et l'avatar.
//!
//! Les clés sont celles de l'ancien front React : une session ouverte avant
//! la migration reste ouverte après.

const CLE_BASE: &str = "moncap.apiBase";
const CLE_JETON: &str = "moncap.token";
const CLE_UTILISATEUR: &str = "moncap.user";
const CLE_AVATAR: &str = "moncap.avatar";

fn stockage() -> Option<web_sys::Storage> {
    // Le stockage peut manquer — navigation privée stricte, cookies
    // bloqués — et l'accès lui-même peut lever : on traite les deux comme
    // une absence, jamais comme une panne.
    web_sys::window()?.local_storage().ok().flatten()
}

fn lis(cle: &str) -> String {
    stockage()
        .and_then(|s| s.get_item(cle).ok().flatten())
        .unwrap_or_default()
}

fn ecris(cle: &str, valeur: &str) {
    if let Some(s) = stockage() {
        let _ = s.set_item(cle, valeur);
    }
}

fn efface(cle: &str) {
    if let Some(s) = stockage() {
        let _ = s.remove_item(cle);
    }
}

/// L'adresse de l'API, vide pour « ce même serveur » — le cas d'Heroku,
/// où le serveur sert aussi le front.
pub fn base_api() -> String {
    lis(CLE_BASE)
}

pub fn ecris_base_api(url: &str) {
    ecris(CLE_BASE, &normalise_base(url));
}

/// Sans barre finale : les chemins de l'API commencent tous par la leur.
pub fn normalise_base(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// Une session ouverte.
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub jeton: String,
    pub utilisateur: String,
}

pub fn session() -> Option<Session> {
    let jeton = lis(CLE_JETON);
    if jeton.is_empty() {
        return None;
    }
    Some(Session {
        jeton,
        utilisateur: lis(CLE_UTILISATEUR),
    })
}

pub fn ouvre(s: &Session) {
    ecris(CLE_JETON, &s.jeton);
    ecris(CLE_UTILISATEUR, &s.utilisateur);
}

pub fn ferme() {
    efface(CLE_JETON);
    efface(CLE_UTILISATEUR);
}

pub fn avatar() -> String {
    let a = lis(CLE_AVATAR);
    if a.is_empty() {
        "green".into()
    } else {
        a
    }
}

pub fn ecris_avatar(id: &str) {
    ecris(CLE_AVATAR, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_base_perd_ses_barres_finales() {
        assert_eq!(
            normalise_base(" https://x.herokuapp.com// "),
            "https://x.herokuapp.com"
        );
        assert_eq!(normalise_base(""), "");
    }
}
