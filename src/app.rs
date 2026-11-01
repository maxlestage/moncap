//! L'application : le bouclier, les routes, les tâches de fond.

use std::time::Duration;

use paquets::prelude::*;
use paquets::securite::prelude::*;

use crate::cache::{self, CacheRef};
use crate::erreur::texte;
use crate::temps_reel::{self, Hub, HubRef};
use crate::{
    alertes, base, compte, connexion, externes, partage, positions, sauvegarde, statique, trajets,
    tuiles,
};

/// Les réglages qui changent entre la production et les tests.
pub struct Reglages {
    /// La base : une URL SQLite, ou une base déjà ouverte (les essais).
    pub base: paquets::DbSource,
    /// Le cache, une autre base SQLite (voir `cache`).
    pub cache: Db,
    /// La limitation de débit par adresse IP (coupée dans les tests, où
    /// toutes les requêtes viennent de la même adresse).
    pub limites: bool,
    /// Les tâches de fond (purges, ménage, rafraîchissement des feux).
    pub taches: bool,
}

impl Reglages {
    pub fn production() -> Self {
        let base = base::url_base();
        // Un dyno neuf a un disque vide : la base revient de sa sauvegarde
        // avant de s'ouvrir.
        sauvegarde::restaure(&base);
        let cache = cache::url_cache();
        tracing::info!("base {base}, cache {cache}");
        Reglages {
            base: base.into(),
            cache: cache::ouvre(&cache),
            limites: true,
            taches: true,
        }
    }
}

/// Le bouclier devant chaque requête : identifiants de requête, hygiène des
/// requêtes, limite de taille, limitation de débit, blocage des robots
/// d'attaque, pare-feu applicatif, en-têtes de sécurité.
///
/// Pas de garde CSRF : l'API s'authentifie par jeton `Bearer`, jamais par
/// cookie, une page tierce ne peut donc rien faire au nom de quelqu'un.
fn bouclier(limites: bool) -> Shield {
    let ip = if std::env::var("DYNO").is_ok() {
        // Derrière le routeur Heroku : la dernière adresse de
        // X-Forwarded-For, celle que le client ne peut pas falsifier.
        ClientIpSource::heroku()
    } else {
        ClientIpSource::Peer
    };
    let mut b = paquets::shield()
        .client_ip(ip)
        .without("csrf")
        .guard(
            Waf::new()
                // Les adresses partagées portent du texte libre (`nom`) et
                // les tuiles sont du trafic massif sans paramètres.
                .skip_paths(["/t", "/tiles", "/web/", "/vitrine/"]),
        )
        .guard(
            SecurityHeaders::new()
                // Chaque page pose sa propre politique (les fronts WebAssembly,
                // les cartes Leaflet) ; l'API n'en a pas besoin.
                .csp(None)
                .cross_origin_resource_policy("cross-origin")
                // Le front web localise l'utilisateur : la géolocalisation
                // reste permise à la page elle-même.
                .permissions_policy(
                    "camera=(), microphone=(), payment=(), usb=(), geolocation=(self)",
                ),
        )
        .error_format(ErrorFormat::custom(|refus, _ctx| {
            let message = match refus.status().as_u16() {
                429 => "trop de requêtes, réessaie plus tard".to_owned(),
                413 => "corps trop volumineux".to_owned(),
                403 => "requête refusée".to_owned(),
                _ => refus.message().to_owned(),
            };
            let mut res = paquets::vitesse::http::Response::builder()
                .status(refus.status())
                .header("content-type", "text/plain; charset=utf-8");
            for (nom, valeur) in refus.headers() {
                res = res.header(nom.clone(), valeur.clone());
            }
            res.body(message.into()).unwrap_or_default()
        }));
    b = if limites {
        b.guard(
            RateLimit::new(Quota::per_minute(300).burst(300))
                // Les connexions et inscriptions ont leur frein par compte
                // (voir `connexion`) ; ceci n'arrête que le martèlement.
                .route("/auth", Quota::per_minute(30).burst(30))
                .route("/tiles", Quota::per_minute(6000).burst(1000)),
        )
    } else {
        b.without("rate_limit")
    };
    b.build()
}

/// L'origine est-elle autorisée à appeler l'API depuis un navigateur ?
/// Développement local (`localhost`, `127.0.0.1`, tout port), les applis
/// Heroku, et les origines listées dans `ALLOWED_ORIGINS`.
pub fn origine_autorisee(origine: &str, liste: &[String]) -> bool {
    if liste.iter().any(|o| o == origine) {
        return true;
    }
    let (schema, reste) = match origine.split_once("://") {
        Some(x) => x,
        None => return false,
    };
    if reste.contains('/') || reste.contains('@') {
        return false;
    }
    let hote = match reste.rsplit_once(':') {
        Some((h, port)) if port.bytes().all(|b| b.is_ascii_digit()) && !port.is_empty() => h,
        _ => reste,
    };
    match schema {
        "http" | "https" if hote == "localhost" || hote == "127.0.0.1" => true,
        "https" => {
            hote.ends_with(".herokuapp.com")
                && hote.len() > ".herokuapp.com".len()
                && hote
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        }
        _ => false,
    }
}

fn origines_listees() -> Vec<String> {
    std::env::var("ALLOWED_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

/// CORS : répond aux pré-vols et marque les réponses des origines admises.
async fn cors(req: Request, next: Next) -> Response {
    let Some(origine) = req.header("origin").map(str::to_owned) else {
        return next.run(req).await;
    };
    let admise = origine_autorisee(&origine, &origines_listees());
    let pre_vol =
        req.method() == Method::OPTIONS && req.header("access-control-request-method").is_some();
    if pre_vol {
        let mut res = Response::new().status(204).header("vary", "origin");
        if admise {
            let entetes = req
                .header("access-control-request-headers")
                .unwrap_or("*")
                .to_owned();
            res = res
                .header("access-control-allow-origin", origine)
                .header(
                    "access-control-allow-methods",
                    "GET, POST, PUT, PATCH, DELETE, OPTIONS",
                )
                .header("access-control-allow-headers", entetes)
                .header("access-control-max-age", "600");
        }
        return res;
    }
    let mut res = next.run(req).await;
    res.set_header("vary", "origin");
    if admise {
        res.set_header("access-control-allow-origin", origine);
        res.set_header("access-control-expose-headers", "retry-after");
    }
    res
}

/// GET /health — le serveur répond et la base aussi.
async fn sante(req: Request) -> Response {
    let ok: Result<Vec<(i32,)>, _> = req.db().raw("SELECT 1", &[]).await;
    match ok {
        Ok(_) => texte(StatusCode::OK, "ok"),
        Err(e) => {
            tracing::warn!("base indisponible : {e}");
            texte(StatusCode::SERVICE_UNAVAILABLE, "base indisponible")
        }
    }
}

/// L'application complète.
pub fn application(reglages: Reglages) -> App {
    let mut app = App::new();
    app.database(reglages.base);
    app.security(bouclier(reglages.limites));
    app.on_error(|e: paquets::Error| {
        let statut = e.status();
        if statut.is_server_error() {
            texte(statut, "erreur interne")
        } else {
            texte(statut, e.message().to_owned())
        }
    });
    app.middleware(cors);

    let hub: &'static Hub = Box::leak(Box::new(Hub::new()));
    app.state(HubRef(hub));
    let cache: &'static Db = Box::leak(Box::new(reglages.cache));
    app.state(CacheRef(cache));

    app.get("/health", sante);
    connexion::routes(&mut app);
    temps_reel::routes(&mut app);
    alertes::routes(&mut app);
    positions::routes(&mut app);
    trajets::routes(&mut app);
    compte::routes(&mut app);
    externes::routes(&mut app);
    tuiles::routes(&mut app);
    partage::routes(&mut app);
    statique::routes(&mut app);

    let taches = reglages.taches;
    app.on_start(move |db| async move {
        // Une seule instance écrit dans le fichier : pas de course entre
        // serveurs qui démarrent ensemble, la migration passe ou échoue.
        let faites = base::migrations().run(db).await?;
        if !faites.is_empty() {
            tracing::info!(?faites, "migrations appliquées");
        }
        // Un cache en panne ne doit pas empêcher de servir : sans sa table,
        // chaque lecture est un défaut de cache, et rien de plus.
        if let Err(e) = cache::prepare(cache).await {
            tracing::warn!("cache indisponible : {e}");
        }
        if taches {
            demarre_taches(db, cache);
        }
        Ok(())
    });
    app
}

/// Les tâches de fond, sur le fil que Paquets garde pour elles : la purge des
/// alertes chaque minute, et chaque heure le ménage des sessions et du cache,
/// puis `PRAGMA optimize`, qui tient à jour les statistiques dont SQLite se
/// sert pour choisir ses index.
fn demarre_taches(db: &'static Db, cache: &'static Db) {
    tokio::spawn(async move {
        let mut minute = tokio::time::interval(Duration::from_secs(60));
        let mut tours: u64 = 0;
        loop {
            minute.tick().await;
            if let Err(e) = alertes::purge(db).await {
                tracing::warn!("purge des alertes expirées : {e:?}");
            }
            if tours.is_multiple_of(60) {
                if let Err(e) = connexion::menage(db).await {
                    tracing::warn!("ménage des sessions : {e:?}");
                }
                match cache::menage(cache, cache::AGE_MAX, cache::octets_max()).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!("cache : {n} entrées retirées"),
                    Err(e) => tracing::warn!("ménage du cache : {e}"),
                }
                for base in [db, cache] {
                    if let Err(e) = base.raw_execute("PRAGMA optimize", &[]).await {
                        tracing::warn!("PRAGMA optimize : {e}");
                    }
                }
            }
            tours += 1;
        }
    });
    externes::demarre(cache);
    sauvegarde::demarre(db);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_origines_sont_comparees_exactement() {
        let liste = vec!["https://moncap.app".to_owned()];
        for ok in [
            "http://localhost",
            "http://localhost:8080",
            "http://127.0.0.1:3000",
            "https://moncap-c41a5aaf07e8.herokuapp.com",
            "https://moncap.app",
        ] {
            assert!(origine_autorisee(ok, &liste), "{ok}");
        }
        for non in [
            "http://localhost.evil.com",
            "http://127.0.0.1.evil.com",
            "https://evil.com",
            "https://herokuapp.com",
            "http://x.herokuapp.com",
            "https://moncap.app.evil.com",
            "null",
        ] {
            assert!(!origine_autorisee(non, &liste), "{non}");
        }
    }
}
