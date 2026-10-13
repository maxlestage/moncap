//! Les fronts : la vitrine (`vitrine/dist`) et l'application web (`web/dist`).
//!
//! Les pages à adresse fixe se revalident à chaque visite (`no-cache`) ; les
//! fichiers empreintés (`/vitrine/…`, `/web/…`) se gardent un an. Toute autre
//! adresse en GET reçoit l'application web, qui fait son propre routage.

use std::path::{Component, Path, PathBuf};

use paquets::prelude::*;

use crate::erreur::texte;

const VITRINE: &str = "vitrine/dist";
const WEB: &str = "web/dist";

const REVALIDER: &str = "no-cache";
const FIGE: &str = "public, max-age=31536000, immutable";

/// Les pages de la vitrine : (adresse, fichier sous `vitrine/dist`).
const PAGES: &[(&str, &str)] = &[
    ("/", "index.html"),
    ("/es", "es/index.html"),
    ("/en", "en/index.html"),
    ("/mentions-legales", "mentions-legales/index.html"),
    ("/confidentialite", "confidentialite/index.html"),
    ("/es/aviso-legal", "es/aviso-legal/index.html"),
    ("/es/privacidad", "es/privacidad/index.html"),
    ("/en/legal", "en/legal/index.html"),
    ("/en/privacy", "en/privacy/index.html"),
    ("/robots.txt", "robots.txt"),
    ("/sitemap.xml", "sitemap.xml"),
];

pub fn routes(app: &mut App) {
    for &(adresse, fichier) in PAGES {
        app.get(adresse, move |_req: Request| async move {
            sert(Path::new(VITRINE).join(fichier), REVALIDER).await
        });
    }
    app.get("/app", |_req: Request| async move {
        sert(Path::new(WEB).join("index.html"), REVALIDER).await
    });
    app.get("/vitrine/*chemin", |req: Request| async move {
        fichier_empreinte(&req, VITRINE).await
    });
    app.get("/web/*chemin", |req: Request| async move {
        fichier_empreinte(&req, WEB).await
    });
    app.fallback(repli);
}

/// Un chemin relatif sans `..`, ni racine, ni fichier caché.
fn chemin_sur(brut: &str) -> Option<PathBuf> {
    let p = Path::new(brut.trim_start_matches('/'));
    let mut sur = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(s) if !s.to_string_lossy().starts_with('.') => sur.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(sur)
}

/// Sert un fichier (ETag, `Last-Modified`, 304, plages), avec ce cache.
async fn sert(chemin: PathBuf, cache: &str) -> Response {
    let mut res = res::file(&chemin).await;
    if res.status_code().is_success() || res.status_code() == StatusCode::NOT_MODIFIED {
        res.set_header("cache-control", cache);
    }
    res
}

/// `/vitrine/*` et `/web/*` : les fichiers empreintés, gardés un an. Un
/// fichier absent répond 404 **sans** cache, pour qu'un déploiement en cours
/// ne fige pas l'absence chez le client.
async fn fichier_empreinte(req: &Request, racine: &str) -> Response {
    let Some(rel) = req.param("chemin").and_then(chemin_sur) else {
        return texte(StatusCode::NOT_FOUND, "introuvable");
    };
    let mut chemin = Path::new(racine).join(rel);
    if chemin.is_dir() {
        chemin.push("index.html");
    }
    if !chemin.is_file() {
        return texte(StatusCode::NOT_FOUND, "introuvable");
    }
    sert(chemin, FIGE).await
}

/// Toute autre adresse : le fichier de `web/dist` s'il existe, sinon la page
/// de l'application web (200), qui route elle-même. Hors GET : 404.
async fn repli(req: Request) -> Response {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return texte(StatusCode::NOT_FOUND, "introuvable");
    }
    if let Some(rel) = chemin_sur(req.path())
        && !rel.as_os_str().is_empty()
    {
        let chemin = Path::new(WEB).join(&rel);
        if chemin.is_file() {
            return res::file(chemin).await;
        }
    }
    res::file(Path::new(WEB).join("index.html")).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_chemins_ne_sortent_pas_du_dossier() {
        assert_eq!(chemin_sur("a/b.js"), Some(PathBuf::from("a/b.js")));
        assert_eq!(chemin_sur("/a/./b.js"), Some(PathBuf::from("a/b.js")));
        assert_eq!(chemin_sur("../secret"), None);
        assert_eq!(chemin_sur("a/../../b"), None);
        assert_eq!(chemin_sur(".env"), None);
    }

    #[test]
    fn pages_vitrine_construites() {
        for (_, fichier) in PAGES {
            assert!(
                Path::new(VITRINE).join(fichier).is_file(),
                "{fichier} manque dans {VITRINE}"
            );
        }
    }

    #[test]
    fn front_web_construit() {
        let index = std::fs::read_to_string(Path::new(WEB).join("index.html"))
            .expect("web/dist/index.html");
        let refs: Vec<&str> = index
            .split('"')
            .filter(|s| s.starts_with("/web/"))
            .collect();
        assert!(
            refs.len() >= 5,
            "trop peu de fichiers référencés : {refs:?}"
        );
        for r in refs {
            let f = Path::new(WEB).join(r.trim_start_matches("/web/"));
            assert!(f.is_file(), "{r} référencé mais absent");
        }
    }
}
