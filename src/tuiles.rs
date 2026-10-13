//! Les tuiles de carte (cartes hors ligne de l'app) : la feuille de style
//! MapLibre, le relais vers un serveur amont avec cache mémoire, et la lecture
//! d'une archive PMTiles auto-hébergée.
//!
//! Tout se règle par l'environnement (`TILES_PMTILES`, `TILES_UPSTREAM`,
//! `TILES_GLYPHS`, `TILES_USER_AGENT`), lu dans un [`ConfigTuiles`] que les
//! handlers reçoivent : les essais passent une configuration explicite au lieu
//! de modifier l'environnement du processus, partagé entre essais parallèles.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use paquets::prelude::*;
use serde_json::{Value, json};

use crate::erreur::{Resultat, param, texte};
use crate::pmtiles;

/// Zoom maximal servi. Au-delà, une zone téléchargée pèserait très lourd et
/// la charge amont exploserait.
const TILES_MAX_ZOOM: u32 = 17;
/// Nombre de tuiles gardées en mémoire (~18 Ko pièce, soit ~27 Mo au plus).
const TILES_CACHE_MAX: usize = 1500;
/// Nombre de répertoires feuilles PMTiles gardés en mémoire. Une feuille
/// pèse quelques dizaines de Ko décodée : borner évite qu'un parcours de
/// toute la planète ne fasse gonfler la mémoire sans fin.
const FEUILLES_MAX: usize = 4096;
/// Cache client des tuiles : leur rendu ne change qu'exceptionnellement.
const CACHE_TUILE: &str = "public, max-age=2592000, immutable";
/// L'agent par défaut des requêtes sortantes.
const AGENT_DEFAUT: &str = "MonCapGPS/1.0 (+https://github.com/maxlestage/moncap)";

pub fn routes(app: &mut App) {
    app.get("/tiles/style.json", |req: Request| async move {
        style(&req, &ConfigTuiles::depuis_env())
    });
    app.get("/tiles/:z/:x/:y", |req: Request| async move {
        tuile(&req, &ConfigTuiles::depuis_env()).await
    });
}

// ---------------------------------------------------------------------------
// Configuration.
// ---------------------------------------------------------------------------

/// Les réglages des tuiles, déjà filtrés : une valeur invalide vaut absente.
#[derive(Clone, Debug, Default)]
pub struct ConfigTuiles {
    /// Archive PMTiles auto-hébergée (`TILES_PMTILES`), interrogée par plages
    /// d'octets. Prioritaire sur le relais : c'est le mode conforme, où l'on
    /// sert ses propres tuiles issues d'OpenStreetMap. Ne vaut que si elle
    /// commence par `http`.
    pub pmtiles: Option<String>,
    /// Serveur amont (`TILES_UPSTREAM`), au format
    /// `https://exemple.fr/{z}/{x}/{y}.png`. La clé éventuelle du fournisseur
    /// reste ainsi sur le serveur, jamais dans l'app. Ne vaut qu'avec `{z}`,
    /// `{x}` et `{y}`.
    pub amont: Option<String>,
    /// Polices matricielles (`TILES_GLYPHS`) : sans elles, pas de libellés.
    pub glyphes: Option<String>,
    /// En-tête `User-Agent` des requêtes sortantes (`TILES_USER_AGENT`). La
    /// plupart des fournisseurs — OpenStreetMap en tête — refusent les
    /// requêtes sans agent identifiable.
    pub agent: String,
}

impl ConfigTuiles {
    /// Lit l'environnement.
    pub fn depuis_env() -> Self {
        let var = |nom: &str| std::env::var(nom).ok();
        ConfigTuiles::depuis(
            var("TILES_PMTILES"),
            var("TILES_UPSTREAM"),
            var("TILES_GLYPHS"),
            var("TILES_USER_AGENT"),
        )
    }

    /// Construit la configuration depuis des valeurs brutes, avec les mêmes
    /// filtres que l'environnement.
    pub fn depuis(
        pmtiles: Option<String>,
        amont: Option<String>,
        glyphes: Option<String>,
        agent: Option<String>,
    ) -> Self {
        ConfigTuiles {
            pmtiles: pmtiles
                .map(|v| v.trim().to_owned())
                .filter(|v| v.starts_with("http")),
            amont: amont
                .map(|v| v.trim().to_owned())
                .filter(|v| v.contains("{z}") && v.contains("{x}") && v.contains("{y}")),
            glyphes: glyphes.filter(|v| !v.is_empty()),
            agent: agent
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| AGENT_DEFAUT.to_owned()),
        }
    }

    /// Une source au moins : l'archive ou l'amont.
    fn configuree(&self) -> bool {
        self.pmtiles.is_some() || self.amont.is_some()
    }
}

/// Client HTTP sortant des tuiles (pool de connexions et pile TLS construits
/// une seule fois), 15 s au plus par requête.
fn client_http() -> &'static reqwest::Client {
    static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
    HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default()
    })
}

// ---------------------------------------------------------------------------
// Feuille de style.
// ---------------------------------------------------------------------------

/// La feuille de style MapLibre, pointant sur `{scheme}://{host}/tiles/…`.
///
/// Les tuiles vectorielles ne sont que des données : c'est le style qui décide
/// des couleurs et de l'épaisseur des traits. On en sert un minimal, au schéma
/// OpenMapTiles (celui que produit `planetiler`), pointant sur notre propre
/// endpoint de tuiles.
///
/// Les libellés sont **omis par défaut** : ils exigent des polices matricielles
/// (« glyphs ») à héberger séparément. Renseigner `TILES_GLYPHS` les active.
pub fn style_json(host: &str, glyphes: Option<&str>) -> Value {
    let scheme = if host.starts_with("localhost") || host.starts_with("127.") {
        "http"
    } else {
        "https"
    };
    let tiles = format!("{scheme}://{host}/tiles/{{z}}/{{x}}/{{y}}");

    let mut style = json!({
        "version": 8,
        "name": "MonCap hors ligne",
        "sources": {
            "osm": {
                "type": "vector",
                "tiles": [tiles],
                "minzoom": 0,
                "maxzoom": 14,
                "attribution": "© OpenStreetMap contributors"
            }
        },
        "layers": [
            { "id": "fond", "type": "background",
              "paint": { "background-color": "#f2efe9" } },
            { "id": "eau", "type": "fill", "source": "osm", "source-layer": "water",
              "paint": { "fill-color": "#a8c8e8" } },
            { "id": "espaces-verts", "type": "fill", "source": "osm",
              "source-layer": "landcover",
              "paint": { "fill-color": "#cde6c0", "fill-opacity": 0.6 } },
            { "id": "batiments", "type": "fill", "source": "osm",
              "source-layer": "building", "minzoom": 13.0,
              "paint": { "fill-color": "#e0dcd4" } },
            // Les petites routes n'apparaissent qu'en zoomant : sinon la carte
            // devient illisible et le rendu s'alourdit pour rien.
            { "id": "routes-secondaires", "type": "line", "source": "osm",
              "source-layer": "transportation", "minzoom": 11.0,
              "paint": { "line-color": "#ffffff", "line-width": 2.0 } },
            { "id": "routes-principales", "type": "line", "source": "osm",
              "source-layer": "transportation",
              "filter": ["in", "class", "motorway", "trunk", "primary"],
              "paint": { "line-color": "#f6c05a", "line-width": 3.5 } }
        ]
    });

    if let Some(glyphs) = glyphes {
        style["glyphs"] = Value::String(glyphs.to_owned());
        if let Some(layers) = style["layers"].as_array_mut() {
            layers.push(json!({
                "id": "noms-lieux", "type": "symbol", "source": "osm",
                "source-layer": "place", "minzoom": 8.0,
                "layout": {
                    "text-field": ["get", "name"],
                    "text-font": ["Noto Sans Regular"],
                    "text-size": 13.0
                },
                "paint": { "text-color": "#33322f", "text-halo-color": "#ffffff",
                           "text-halo-width": 1.2 }
            }));
        }
    }
    style
}

/// GET /tiles/style.json — la feuille de style des tuiles servies.
fn style(req: &Request, cfg: &ConfigTuiles) -> Response {
    if !cfg.configuree() {
        return texte(StatusCode::NOT_FOUND, "tuiles non configurées");
    }
    // Adresse publique déduite de la requête : le style doit pointer sur le
    // même hôte que celui par lequel l'app nous a joints.
    let host = req.host().unwrap_or("localhost:3000");
    Response::new().json(style_json(host, cfg.glyphes.as_deref()))
}

// ---------------------------------------------------------------------------
// Tuiles.
// ---------------------------------------------------------------------------

/// Les coordonnées d'une tuile, validées : `y` arrive avec son extension
/// éventuelle (« 12.png ») et on n'en garde que le nombre, l'amont décidant
/// du format. Bornes strictes : sans elles, n'importe qui pourrait faire
/// relayer des requêtes arbitraires et saturer l'amont.
fn coordonnees(z: u32, x: u32, y: &str) -> Result<(u32, u32, u32), &'static str> {
    let y_num = y.split('.').next().unwrap_or_default();
    let Ok(y) = y_num.parse::<u32>() else {
        return Err("tuile invalide");
    };
    if z > TILES_MAX_ZOOM {
        return Err("zoom trop élevé");
    }
    let limit = 1u32 << z;
    if x >= limit || y >= limit {
        return Err("tuile hors limites");
    }
    Ok((z, x, y))
}

/// GET /tiles/:z/:x/:y — une tuile, depuis l'archive ou le relais.
///
/// **La licence applicable est celle de l'amont** : ce relais ne rend pas
/// légitime une source qui interdirait le téléchargement de zones.
async fn tuile(req: &Request, cfg: &ConfigTuiles) -> Resultat<Response> {
    let z: u32 = param(req, "z")?;
    let x: u32 = param(req, "x")?;
    // Non configuré : la fonction est simplement inactive, avant toute
    // validation.
    if !cfg.configuree() {
        return Ok(texte(StatusCode::NOT_FOUND, "tuiles non configurées"));
    }
    let (z, x, y) = match coordonnees(z, x, req.param("y").unwrap_or_default()) {
        Ok(c) => c,
        Err(message) => return Ok(texte(StatusCode::BAD_REQUEST, message)),
    };

    // Archive auto-hébergée d'abord : c'est le mode conforme.
    if let Some(url) = &cfg.pmtiles {
        let lecteur = LecteurHttp {
            url,
            agent: &cfg.agent,
        };
        return Ok(tuile_archive(archive_globale(), url, &lecteur, z, x, y).await);
    }
    let Some(amont) = &cfg.amont else {
        return Ok(texte(StatusCode::NOT_FOUND, "tuiles non configurées"));
    };
    Ok(tuile_amont(amont, &cfg.agent, z, x, y).await)
}

/// Réponse tuile : cache long côté client.
fn reponse_tuile(octets: Vec<u8>, mime: &str) -> Response {
    Response::new()
        .header("content-type", mime)
        .header("cache-control", CACHE_TUILE)
        .send(octets)
}

// ---------------------------------------------------------------------------
// Relais amont.
// ---------------------------------------------------------------------------

/// Cache borné, éviction du plus ancien inséré.
struct Fifo<K, V, const MAX: usize> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K, V, const MAX: usize> Default for Fifo<K, V, MAX> {
    fn default() -> Self {
        Fifo {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }
}

impl<K: Hash + Eq + Clone, V: Clone, const MAX: usize> Fifo<K, V, MAX> {
    fn get(&self, key: &K) -> Option<V> {
        self.map.get(key).cloned()
    }

    fn put(&mut self, key: K, value: V) {
        if self.map.insert(key.clone(), value).is_none() {
            self.order.push_back(key);
            while self.order.len() > MAX {
                if let Some(old) = self.order.pop_front() {
                    self.map.remove(&old);
                }
            }
        }
    }
}

/// Les tuiles déjà relayées : (z, x, y) → (octets, type MIME).
type TileCache = Fifo<(u32, u32, u32), (Vec<u8>, String), TILES_CACHE_MAX>;

/// Cache mémoire des tuiles déjà servies. Le disque de Heroku étant éphémère,
/// un cache mémoire borné est le meilleur compromis : il absorbe les
/// re-demandes d'une même session sans rien promettre de durable.
fn cache_tuiles() -> &'static Mutex<TileCache> {
    static CACHE: OnceLock<Mutex<TileCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(TileCache::default()))
}

/// Relaie une tuile depuis l'amont, avec le cache mémoire.
async fn tuile_amont(amont: &str, agent: &str, z: u32, x: u32, y: u32) -> Response {
    let key = (z, x, y);
    if let Some((octets, mime)) = cache_tuiles().lock().ok().and_then(|c| c.get(&key)) {
        return reponse_tuile(octets, &mime);
    }
    let url = amont
        .replace("{z}", &z.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string());
    let Ok(resp) = client_http()
        .get(&url)
        .header(reqwest::header::USER_AGENT, agent)
        .send()
        .await
    else {
        return texte(StatusCode::BAD_GATEWAY, "amont injoignable");
    };
    if !resp.status().is_success() {
        return texte(StatusCode::BAD_GATEWAY, "amont en erreur");
    }
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/png")
        .to_owned();
    let Ok(octets) = resp.bytes().await else {
        return texte(StatusCode::BAD_GATEWAY, "tuile illisible");
    };
    let octets = octets.to_vec();
    if let Ok(mut c) = cache_tuiles().lock() {
        c.put(key, (octets.clone(), mime.clone()));
    }
    reponse_tuile(octets, &mime)
}

// ---------------------------------------------------------------------------
// Archive PMTiles.
// ---------------------------------------------------------------------------

/// Qui sait lire une plage d'octets de l'archive : HTTP en production, un
/// tampon en mémoire dans les essais.
trait LecteurPlages {
    async fn plage(&self, offset: u64, length: u32) -> Option<Vec<u8>>;
}

/// L'archive servie par un hébergeur HTTP qui accepte les plages d'octets.
struct LecteurHttp<'a> {
    url: &'a str,
    agent: &'a str,
}

impl LecteurPlages for LecteurHttp<'_> {
    async fn plage(&self, offset: u64, length: u32) -> Option<Vec<u8>> {
        if length == 0 {
            return None;
        }
        let end = offset + length as u64 - 1;
        let resp = client_http()
            .get(self.url)
            .header(reqwest::header::RANGE, format!("bytes={offset}-{end}"))
            .header(reqwest::header::USER_AGENT, self.agent)
            .send()
            .await
            .ok()?;
        // 206 attendu ; un 200 signifierait que l'hébergeur ignore les plages
        // et renverrait toute l'archive — refusé, elle peut peser des
        // gigaoctets.
        if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            tracing::warn!("PMTiles : hébergeur sans support des plages d'octets");
            return None;
        }
        resp.bytes().await.ok().map(|b| b.to_vec())
    }
}

/// En-tête et répertoire racine, lus une seule fois : minuscules devant les
/// données de tuiles, et nécessaires à chaque requête.
struct PmArchive {
    header: pmtiles::Header,
    root: Vec<pmtiles::Entry>,
}

/// Répertoires feuilles déjà lus, repérés par (décalage, longueur).
type LeafCache = Fifo<(u64, u32), Arc<Vec<pmtiles::Entry>>, FEUILLES_MAX>;

/// Ce que l'on garde d'une archive entre deux requêtes.
#[derive(Default)]
struct EtatArchive {
    /// L'archive chargée, avec l'adresse d'où elle vient. Un échec n'est pas
    /// gardé : la requête suivante réessaie.
    archive: tokio::sync::Mutex<Option<(String, Arc<PmArchive>)>>,
    /// Les feuilles déjà lues, pour ne pas les redemander à chaque tuile.
    feuilles: Mutex<LeafCache>,
}

fn archive_globale() -> &'static EtatArchive {
    static ETAT: OnceLock<EtatArchive> = OnceLock::new();
    ETAT.get_or_init(EtatArchive::default)
}

/// En-tête + répertoire racine, chargés à la première demande.
async fn charge_archive<L: LecteurPlages>(
    etat: &EtatArchive,
    cle: &str,
    lecteur: &L,
) -> Option<Arc<PmArchive>> {
    let mut garde = etat.archive.lock().await;
    if let Some((url, a)) = garde.as_ref()
        && url == cle
    {
        return Some(a.clone());
    }
    let head = lecteur.plage(0, pmtiles::HEADER_LEN as u32).await?;
    let header = pmtiles::Header::parse(&head)?;
    let raw = lecteur
        .plage(header.root_offset, header.root_length as u32)
        .await?;
    let root = pmtiles::parse_directory(&pmtiles::decompress(&raw, header.internal_compression)?)?;
    let archive = Arc::new(PmArchive { header, root });
    if let Ok(mut f) = etat.feuilles.lock() {
        // Une autre archive : ses feuilles ne sont plus les nôtres.
        *f = LeafCache::default();
    }
    *garde = Some((cle.to_owned(), archive.clone()));
    Some(archive)
}

/// Sert une tuile depuis l'archive auto-hébergée.
async fn tuile_archive<L: LecteurPlages>(
    etat: &EtatArchive,
    cle: &str,
    lecteur: &L,
    z: u32,
    x: u32,
    y: u32,
) -> Response {
    let Some(archive) = charge_archive(etat, cle, lecteur).await else {
        return texte(StatusCode::BAD_GATEWAY, "archive illisible");
    };
    if z as u8 > archive.header.max_zoom || (z as u8) < archive.header.min_zoom {
        return texte(StatusCode::NOT_FOUND, "zoom absent de l'archive");
    }
    let id = pmtiles::tile_id(z as u8, x, y);

    // Racine, puis éventuellement un répertoire feuille (un seul niveau).
    let mut found = pmtiles::find(&archive.root, id);
    if let pmtiles::Found::Leaf { offset, length } = found {
        let key = (offset, length);
        let cached = etat.feuilles.lock().ok().and_then(|m| m.get(&key));
        let leaf = match cached {
            Some(l) => l,
            None => {
                let Some(raw) = lecteur
                    .plage(archive.header.leaf_offset + offset, length)
                    .await
                else {
                    return texte(StatusCode::BAD_GATEWAY, "répertoire illisible");
                };
                let Some(entries) = pmtiles::decompress(&raw, archive.header.internal_compression)
                    .and_then(|d| pmtiles::parse_directory(&d))
                else {
                    return texte(StatusCode::BAD_GATEWAY, "répertoire invalide");
                };
                let entries = Arc::new(entries);
                if let Ok(mut m) = etat.feuilles.lock() {
                    m.put(key, entries.clone());
                }
                entries
            }
        };
        found = pmtiles::find(&leaf, id);
    }

    let pmtiles::Found::Tile { offset, length } = found else {
        // Trou dans la couverture : normal en bord de zone.
        return texte(StatusCode::NOT_FOUND, "tuile absente");
    };
    let Some(octets) = lecteur
        .plage(archive.header.tile_data_offset + offset, length)
        .await
    else {
        return texte(StatusCode::BAD_GATEWAY, "tuile illisible");
    };

    let mut res = reponse_tuile(octets, archive.header.content_type());
    // Les tuiles vectorielles sont stockées compressées : on relaie le codage
    // tel quel plutôt que de décompresser pour rien.
    if let Some(codage) = archive.header.content_encoding() {
        res.set_header("content-encoding", codage);
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use paquets::test::TestClient;
    use paquets::vitesse;

    /// Une configuration explicite : jamais l'environnement du processus.
    fn config(pmtiles: Option<&str>, amont: Option<&str>, glyphes: Option<&str>) -> ConfigTuiles {
        ConfigTuiles::depuis(
            pmtiles.map(str::to_owned),
            amont.map(str::to_owned),
            glyphes.map(str::to_owned),
            None,
        )
    }

    /// Un client sur les deux routes, avec cette configuration.
    fn client(cfg: ConfigTuiles) -> TestClient {
        let cfg: &'static ConfigTuiles = Box::leak(Box::new(cfg));
        let mut app = vitesse::App::new();
        app.get("/tiles/style.json", move |req: Request| async move {
            style(&req, cfg)
        });
        app.get("/tiles/:z/:x/:y", move |req: Request| async move {
            tuile(&req, cfg).await
        });
        TestClient::new(app)
    }

    #[test]
    fn la_configuration_filtre_les_valeurs_invalides() {
        let c = ConfigTuiles::depuis(
            Some(" ftp://a.pmtiles ".into()),
            Some("https://a/{z}/{x}.png".into()),
            Some(String::new()),
            Some("  ".into()),
        );
        assert!(c.pmtiles.is_none());
        assert!(c.amont.is_none());
        assert!(c.glyphes.is_none());
        assert_eq!(c.agent, AGENT_DEFAUT);
        assert!(!c.configuree());
        let c = ConfigTuiles::depuis(
            Some(" https://a/b.pmtiles ".into()),
            Some(" https://a/{z}/{x}/{y}.png ".into()),
            None,
            Some(" Agent/2 ".into()),
        );
        assert_eq!(c.pmtiles.as_deref(), Some("https://a/b.pmtiles"));
        assert_eq!(c.amont.as_deref(), Some("https://a/{z}/{x}/{y}.png"));
        assert_eq!(c.agent, "Agent/2");
    }

    /// Le style doit rester valide et pointer sur notre propre endpoint : une
    /// erreur ici donne une carte vide, sans message d'erreur côté app.
    #[tokio::test]
    async fn style_json_is_valid_and_self_referencing() {
        let c = client(config(
            Some("https://exemple.invalid/a.pmtiles"),
            None,
            None,
        ));
        let resp = c
            .get("/tiles/style.json")
            .header("host", "moncap.example")
            .await;
        assert_eq!(resp.status(), 200);
        let style: Value = serde_json::from_slice(resp.bytes()).expect("JSON valide");

        assert_eq!(style["version"], 8);
        let tiles = style["sources"]["osm"]["tiles"][0].as_str().unwrap();
        assert_eq!(tiles, "https://moncap.example/tiles/{z}/{x}/{y}");
        // Sans polices hébergées, aucune couche de libellés : elle échouerait
        // au chargement du style.
        assert!(style.get("glyphs").is_none());
        let layers = style["layers"].as_array().unwrap();
        assert!(!layers.is_empty());
        assert!(
            layers.iter().all(|l| l["type"] != "symbol"),
            "pas de libellés sans glyphs"
        );
    }

    /// Avec des polices hébergées, les libellés apparaissent.
    #[tokio::test]
    async fn style_json_adds_labels_with_glyphs() {
        let c = client(config(
            Some("https://exemple.invalid/a.pmtiles"),
            None,
            Some("https://exemple.invalid/{fontstack}/{range}.pbf"),
        ));
        let resp = c
            .get("/tiles/style.json")
            .header("host", "moncap.example")
            .await;
        let style: Value = serde_json::from_slice(resp.bytes()).unwrap();
        assert!(style["glyphs"].is_string());
        let layers = style["layers"].as_array().unwrap();
        assert!(layers.iter().any(|l| l["type"] == "symbol"));
    }

    #[test]
    fn le_style_local_reste_en_http() {
        let s = style_json("localhost:3000", None);
        assert_eq!(
            s["sources"]["osm"]["tiles"][0],
            "http://localhost:3000/tiles/{z}/{x}/{y}"
        );
        let s = style_json("127.0.0.1:8080", None);
        assert_eq!(
            s["sources"]["osm"]["tiles"][0],
            "http://127.0.0.1:8080/tiles/{z}/{x}/{y}"
        );
    }

    #[tokio::test]
    async fn le_style_sans_source_est_absent() {
        let c = client(config(None, None, Some("https://g/{fontstack}")));
        let r = c.get("/tiles/style.json").await;
        assert_eq!(r.status(), 404);
        assert_eq!(r.text(), "tuiles non configurées");
    }

    /// Le relais de tuiles doit refuser tout ce qui sort des bornes : sans
    /// cela, n'importe qui pourrait lui faire relayer des requêtes arbitraires
    /// et saturer le serveur amont.
    #[tokio::test]
    async fn tiles_rejects_out_of_bounds() {
        // Amont configuré, sinon tout répondrait 404 sans rien valider.
        let c = client(config(
            None,
            Some("https://exemple.invalid/{z}/{x}/{y}.png"),
            None,
        ));
        // Au zoom 1 il n'existe que les tuiles 0 et 1 sur chaque axe.
        for (uri, message) in [
            ("/tiles/1/2/0", "tuile hors limites"),
            ("/tiles/1/0/2", "tuile hors limites"),
            ("/tiles/99/0/0", "zoom trop élevé"),
            ("/tiles/1/0/abc", "tuile invalide"),
        ] {
            let resp = c.get(uri).await;
            assert_eq!(resp.status(), 400, "{uri} aurait dû être refusée");
            assert_eq!(resp.text(), message, "{uri}");
        }
        // z et x non numériques : refusés avant tout.
        assert_eq!(c.get("/tiles/a/0/0").await.status(), 400);
        assert_eq!(c.get("/tiles/1/b/0").await.status(), 400);
    }

    /// Sans amont configuré, la fonction est simplement inactive.
    #[tokio::test]
    async fn tiles_absent_without_upstream() {
        let c = client(config(None, None, None));
        let resp = c.get("/tiles/10/511/340.png").await;
        assert_eq!(resp.status(), 404);
        assert_eq!(resp.text(), "tuiles non configurées");
        // Même une tuile hors bornes : le 404 passe avant la validation.
        assert_eq!(c.get("/tiles/1/0/abc").await.status(), 404);
    }

    #[test]
    fn y_garde_le_nombre_sans_l_extension() {
        assert_eq!(coordonnees(10, 511, "340.png"), Ok((10, 511, 340)));
        assert_eq!(coordonnees(10, 511, "340"), Ok((10, 511, 340)));
        assert_eq!(coordonnees(0, 0, "0.pbf.gz"), Ok((0, 0, 0)));
        assert_eq!(coordonnees(17, 0, "0"), Ok((17, 0, 0)));
        assert_eq!(coordonnees(18, 0, "0"), Err("zoom trop élevé"));
        assert_eq!(coordonnees(1, 0, ".png"), Err("tuile invalide"));
    }

    /// Un amont injoignable (port fermé en local, sans réseau) répond 502.
    #[tokio::test]
    async fn un_amont_injoignable_repond_502() {
        let c = client(config(
            None,
            Some("http://127.0.0.1:1/{z}/{x}/{y}.png"),
            None,
        ));
        let r = c.get("/tiles/3/7/6.png").await;
        assert_eq!(r.status(), 502);
        assert_eq!(r.text(), "amont injoignable");
    }

    /// Le cache borné évince bien les plus anciennes tuiles.
    #[test]
    fn tile_cache_evicts_oldest() {
        let mut cache = TileCache::default();
        for i in 0..(TILES_CACHE_MAX as u32 + 10) {
            cache.put((10, i, 0), (vec![1, 2, 3], "image/png".into()));
        }
        assert_eq!(cache.map.len(), TILES_CACHE_MAX);
        assert!(
            cache.get(&(10, 0, 0)).is_none(),
            "la plus ancienne doit sortir"
        );
        assert!(cache.get(&(10, TILES_CACHE_MAX as u32 + 5, 0)).is_some());
    }

    #[test]
    fn le_cache_des_feuilles_est_borne() {
        let mut cache = LeafCache::default();
        for i in 0..(FEUILLES_MAX as u64 + 3) {
            cache.put((i, 1), Arc::new(Vec::new()));
        }
        assert_eq!(cache.map.len(), FEUILLES_MAX);
        assert_eq!(cache.order.len(), FEUILLES_MAX);
        assert!(cache.get(&(0, 1)).is_none());
    }

    // -----------------------------------------------------------------------
    // Une archive PMTiles en mémoire, pour éprouver la lecture sans réseau.
    // -----------------------------------------------------------------------

    /// Une archive en mémoire, qui compte ses lectures.
    struct Memoire {
        octets: Vec<u8>,
        lectures: std::sync::atomic::AtomicUsize,
    }

    impl LecteurPlages for Memoire {
        async fn plage(&self, offset: u64, length: u32) -> Option<Vec<u8>> {
            self.lectures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let debut = offset as usize;
            let fin = debut.checked_add(length as usize)?;
            self.octets.get(debut..fin).map(<[u8]>::to_vec)
        }
    }

    fn varint(out: &mut Vec<u8>, mut v: u64) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }

    /// Un répertoire non compressé (décalages explicites).
    fn repertoire(entrees: &[pmtiles::Entry]) -> Vec<u8> {
        let mut out = Vec::new();
        varint(&mut out, entrees.len() as u64);
        let mut dernier = 0;
        for e in entrees {
            varint(&mut out, e.tile_id - dernier);
            dernier = e.tile_id;
        }
        for e in entrees {
            varint(&mut out, e.run_length as u64);
        }
        for e in entrees {
            varint(&mut out, e.length as u64);
        }
        for e in entrees {
            varint(&mut out, e.offset + 1);
        }
        out
    }

    /// Une archive : la tuile 0/0/0 et 1/0/0 à la racine, et la tuile 1/1/1
    /// (id 3) derrière une feuille. Tuiles en `tile_compression` donnée.
    fn archive(tile_compression: u8) -> Memoire {
        let feuille = repertoire(&[pmtiles::Entry {
            tile_id: 3,
            offset: 6,
            length: 3,
            run_length: 1,
        }]);
        let racine = repertoire(&[
            pmtiles::Entry {
                tile_id: 0,
                offset: 0,
                length: 3,
                run_length: 1,
            },
            pmtiles::Entry {
                tile_id: 1,
                offset: 3,
                length: 3,
                run_length: 1,
            },
            pmtiles::Entry {
                tile_id: 3,
                offset: 0,
                length: feuille.len() as u32,
                run_length: 0,
            },
        ]);
        let donnees = b"aaabbbccc";
        let racine_off = pmtiles::HEADER_LEN as u64;
        let feuille_off = racine_off + racine.len() as u64;
        let donnees_off = feuille_off + feuille.len() as u64;
        let mut h = vec![0u8; pmtiles::HEADER_LEN];
        h[0..7].copy_from_slice(b"PMTiles");
        h[7] = 3;
        h[8..16].copy_from_slice(&racine_off.to_le_bytes());
        h[16..24].copy_from_slice(&(racine.len() as u64).to_le_bytes());
        h[40..48].copy_from_slice(&feuille_off.to_le_bytes());
        h[56..64].copy_from_slice(&donnees_off.to_le_bytes());
        h[97] = 1;
        h[98] = tile_compression;
        h[99] = 1;
        h[100] = 0;
        h[101] = 1;
        let mut octets = h;
        octets.extend_from_slice(&racine);
        octets.extend_from_slice(&feuille);
        octets.extend_from_slice(donnees);
        Memoire {
            octets,
            lectures: Default::default(),
        }
    }

    #[tokio::test]
    async fn l_archive_sert_racine_et_feuille() {
        let etat = EtatArchive::default();
        let a = archive(2);
        let r = tuile_archive(&etat, "a", &a, 0, 0, 0).await;
        assert_eq!(r.status_code(), 200);
        assert_eq!(
            r.get_header("content-type"),
            Some("application/vnd.mapbox-vector-tile")
        );
        assert_eq!(r.get_header("cache-control"), Some(CACHE_TUILE));
        assert_eq!(r.get_header("content-encoding"), Some("gzip"));
        let corps = r.into_body().to_bytes().await.unwrap();
        assert_eq!(&corps[..], b"aaa");

        // 1/1/1 vit dans la feuille.
        let r = tuile_archive(&etat, "a", &a, 1, 1, 1).await;
        assert_eq!(r.status_code(), 200);
        assert_eq!(&r.into_body().to_bytes().await.unwrap()[..], b"ccc");

        // L'en-tête, la racine et la feuille ne se relisent pas : seule la
        // tuile coûte une lecture.
        let avant = a.lectures.load(std::sync::atomic::Ordering::Relaxed);
        let r = tuile_archive(&etat, "a", &a, 1, 1, 1).await;
        assert_eq!(r.status_code(), 200);
        assert_eq!(
            a.lectures.load(std::sync::atomic::Ordering::Relaxed),
            avant + 1
        );

        // 1/1/0 (id 4) n'est nulle part.
        let r = tuile_archive(&etat, "a", &a, 1, 1, 0).await;
        assert_eq!(r.status_code(), 404);
        // Zoom 2 hors de l'archive.
        let r = tuile_archive(&etat, "a", &a, 2, 0, 0).await;
        assert_eq!(r.status_code(), 404);
    }

    #[tokio::test]
    async fn brotli_et_zstd_disent_leur_codage() {
        for (compression, codage) in [(1, None), (3, Some("br")), (4, Some("zstd"))] {
            let etat = EtatArchive::default();
            let r = tuile_archive(&etat, "a", &archive(compression), 0, 0, 0).await;
            assert_eq!(r.status_code(), 200);
            assert_eq!(r.get_header("content-encoding"), codage, "{compression}");
        }
    }

    #[tokio::test]
    async fn une_archive_illisible_repond_502_et_se_relit_ensuite() {
        let etat = EtatArchive::default();
        let vide = Memoire {
            octets: b"pas une archive".to_vec(),
            lectures: Default::default(),
        };
        let r = tuile_archive(&etat, "a", &vide, 0, 0, 0).await;
        assert_eq!(r.status_code(), 502);
        // L'échec n'est pas gardé : la bonne archive se charge ensuite.
        let r = tuile_archive(&etat, "a", &archive(1), 0, 0, 0).await;
        assert_eq!(r.status_code(), 200);
    }
}
