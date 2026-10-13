//! Ce qui se partage hors de l'app : la position en direct (`/live`), un
//! itinéraire (`/t`), et la politique de confidentialité (`/privacy`).
//!
//! Les pages HTML posent chacune leur `content-security-policy` (le bouclier
//! n'en pose aucune globalement) : Leaflet 1.9.4 depuis unpkg, les tuiles
//! OpenStreetMap, et leurs blocs `<script>` / `<style>` en ligne, autorisés
//! par leur empreinte SHA-256 calculée sur le HTML réellement envoyé.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use paquets::prelude::*;
use paquets::vitesse;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::base::maintenant;
use crate::commun::{coord_valide, nettoie_label};
use crate::connexion::jeton_aleatoire;
use crate::erreur::{Erreur, Resultat, corps_json, requete_query, texte};

pub fn routes(app: &mut App) {
    enregistre(app);
}

/// Les routes, sur l'application nue : les essais s'en servent sans base.
fn enregistre(app: &mut vitesse::App) {
    app.post("/live", cree_partage);
    app.get("/live/:token", page_suivi);
    app.post("/live/:token", met_a_jour);
    app.delete("/live/:token", arrete);
    app.get("/live/:token/pos", position);
    app.get("/privacy", confidentialite);
    app.get("/t", trajet_partage);
}

// ---------------------------------------------------------------------------
// Politique de sécurité des pages.
// ---------------------------------------------------------------------------

/// Base64 standard (avec remplissage), pour les empreintes CSP.
fn base64(octets: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(octets.len().div_ceil(3) * 4);
    for bloc in octets.chunks(3) {
        let n = (bloc[0] as u32) << 16
            | (*bloc.get(1).unwrap_or(&0) as u32) << 8
            | *bloc.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= bloc.len() {
                s.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// La source CSP `'sha256-…'` du premier bloc `ouvrant … </fermant>` du HTML
/// (le contenu exact entre les balises, tel que le navigateur le hache).
fn empreinte_bloc(html: &str, ouvrant: &str, fermant: &str) -> String {
    let contenu = html
        .split_once(ouvrant)
        .and_then(|(_, reste)| reste.split_once(fermant))
        .map(|(c, _)| c)
        .unwrap_or_default();
    format!("'sha256-{}'", base64(&Sha256::digest(contenu.as_bytes())))
}

/// Leaflet, à la version près : feuille de style, script et icônes.
const LEAFLET: &str = "https://unpkg.com/leaflet@1.9.4/";

/// La politique d'une page de carte Leaflet. Le script en ligne passe par son
/// empreinte ; les styles en ligne, eux, restent permis (`'unsafe-inline'`) :
/// Leaflet fabrique son balisage à la volée, et un style injecté ne peut pas
/// exécuter de code. `connect` : ce que la page interroge (`'self'` pour le
/// suivi en direct, rien pour un itinéraire).
fn csp_carte(html: &str, connect: &str) -> String {
    format!(
        "default-src 'none'; script-src {LEAFLET} {script}; \
         style-src {LEAFLET} 'unsafe-inline'; \
         img-src {LEAFLET} https://tile.openstreetmap.org https://*.tile.openstreetmap.org data:; \
         connect-src {connect}; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        script = empreinte_bloc(html, "<script>", "</script>"),
    )
}

/// Une page HTML avec sa politique.
fn page(html: String, csp: String) -> Response {
    Response::new()
        .header("content-security-policy", csp)
        .html(html)
}

// ---------------------------------------------------------------------------
// Partage de position en direct : l'utilisateur crée un lien, l'envoie à un
// proche qui l'ouvre dans un navigateur (sans l'app) et suit sa position en
// temps réel. Stocké en mémoire, éphémère (TTL), jeton non devinable.
// ---------------------------------------------------------------------------

/// Un partage de position actif.
#[derive(Clone, Serialize)]
struct LiveShare {
    name: String,
    lat: Option<f64>,
    lon: Option<f64>,
    heading: f64,
    speed: f64,
    /// Horodatage (s) de la dernière position.
    updated: i64,
    /// Expiration (s epoch) ; non sérialisé.
    #[serde(skip)]
    expires: i64,
    /// Le secret du propriétaire, seul autorisé à déplacer ou arrêter le
    /// partage ; jamais montré à qui regarde.
    #[serde(skip)]
    secret: String,
}

type LiveMap = Mutex<HashMap<String, LiveShare>>;

fn live_map() -> &'static LiveMap {
    static LIVE: OnceLock<LiveMap> = OnceLock::new();
    LIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Durée de vie d'un partage sans mise à jour (prolongée à chaque position).
const LIVE_TTL_SECS: i64 = 3 * 3600;
/// Nombre de partages simultanés au plus, par instance.
const LIVE_MAX: usize = 20_000;

/// Faut-il refuser une mise à jour **sans** secret ?
///
/// Pas encore : les versions de l'app iOS déjà installées ne l'envoient pas,
/// et seraient coupées net. En attendant, une mise à jour sans secret passe
/// avec un avertissement au journal ; un secret **faux**, lui, est refusé.
///
/// À FAIRE : passer à `true` quand toutes les versions de l'app en service
/// envoient `x-live-secret` (le partage reçu de `POST /live` en donne un) —
/// alors seul le propriétaire pourra déplacer ou arrêter un partage, et plus
/// quiconque a reçu le lien de suivi.
const SECRET_EXIGE: bool = false;

/// Le verdict sur le secret fourni avec une mise à jour.
#[derive(Debug, PartialEq, Eq)]
enum Controle {
    /// Le bon secret.
    Accepte,
    /// Pas de secret : toléré pendant la transition (voir [`SECRET_EXIGE`]).
    Tolere,
    /// Un mauvais secret, ou aucun quand il est exigé.
    Refuse,
}

/// Comparaison en temps constant (à longueur égale), pour ne pas laisser
/// deviner le secret caractère par caractère.
fn egaux(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn controle_secret(attendu: &str, fourni: Option<&str>, exige: bool) -> Controle {
    match fourni {
        Some(s) if egaux(s, attendu) => Controle::Accepte,
        Some(_) => Controle::Refuse,
        None if exige => Controle::Refuse,
        None => Controle::Tolere,
    }
}

/// Le secret envoyé : en-tête `x-live-secret`, sinon `?secret=`.
fn secret_fourni(req: &Request) -> Option<String> {
    req.header("x-live-secret")
        .map(str::to_owned)
        .or_else(|| req.query("secret").map(|s| s.into_owned()))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Vérifie le secret d'un partage ; `Err` = 403.
fn verifie_secret(partage: &LiveShare, req: &Request) -> Resultat<()> {
    match controle_secret(&partage.secret, secret_fourni(req).as_deref(), SECRET_EXIGE) {
        Controle::Accepte => Ok(()),
        Controle::Tolere => {
            tracing::warn!(
                route = %req.path(),
                "partage en direct modifié sans secret (ancienne version de l'app ?)"
            );
            Ok(())
        }
        Controle::Refuse => Err(Erreur::Statut(
            StatusCode::FORBIDDEN,
            "secret du partage invalide".into(),
        )),
    }
}

#[derive(Deserialize)]
struct LiveStartReq {
    name: Option<String>,
}

#[derive(Serialize)]
struct LiveStartResp {
    token: String,
    /// À renvoyer (`x-live-secret`) pour déplacer ou arrêter le partage.
    secret: String,
}

/// POST /live — crée un partage et renvoie son jeton et son secret.
async fn cree_partage(req: Request) -> Resultat<Response> {
    let corps: LiveStartReq = corps_json(&req).await?;
    let name = corps
        .name
        .map(|n| nettoie_label(&n))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Un ami".to_owned());
    let now = maintenant();
    let token = jeton_aleatoire();
    let secret = jeton_aleatoire();
    {
        let mut g = live_map().lock().map_err(Erreur::interne)?;
        g.retain(|_, s| s.expires > now); // purge des partages expirés
        if g.len() >= LIVE_MAX {
            return Err(Erreur::interne("trop de partages en direct"));
        }
        g.insert(
            token.clone(),
            LiveShare {
                name,
                lat: None,
                lon: None,
                heading: 0.0,
                speed: 0.0,
                updated: now,
                expires: now + LIVE_TTL_SECS,
                secret: secret.clone(),
            },
        );
    }
    Ok(Response::new().json(LiveStartResp { token, secret }))
}

#[derive(Deserialize)]
struct LivePos {
    lat: f64,
    lon: f64,
    heading: Option<f64>,
    speed: Option<f64>,
}

/// POST /live/:token — met à jour la position (depuis l'app qui partage).
async fn met_a_jour(req: Request) -> Resultat<Response> {
    let p: LivePos = corps_json(&req).await?;
    if !coord_valide(p.lat, p.lon) {
        return Ok(texte(StatusCode::BAD_REQUEST, ""));
    }
    let token = req.param("token").unwrap_or_default();
    let now = maintenant();
    let mut g = live_map().lock().map_err(Erreur::interne)?;
    match g.get_mut(token) {
        Some(s) if s.expires > now => {
            verifie_secret(s, &req)?;
            s.lat = Some(p.lat);
            s.lon = Some(p.lon);
            s.heading = p.heading.unwrap_or(0.0);
            s.speed = p.speed.unwrap_or(0.0);
            s.updated = now;
            s.expires = now + LIVE_TTL_SECS;
            Ok(Response::new())
        }
        _ => Ok(texte(StatusCode::NOT_FOUND, "")),
    }
}

/// DELETE /live/:token — arrête le partage. 200 même s'il n'existe plus.
async fn arrete(req: Request) -> Resultat<Response> {
    let token = req.param("token").unwrap_or_default();
    let mut g = live_map().lock().map_err(Erreur::interne)?;
    if let Some(s) = g.get(token) {
        verifie_secret(s, &req)?;
        g.remove(token);
    }
    Ok(Response::new())
}

/// GET /live/:token/pos — dernière position (JSON), pour la page de suivi.
async fn position(req: Request) -> Resultat<Response> {
    let token = req.param("token").unwrap_or_default();
    let now = maintenant();
    let g = live_map().lock().map_err(Erreur::interne)?;
    Ok(match g.get(token) {
        Some(s) if s.expires > now => Response::new().json(s),
        _ => texte(StatusCode::NOT_FOUND, "expired"),
    })
}

/// GET /live/:token — page web de suivi (carte Leaflet qui interroge /pos).
async fn page_suivi(req: Request) -> Response {
    let token = req.param("token").unwrap_or_default();
    // Le jeton est injecté dans le HTML/JS : on n'accepte que de l'hexadécimal
    // (généré par jeton_aleatoire) pour écarter toute injection.
    if token.len() > 64 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return texte(StatusCode::NOT_FOUND, "");
    }
    let html = live_html(token);
    let csp = csp_carte(&html, "'self'");
    page(html, csp)
}

/// Page HTML autonome de suivi (Leaflet + OpenStreetMap, sans clé).
fn live_html(token: &str) -> String {
    format!(
        r#"<!doctype html><html lang="fr"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Position en direct — MonCap GPS</title>
<link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css">
<style>
  html,body{{margin:0;height:100%;font-family:-apple-system,system-ui,sans-serif}}
  #map{{position:absolute;inset:0}}
  #bar{{position:absolute;z-index:1000;left:12px;top:12px;right:12px;background:#fff;
    border-radius:14px;padding:10px 14px;box-shadow:0 2px 10px rgba(0,0,0,.15);
    display:flex;align-items:center;gap:10px}}
  #dot{{width:12px;height:12px;border-radius:50%;background:#2a7;flex:none;
    animation:p 1.4s infinite}}
  @keyframes p{{0%,100%{{opacity:1}}50%{{opacity:.3}}}}
  #name{{font-weight:700}} #sub{{color:#777;font-size:13px}}
</style></head><body>
<div id="bar"><div id="dot"></div><div><div id="name">Position en direct</div>
<div id="sub">Connexion…</div></div></div>
<div id="map"></div>
<script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
<script>
var token={token:?};
var map=L.map('map').setView([46.6,2.4],6);
L.tileLayer('https://tile.openstreetmap.org/{{z}}/{{x}}/{{y}}.png',
  {{maxZoom:19,attribution:'© OpenStreetMap'}}).addTo(map);
var marker=null,first=true;
function ago(t){{var d=Math.max(0,Math.floor(Date.now()/1000-t));
  if(d<60)return 'il y a '+d+' s'; if(d<3600)return 'il y a '+Math.floor(d/60)+' min';
  return 'il y a '+Math.floor(d/3600)+' h';}}
function tick(){{
  fetch('/live/'+token+'/pos').then(function(r){{
    if(!r.ok)throw 0; return r.json();}}).then(function(s){{
    document.getElementById('name').textContent=s.name||'Position en direct';
    if(s.lat==null){{document.getElementById('sub').textContent='En attente de position…';return;}}
    var ll=[s.lat,s.lon];
    if(!marker){{marker=L.marker(ll).addTo(map);}} else {{marker.setLatLng(ll);}}
    if(first){{map.setView(ll,14);first=false;}} else {{map.panTo(ll);}}
    var kmh=Math.round(s.speed||0);
    document.getElementById('sub').textContent=kmh+' km/h · '+ago(s.updated);
  }}).catch(function(){{
    document.getElementById('sub').textContent='Partage terminé';
    document.getElementById('dot').style.background='#c33';
  }});
}}
tick(); setInterval(tick,4000);
</script></body></html>"#,
        token = token
    )
}

// ---------------------------------------------------------------------------
// Itinéraire partagé.
// ---------------------------------------------------------------------------

/// Un « lat,lon » venu d'un lien de partage : deux nombres finis dans les
/// bornes du globe, arrondis à 1e-5. Rien d'autre n'entre dans la page — les
/// coordonnées rendues sont reformatées depuis ces nombres, jamais recopiées
/// du texte reçu.
fn partage_coord(brut: &str) -> Option<(f64, f64)> {
    let mut morceaux = brut.split(',');
    let lat: f64 = morceaux.next()?.trim().parse().ok()?;
    let lon: f64 = morceaux.next()?.trim().parse().ok()?;
    if morceaux.next().is_some() || !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    Some(((lat * 1e5).round() / 1e5, (lon * 1e5).round() / 1e5))
}

/// Texte venu d'un lien, prêt pour du HTML : échappé, borné à soixante
/// caractères, sans caractères de contrôle.
fn partage_echappe(brut: &str) -> String {
    let mut s = String::with_capacity(brut.len());
    for c in brut.chars().take(60) {
        match c {
            '&' => s.push_str("&amp;"),
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            '"' => s.push_str("&quot;"),
            '\'' => s.push_str("&#39;"),
            c if c.is_control() => {}
            c => s.push(c),
        }
    }
    s
}

/// Valeur encodée pour la partie requête d'un lien (RFC 3986 strict : seuls
/// les non-réservés passent tels quels).
fn partage_encode(brut: &str) -> String {
    let mut s = String::with_capacity(brut.len());
    for b in brut.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                s.push(b as char)
            }
            _ => s.push_str(&format!("%{b:02X}")),
        }
    }
    s
}

#[derive(Deserialize)]
struct PartageQuery {
    /// Destination « lat,lon » — le seul champ obligatoire.
    a: String,
    #[serde(default)]
    nom: String,
    #[serde(default)]
    mode: String,
    /// Étapes « lat,lon|lat,lon », cinq au plus.
    #[serde(default)]
    v: String,
}

/// GET /t — la page d'un itinéraire partagé depuis l'app.
///
/// Aucun stockage : le lien porte tout (destination, nom, mode, étapes), la
/// page ne fait que le rendre — une carte, le nom, et le lien profond
/// `moncap://` qui ouvre l'app avec le même trajet. Qui n'a pas l'app a les
/// liens Plans et Google Maps en dessous.
async fn trajet_partage(req: Request) -> Resultat<Response> {
    let q: PartageQuery = requete_query(&req)?;
    let Some((lat, lon)) = partage_coord(&q.a) else {
        return Err(Erreur::requete("destination invalide"));
    };
    let mode = match q.mode.as_str() {
        m @ ("walk" | "bike" | "transit") => m,
        _ => "car",
    };
    let mut vias: Vec<(f64, f64)> = Vec::new();
    for morceau in q.v.split('|').take(5) {
        if morceau.is_empty() {
            continue;
        }
        let Some(p) = partage_coord(morceau) else {
            return Err(Erreur::requete("étape invalide"));
        };
        vias.push(p);
    }

    let nom = q.nom.trim();
    let nom_html = partage_echappe(nom);
    let titre = if nom_html.is_empty() {
        "Itinéraire partagé".to_string()
    } else {
        nom_html.clone()
    };
    let mode_libelle = match mode {
        "walk" => "🚶 À pied",
        "bike" => "🚴 À vélo",
        "transit" => "🚌 En transports",
        _ => "🚗 En voiture",
    };

    // Le lien profond est reconstruit depuis les valeurs validées, jamais
    // recopié de la requête. Les étapes voyagent encodées : un « | » brut
    // fait échouer l'analyse d'URL côté iOS.
    let mut lien = format!("moncap://trajet?a={lat},{lon}&mode={mode}");
    if !vias.is_empty() {
        let liste = vias
            .iter()
            .map(|(la, lo)| format!("{la},{lo}"))
            .collect::<Vec<_>>()
            .join("|");
        lien.push_str(&format!("&v={}", partage_encode(&liste)));
    }
    if !nom.is_empty() {
        lien.push_str(&format!("&nom={}", partage_encode(nom)));
    }

    let jalons_js = vias
        .iter()
        .map(|(la, lo)| {
            format!("L.circleMarker([{la},{lo}],{{radius:6,color:'#63c'}}).addTo(map);")
        })
        .collect::<Vec<_>>()
        .join("");

    let html = format!(
        r#"<!doctype html><html lang="fr"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{titre} — MonCap GPS</title>
<link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css">
<style>
  html,body{{margin:0;height:100%;font-family:-apple-system,system-ui,sans-serif}}
  #map{{position:absolute;inset:0}}
  #bar{{position:absolute;z-index:1000;left:12px;top:12px;right:12px;background:#fff;
    border-radius:14px;padding:12px 14px;box-shadow:0 2px 10px rgba(0,0,0,.15)}}
  #nom{{font-weight:700}} #sub{{color:#777;font-size:13px;margin-top:2px}}
  #ouvre{{display:block;margin-top:10px;text-align:center;background:#2a7;color:#fff;
    text-decoration:none;font-weight:700;border-radius:10px;padding:10px}}
  #autres{{margin-top:8px;font-size:13px;text-align:center}}
  #autres a{{color:#57c;margin:0 6px}}
</style></head><body>
<div id="bar"><div id="nom">{titre}</div><div id="sub">{mode_libelle}</div>
<a id="ouvre" href="{lien}">Ouvrir dans MonCap GPS</a>
<div id="autres"><a href="https://maps.apple.com/?daddr={lat},{lon}">Plans</a>
<a href="https://www.google.com/maps/dir/?api=1&amp;destination={lat},{lon}">Google Maps</a></div></div>
<div id="map"></div>
<script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
<script>
var map=L.map('map').setView([{lat},{lon}],12);
L.tileLayer('https://tile.openstreetmap.org/{{z}}/{{x}}/{{y}}.png',
  {{maxZoom:19,attribution:'© OpenStreetMap'}}).addTo(map);
L.marker([{lat},{lon}]).addTo(map);{jalons_js}
</script></body></html>"#
    );
    let csp = csp_carte(&html, "'none'");
    Ok(page(html, csp))
}

// ---------------------------------------------------------------------------
// Politique de confidentialité.
// ---------------------------------------------------------------------------

/// GET /privacy — politique de confidentialité (publique, exigée pour la
/// distribution App Store).
async fn confidentialite(_req: Request) -> Response {
    static CSP: OnceLock<String> = OnceLock::new();
    let csp = CSP.get_or_init(|| {
        format!(
            "default-src 'none'; style-src {}; frame-ancestors 'none'; base-uri 'none'; \
             form-action 'none'",
            empreinte_bloc(PRIVACY_HTML, "<style>", "</style>")
        )
    });
    Response::new()
        .header("content-security-policy", csp.as_str())
        .html(PRIVACY_HTML)
}

const PRIVACY_HTML: &str = r#"<!doctype html>
<html lang="fr"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>MonCap GPS — Politique de confidentialité</title>
<style>body{font-family:-apple-system,sans-serif;max-width:720px;margin:2rem auto;padding:0 1rem;line-height:1.6;color:#1a1d21}h1{font-size:1.5rem}h2{font-size:1.1rem;margin-top:1.5rem}</style>
</head><body>
<h1>🛰️ MonCap GPS — Politique de confidentialité</h1>
<h2>Données collectées</h2>
<p>MonCap GPS stocke : votre adresse e-mail de connexion, un mot de passe haché
(Argon2, jamais en clair), vos positions enregistrées, vos trajets parcourus,
vos recherches récentes et vos signalements (avec votes).</p>
<h2>Position en temps réel</h2>
<p>Pendant l'utilisation, votre position et le nom associé à votre compte sont
diffusés en direct aux autres utilisateurs connectés. Cette diffusion n'est pas
conservée. Le bouton « Stop partage » l'interrompt à tout moment.</p>
<h2>Services tiers</h2>
<p>Le calcul d'itinéraires et la recherche de lieux utilisent Apple Plans ;
le vélo utilise BRouter (OpenStreetMap) ; le dénivelé utilise open-meteo ;
les limitations de vitesse utilisent Overpass (OpenStreetMap). Seules des
coordonnées géographiques leur sont transmises, jamais votre identité.</p>
<h2>Conservation et suppression</h2>
<p>Vos données sont conservées tant que votre compte existe. Vous pouvez
supprimer définitivement votre compte et l'ensemble de vos données depuis
l'application : menu ▸ Compte ▸ « Supprimer mon compte ».</p>
<h2>Partage</h2>
<p>Vos données ne sont ni vendues ni transmises à des tiers.</p>
</body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use paquets::test::TestClient;
    use serde_json::{Value, json};

    /// Les routes du module, sans base ni bouclier.
    fn client() -> TestClient {
        let mut app = vitesse::App::new();
        enregistre(&mut app);
        TestClient::new(app)
    }

    #[test]
    fn partage_coord_borne_le_globe() {
        assert_eq!(partage_coord("48.8566,2.3522"), Some((48.8566, 2.3522)));
        assert_eq!(partage_coord(" 44.5 , -1.2 "), Some((44.5, -1.2)));
        // Arrondi à 1e-5 : le lien profond reste court et stable.
        assert_eq!(
            partage_coord("44.123456789,-1.20000049"),
            Some((44.12346, -1.2))
        );
        assert_eq!(partage_coord("91,0"), None);
        assert_eq!(partage_coord("0,181"), None);
        assert_eq!(partage_coord("48.85,2.35,3"), None);
        assert_eq!(partage_coord("abc"), None);
        assert_eq!(partage_coord("NaN,2"), None);
        assert_eq!(partage_coord(""), None);
    }

    #[test]
    fn partage_echappe_neutralise_le_html() {
        assert_eq!(partage_echappe(r#"<b>&"'"#), "&lt;b&gt;&amp;&quot;&#39;");
        // Borné à soixante caractères : un nom de lieu, pas un roman.
        assert_eq!(partage_echappe(&"a".repeat(100)).chars().count(), 60);
        assert_eq!(partage_echappe("Chez\u{7}Paul"), "ChezPaul");
    }

    #[test]
    fn partage_encode_reserve_tout() {
        assert_eq!(
            partage_encode("chez Paul & fils"),
            "chez%20Paul%20%26%20fils"
        );
        assert_eq!(
            partage_encode("44.5,-1.2|44.6,-1.1"),
            "44.5%2C-1.2%7C44.6%2C-1.1"
        );
        assert_eq!(partage_encode("été"), "%C3%A9t%C3%A9");
    }

    #[test]
    fn base64_des_vecteurs_connus() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // SHA-256 de « abc », tel qu'un navigateur l'attend dans une CSP.
        assert_eq!(
            base64(&Sha256::digest(b"abc")),
            "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
        );
    }

    #[test]
    fn le_secret_se_controle() {
        assert_eq!(controle_secret("abc", Some("abc"), true), Controle::Accepte);
        assert_eq!(controle_secret("abc", Some("abd"), false), Controle::Refuse);
        assert_eq!(controle_secret("abc", Some("ab"), false), Controle::Refuse);
        assert_eq!(controle_secret("abc", None, false), Controle::Tolere);
        assert_eq!(controle_secret("abc", None, true), Controle::Refuse);
    }

    /// Le script en ligne d'une page doit être celui que la CSP autorise.
    fn script_autorise(html: &str, csp: &str) {
        let (_, reste) = html.split_once("<script>").expect("script en ligne");
        let (script, _) = reste.split_once("</script>").unwrap();
        let empreinte = format!("'sha256-{}'", base64(&Sha256::digest(script.as_bytes())));
        assert!(csp.contains(&empreinte), "{csp}");
        assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
        assert!(csp.contains("https://unpkg.com/leaflet@1.9.4/"), "{csp}");
        assert!(csp.contains("https://tile.openstreetmap.org"), "{csp}");
    }

    #[tokio::test]
    async fn le_partage_en_direct_de_bout_en_bout() {
        let c = client();
        let r = c.post("/live").json(&json!({"name": "  Paul  "})).await;
        assert_eq!(r.status(), 200, "{}", r.text());
        let v: Value = r.json();
        let token = v["token"].as_str().unwrap().to_owned();
        let secret = v["secret"].as_str().unwrap().to_owned();
        assert_eq!(token.len(), 32);
        assert_eq!(secret.len(), 32);
        assert_ne!(token, secret);

        // Pas encore de position.
        let r = c.get(&format!("/live/{token}/pos")).await;
        assert_eq!(r.status(), 200);
        let v: Value = r.json();
        assert_eq!(v["name"], "Paul");
        assert!(v["lat"].is_null());
        assert_eq!(v["heading"], 0.0);
        assert!(v.get("secret").is_none() && v.get("expires").is_none());

        // Le propriétaire, avec son secret (en-tête ou paramètre).
        let r = c
            .post(&format!("/live/{token}"))
            .header("x-live-secret", &secret)
            .json(&json!({"lat": 48.85, "lon": 2.35, "speed": 30.0}))
            .await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.text(), "");
        let v: Value = c.get(&format!("/live/{token}/pos")).await.json();
        assert_eq!(v["lat"], 48.85);
        assert_eq!(v["speed"], 30.0);
        assert_eq!(v["heading"], 0.0);
        let r = c
            .post(&format!("/live/{token}?secret={secret}"))
            .json(&json!({"lat": 48.86, "lon": 2.36, "heading": 90.0}))
            .await;
        assert_eq!(r.status(), 200);

        // Un mauvais secret est refusé, sans rien changer.
        let r = c
            .post(&format!("/live/{token}"))
            .header("x-live-secret", "0123456789abcdef0123456789abcdef")
            .json(&json!({"lat": 1.0, "lon": 1.0}))
            .await;
        assert_eq!(r.status(), 403);
        let r = c
            .delete(&format!("/live/{token}"))
            .header("x-live-secret", "faux")
            .await;
        assert_eq!(r.status(), 403);
        let v: Value = c.get(&format!("/live/{token}/pos")).await.json();
        assert_eq!(v["lat"], 48.86);

        // Les anciennes versions de l'app n'envoient pas de secret : toléré.
        let r = c
            .post(&format!("/live/{token}"))
            .json(&json!({"lat": 45.0, "lon": 5.0}))
            .await;
        assert_eq!(r.status(), 200);

        // Coordonnées hors du globe : 400 vide.
        let r = c
            .post(&format!("/live/{token}"))
            .json(&json!({"lat": 91.0, "lon": 0.0}))
            .await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.text(), "");

        // Arrêt, puis plus rien.
        let r = c
            .delete(&format!("/live/{token}"))
            .header("x-live-secret", &secret)
            .await;
        assert_eq!(r.status(), 200);
        let r = c.get(&format!("/live/{token}/pos")).await;
        assert_eq!(r.status(), 404);
        assert_eq!(r.text(), "expired");
        let r = c
            .post(&format!("/live/{token}"))
            .json(&json!({"lat": 45.0, "lon": 5.0}))
            .await;
        assert_eq!(r.status(), 404);
        // Arrêter un partage disparu reste un succès.
        assert_eq!(c.delete(&format!("/live/{token}")).await.status(), 200);
    }

    #[tokio::test]
    async fn un_partage_sans_nom_est_un_ami() {
        let c = client();
        let v: Value = c.post("/live").json(&json!({})).await.json();
        let token = v["token"].as_str().unwrap();
        let v: Value = c.get(&format!("/live/{token}/pos")).await.json();
        assert_eq!(v["name"], "Un ami");
        let v: Value = c.post("/live").json(&json!({"name": "   "})).await.json();
        let token = v["token"].as_str().unwrap();
        let v: Value = c.get(&format!("/live/{token}/pos")).await.json();
        assert_eq!(v["name"], "Un ami");
        // Sans corps JSON : 415.
        assert_eq!(c.post("/live").await.status(), 415);
    }

    #[tokio::test]
    async fn la_page_de_suivi_n_accepte_que_de_l_hexadecimal() {
        let c = client();
        let r = c.get("/live/abc123").await;
        assert_eq!(r.status(), 200);
        let html = r.text();
        assert!(html.contains(r#"var token="abc123";"#));
        script_autorise(&html, r.header("content-security-policy").unwrap());
        assert!(
            r.header("content-security-policy")
                .unwrap()
                .contains("connect-src 'self'")
        );
        assert_eq!(c.get("/live/abc%3Cscript").await.status(), 404);
        assert_eq!(c.get("/live/xyz").await.status(), 404);
        assert_eq!(
            c.get(&format!("/live/{}", "a".repeat(65))).await.status(),
            404
        );
    }

    #[tokio::test]
    async fn le_trajet_partage_rend_une_page_sure() {
        let c = client();
        let r = c
            .get("/t?a=44.5,-1.2&nom=Chez%20%3CPaul%3E&mode=bike&v=44.6,-1.1%7C%7C44.7,-1.0")
            .await;
        assert_eq!(r.status(), 200);
        let html = r.text();
        assert!(html.contains("<title>Chez &lt;Paul&gt; — MonCap GPS</title>"));
        assert!(html.contains("🚴 À vélo"));
        assert!(html.contains(
            r#"href="moncap://trajet?a=44.5,-1.2&mode=bike&v=44.6%2C-1.1%7C44.7%2C-1&nom=Chez%20%3CPaul%3E""#
        ));
        assert!(html.contains("https://maps.apple.com/?daddr=44.5,-1.2"));
        assert!(html.contains("L.circleMarker([44.7,-1],"));
        script_autorise(&html, r.header("content-security-policy").unwrap());

        // Sans nom, le titre par défaut ; un mode inconnu devient la voiture.
        let r = c.get("/t?a=48.85,2.35&mode=fusee").await;
        let html = r.text();
        assert!(html.contains("<title>Itinéraire partagé — MonCap GPS</title>"));
        assert!(html.contains("moncap://trajet?a=48.85,2.35&mode=car\""));
        assert!(html.contains("🚗 En voiture"));

        let r = c.get("/t?a=91,0").await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.text(), "destination invalide");
        let r = c.get("/t?a=48,2&v=1,2%7Cabc").await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.text(), "étape invalide");
        assert_eq!(c.get("/t").await.status(), 400);
    }

    #[tokio::test]
    async fn la_confidentialite_a_sa_politique() {
        let c = client();
        let r = c.get("/privacy").await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.header("content-type"), Some("text/html; charset=utf-8"));
        assert_eq!(r.text(), PRIVACY_HTML);
        let csp = r.header("content-security-policy").unwrap();
        let (_, reste) = PRIVACY_HTML.split_once("<style>").unwrap();
        let (style, _) = reste.split_once("</style>").unwrap();
        let empreinte = format!("'sha256-{}'", base64(&Sha256::digest(style.as_bytes())));
        assert!(csp.contains(&format!("style-src {empreinte}")), "{csp}");
        assert!(csp.contains("default-src 'none'"));
        assert!(csp.contains("frame-ancestors 'none'"));
    }
}
