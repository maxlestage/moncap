//! Le courtier des paquets d'itinéraires hors ligne (GitHub Releases).
//!
//! L'app lisait la liste des régions directement sur l'API GitHub et tirait les
//! archives des Releases. Le dépôt étant privé, GitHub répondait 404 : la liste
//! restait vide, aucune région n'était proposée, et le message affiché parlait
//! de réseau alors que le réseau allait très bien.
//!
//! Un jeton ne peut pas voyager dans l'app — n'importe qui l'extrait du binaire,
//! et il ouvrirait le dépôt entier. C'est donc le serveur qui le détient. Il
//! publie la liste, et pour un fichier il demande à GitHub l'adresse signée puis
//! la transmet : l'archive descend d'Amazon directement sur le téléphone, sans
//! traverser le dyno — deux gigaoctets par région n'ont rien à faire ici.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use paquets::prelude::*;
use serde::{Deserialize, Serialize};

use super::verrou;
use crate::cache;
use crate::erreur::{Erreur, Resultat};

/// Étiquette des Releases publiées par le workflow des tuiles.
///
/// Ce préfixe n'est pas cosmétique : il **borne** ce que le courtier accepte de
/// servir. Sans lui, l'endpoint deviendrait une porte ouverte sur toutes les
/// pièces jointes d'un dépôt privé.
const PAQUETS_PREFIXE: &str = "tuiles-eu-";

/// Dire lequel des deux manque : « indisponible » tout court a coûté une
/// soirée de recherche du côté du réseau.
const SANS_JETON: &str = "paquets non configurés : GITHUB_TOKEN absent";

/// Délai de chaque appel à GitHub. Sans lui, un GitHub qui ne répond plus
/// laissait la requête de l'app pendre indéfiniment.
const DELAI_GITHUB: Duration = Duration::from_secs(15);

/// Une région publiée, telle que l'app la reçoit.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct RegionPubliee {
    /// Étiquette de la Release, qui sert aussi d'identifiant de fichier.
    id: String,
    /// « France · Nouvelle-Aquitaine », tel que nommé par Geofabrik.
    nom: String,
    /// Poids de l'archive, mesuré sur la pièce jointe.
    octets: u64,
    /// Date de la dernière construction de l'archive (`updated_at` de la
    /// pièce jointe, ISO 8601). C'est elle qui dit qu'un paquet installé a
    /// une version plus fraîche en ligne : le poids seul peut rester
    /// identique d'une construction à l'autre.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    construit: String,
}

struct PaquetsCache {
    fetched_at: Instant,
    data: Vec<RegionPubliee>,
}

static PAQUETS_CACHE: Mutex<Option<PaquetsCache>> = Mutex::new(None);

/// Deux cents Releases coûtent trois appels à l'API GitHub, et elles ne
/// changent qu'à une reconstruction des tuiles.
const PAQUETS_TTL: Duration = Duration::from_secs(3600);

/// La clé de la liste dans le cache SQLite.
const CLE_DISQUE: &str = "regions";

/// Le jeton tel que collé, sans espaces ni guillemets autour.
///
/// Les guillemets sont retirés en plus des espaces : collé depuis un fichier
/// `.env` ou une commande, un jeton arrive souvent entouré de `"` ou de `'`.
/// Le tableau de bord Heroku les conserve tels quels, GitHub renvoie alors
/// 401, et rien ne le laisse deviner — la valeur *paraît* juste.
fn nettoie_jeton(brut: &str) -> String {
    brut.trim().trim_matches(['"', '\'']).trim().to_string()
}

/// Jeton GitHub détenu **par le serveur** (`GITHUB_TOKEN`, repli
/// `MONCAP_GITHUB_TOKEN`), jamais exposé.
pub(super) fn paquets_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .or_else(|_| std::env::var("MONCAP_GITHUB_TOKEN"))
        .ok()
        .map(|k| nettoie_jeton(&k))
        .filter(|k| !k.is_empty())
}

/// Décrit la **forme** d'un jeton, sans en révéler un seul caractère.
///
/// Un 401 ne dit pas lequel des deux : une valeur qui n'est pas un jeton, ou
/// un jeton authentique devenu invalide. La différence change tout le geste à
/// faire, et elle se lit dans le préfixe — que GitHub publie, et qui n'est
/// donc pas un secret.
fn forme_du_jeton(token: &str) -> String {
    let genre = if token.starts_with("github_pat_") {
        "à portée fine"
    } else if token.starts_with("ghp_") {
        "classique"
    } else if token.starts_with("gho_") || token.starts_with("ghs_") {
        "d'application, pas un jeton personnel"
    } else {
        "de format inconnu — ce n'est probablement pas un jeton"
    };
    format!("jeton {genre}, {} caractères", token.chars().count())
}

/// Dépôt qui porte les Releases (`MONCAP_DEPOT`, par défaut `MonCapMaps`,
/// public et dédié aux paquets : le code de l'app reste privé).
fn paquets_depot() -> String {
    std::env::var("MONCAP_DEPOT")
        .ok()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "maxlestage/MonCapMaps".to_string())
}

/// Une étiquette n'est qu'un identifiant de Release : minuscules, chiffres et
/// tirets. Tout le reste est refusé avant de composer une URL — c'est ce qui
/// empêche un `..` ou un `/` de sortir du dépôt visé.
fn etiquette_valide(tag: &str) -> bool {
    tag.starts_with(PAQUETS_PREFIXE)
        && tag.len() <= 128
        && tag
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Suffixe de pièce jointe pour chaque nature de fichier servie. `lieux`,
/// `rues` et `vitesses` sont les annexes hors ligne — annuaire de lieux,
/// index des voies nommées et jalons de limites de vitesse — produites par le
/// workflow des tuiles depuis le même extrait OSM ; les régions bâties avant
/// leur existence ne les portent pas, et l'app les traite comme optionnelles
/// (un 404 n'est pas une panne).
fn nature_suffixe(nature: &str) -> Option<&'static str> {
    match nature {
        "archive" => Some(".tar"),
        "config" => Some(".json"),
        "lieux" => Some(".lieux.zz"),
        "rues" => Some(".rues.zz"),
        "vitesses" => Some(".vitesses.zz"),
        _ => None,
    }
}

/// Le client des appels à l'API GitHub, avec son délai.
fn client_github() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(DELAI_GITHUB)
            .build()
            .unwrap_or_default()
    })
}

/// Le client qui **ne suit pas** les redirections, avec le même délai.
///
/// Sans cette politique, reqwest suivrait la redirection et rapatrierait
/// l'archive entière sur le dyno — deux gigaoctets pour rien. S'il ne se
/// construit pas, aucun repli : un client ordinaire suivrait la redirection.
fn client_sans_suivi() -> Resultat<&'static reqwest::Client> {
    static C: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(DELAI_GITHUB)
            .build()
            .map_err(|e| e.to_string())
    })
    .as_ref()
    .map_err(Erreur::interne)
}

/// Une demande à l'API GitHub, en-têtes et jeton compris.
fn demande(
    client: &reqwest::Client,
    url: String,
    accept: &str,
    token: &str,
) -> reqwest::RequestBuilder {
    client
        .get(url)
        .header(reqwest::header::USER_AGENT, "moncap")
        .header(reqwest::header::ACCEPT, accept)
        .bearer_auth(token)
}

fn passerelle(message: impl Into<String>) -> Erreur {
    Erreur::Statut(StatusCode::BAD_GATEWAY, message.into())
}

/// GET /offline/regions — les régions d'itinéraires téléchargeables.
pub(super) async fn liste(req: Request) -> Resultat<Response> {
    let Some(token) = paquets_token() else {
        return Err(Erreur::Indisponible(SANS_JETON.to_owned()));
    };
    let disque = cache::cache(&req);

    // Mémoire vide (un redémarrage) : la dernière liste gardée sur disque y
    // revient avec son âge — fraîche, elle évite trois appels à GitHub ;
    // périmée, elle sert encore de repli.
    let vide = verrou(&PAQUETS_CACHE).is_none();
    if vide
        && let Some((data, age)) = cache::lit_json::<Vec<RegionPubliee>>(disque, CLE_DISQUE).await
    {
        let age = Duration::from_secs(age.max(0) as u64);
        *verrou(&PAQUETS_CACHE) = Some(PaquetsCache {
            fetched_at: Instant::now().checked_sub(age).unwrap_or_else(Instant::now),
            data,
        });
    }

    if let Some(c) = verrou(&PAQUETS_CACHE).as_ref()
        && c.fetched_at.elapsed() < PAQUETS_TTL
    {
        return Ok(Response::new().json(&c.data));
    }

    match releases_publiees(&token).await {
        Ok(regions) => {
            cache::range_json(disque, CLE_DISQUE, &regions).await;
            *verrou(&PAQUETS_CACHE) = Some(PaquetsCache {
                fetched_at: Instant::now(),
                data: regions.clone(),
            });
            Ok(Response::new().json(regions))
        }
        Err(e) => {
            // Un cache périmé vaut mieux qu'une liste vide : les noms et les
            // poids ne bougent qu'à une reconstruction.
            if let Some(c) = verrou(&PAQUETS_CACHE).as_ref() {
                return Ok(Response::new().json(&c.data));
            }
            Err(passerelle(format!("GitHub : {e}")))
        }
    }
}

/// Relève les Releases du dépôt, page par page.
async fn releases_publiees(token: &str) -> Result<Vec<RegionPubliee>, String> {
    let depot = paquets_depot();
    let mut trouvees: Vec<RegionPubliee> = Vec::new();

    // Cent par page, et l'Europe en remplit trois.
    for page in 1..=5 {
        let url = format!("https://api.github.com/repos/{depot}/releases?per_page=100&page={page}");
        let resp = demande(client_github(), url, "application/vnd.github+json", token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            // 401 : la valeur n'est pas reconnue. 403 ou 404 : elle l'est, mais
            // ne porte pas les droits — ou ne voit pas le dépôt. Deux gestes
            // différents, donc deux messages.
            let indice = match resp.status().as_u16() {
                401 => format!(
                    " — {} refusé par GitHub. Vérifie qu'il est collé en entier \
                     et qu'il n'a pas expiré.",
                    forme_du_jeton(token)
                ),
                403 | 404 => format!(
                    " — {} accepté, mais sans accès à {depot}. Vérifie que le \
                     dépôt est dans « Repository access » et que « Contents » \
                     est en lecture.",
                    forme_du_jeton(token)
                ),
                _ => String::new(),
            };
            return Err(format!("HTTP {}{indice}", resp.status()));
        }
        let liste: Vec<serde_json::Value> = resp.json().await.map_err(|e| e.to_string())?;
        let recues = liste.len();
        trouvees.extend(regions_de(liste));
        if recues < 100 {
            break;
        }
    }
    Ok(trouvees)
}

/// La date de construction d'une Release : la ligne `construit: …` de ses
/// notes quand elle y est, sinon la date de son archive. Les régions
/// recopiées sur MonCapMaps portent leur date d'origine dans les notes ;
/// leur archive, datée de la copie, ferait proposer une mise à jour de
/// chaque région installée.
fn date_de_construction(notes: Option<&str>, archive: Option<&str>) -> String {
    notes
        .unwrap_or("")
        .lines()
        .find_map(|l| l.strip_prefix("construit: ").map(str::trim))
        .filter(|d| !d.is_empty())
        .or(archive)
        .unwrap_or("")
        .to_string()
}

/// Les régions d'une page de Releases.
fn regions_de(liste: Vec<serde_json::Value>) -> Vec<RegionPubliee> {
    let mut trouvees = Vec::new();
    for release in liste {
        let Some(tag) = release["tag_name"].as_str() else {
            continue;
        };
        if !tag.starts_with(PAQUETS_PREFIXE) {
            continue;
        }
        // Sans archive, rien à télécharger : une construction interrompue
        // laisse derrière elle la Release et sa configuration seule.
        let Some(tar) = release["assets"].as_array().and_then(|a| {
            a.iter()
                .find(|x| x["name"].as_str().is_some_and(|n| n.ends_with(".tar")))
        }) else {
            continue;
        };
        let titre = release["name"].as_str().unwrap_or("").trim();
        let construit = date_de_construction(release["body"].as_str(), tar["updated_at"].as_str());
        trouvees.push(RegionPubliee {
            id: tag.to_string(),
            construit,
            nom: if titre.is_empty() {
                tag.to_string()
            } else {
                titre.to_string()
            },
            octets: tar["size"].as_u64().unwrap_or(0),
        });
    }
    trouvees
}

/// GET /offline/regions/:tag/:nature — redirige vers l'adresse signée du
/// fichier (`archive` pour les tuiles, `config` pour la configuration,
/// `lieux`, `rues` et `vitesses` pour les annexes hors ligne).
///
/// GitHub répond à une demande de pièce jointe authentifiée par une redirection
/// vers une adresse signée, valable quelques minutes et qui ne demande aucune
/// authentification. C'est cette adresse-là qu'on transmet : le téléphone la
/// suit tout seul, et le dyno ne voit pas passer un octet du transfert.
pub(super) async fn fichier(req: Request) -> Resultat<Response> {
    let Some(token) = paquets_token() else {
        return Err(Erreur::Indisponible(SANS_JETON.to_owned()));
    };
    let tag = req.param("tag").unwrap_or("").to_owned();
    if !etiquette_valide(&tag) {
        return Err(Erreur::introuvable("étiquette inconnue"));
    }
    let Some(suffixe) = nature_suffixe(req.param("nature").unwrap_or("")) else {
        return Err(Erreur::introuvable("fichier inconnu"));
    };

    let depot = paquets_depot();
    let release: serde_json::Value = match demande(
        client_github(),
        format!("https://api.github.com/repos/{depot}/releases/tags/{tag}"),
        "application/vnd.github+json",
        &token,
    )
    .send()
    .await
    {
        Ok(r) if r.status().is_success() => match r.json().await {
            Ok(v) => v,
            Err(e) => return Err(passerelle(format!("GitHub : {e}"))),
        },
        Ok(r) => return Err(passerelle(format!("GitHub : {}", r.status()))),
        Err(e) => return Err(passerelle(format!("GitHub : {e}"))),
    };

    let Some(id) = piece_jointe(&release, suffixe) else {
        return Err(Erreur::introuvable("pièce jointe absente"));
    };

    let resp = demande(
        client_sans_suivi()?,
        format!("https://api.github.com/repos/{depot}/releases/assets/{id}"),
        "application/octet-stream",
        &token,
    )
    .send()
    .await
    .map_err(|e| passerelle(format!("GitHub : {e}")))?;

    match resp.headers().get(reqwest::header::LOCATION) {
        Some(adresse) => match adresse.to_str() {
            Ok(url) => Ok(Redirect::temporary(url).into_response()),
            Err(_) => Err(passerelle("adresse illisible")),
        },
        None => Err(passerelle(format!(
            "GitHub n'a pas redirigé (HTTP {})",
            resp.status()
        ))),
    }
}

/// L'identifiant de la première pièce jointe qui porte ce suffixe.
fn piece_jointe(release: &serde_json::Value, suffixe: &str) -> Option<u64> {
    release["assets"].as_array().and_then(|a| {
        a.iter()
            .find(|x| x["name"].as_str().is_some_and(|n| n.ends_with(suffixe)))
            .and_then(|x| x["id"].as_u64())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natures_de_paquet_connues() {
        // Les quatre fichiers d'une région, et rien d'autre : la nature est
        // une porte, pas un chemin — un nom inconnu ne compose aucune URL.
        assert_eq!(nature_suffixe("archive"), Some(".tar"));
        assert_eq!(nature_suffixe("config"), Some(".json"));
        assert_eq!(nature_suffixe("lieux"), Some(".lieux.zz"));
        assert_eq!(nature_suffixe("rues"), Some(".rues.zz"));
        assert_eq!(nature_suffixe("vitesses"), Some(".vitesses.zz"));
        assert_eq!(nature_suffixe("autre"), None);
        assert_eq!(nature_suffixe("../secret"), None);
    }

    /// Le préfixe d'étiquette n'est pas décoratif : c'est lui qui empêche le
    /// courtier de servir n'importe quelle pièce jointe d'un dépôt privé. Un
    /// `..` ou une barre oblique sortiraient du dépôt visé.
    #[test]
    fn seules_les_etiquettes_de_tuiles_passent() {
        assert!(etiquette_valide("tuiles-eu-france-nouvelle-aquitaine"));
        assert!(etiquette_valide("tuiles-eu-monaco"));

        // Un autre préfixe : ce sont peut-être les binaires signés de l'app.
        assert!(!etiquette_valide("v1.1.0"));
        assert!(!etiquette_valide("secrets"));
        // Une évasion de chemin, même sous le bon préfixe.
        assert!(!etiquette_valide("tuiles-eu-../../secrets"));
        assert!(!etiquette_valide("tuiles-eu-a/b"));
        // Ni majuscules ni caractères de requête : l'étiquette entre dans une
        // URL sans autre échappement.
        assert!(!etiquette_valide("tuiles-eu-France"));
        assert!(!etiquette_valide("tuiles-eu-a?b=c"));
        assert!(!etiquette_valide(&format!("tuiles-eu-{}", "a".repeat(200))));
    }

    /// Un jeton collé depuis un `.env` arrive entouré de guillemets, que le
    /// tableau de bord Heroku conserve tels quels. GitHub renvoie alors 401 et
    /// rien ne le laisse deviner : à l'œil, la valeur paraît juste.
    #[test]
    fn le_jeton_survit_aux_guillemets_et_aux_espaces() {
        for brut in [
            "github_pat_abc",
            " github_pat_abc ",
            "\"github_pat_abc\"",
            "'github_pat_abc'",
            "\" github_pat_abc \"",
            "\ngithub_pat_abc\n",
        ] {
            assert_eq!(nettoie_jeton(brut), "github_pat_abc", "{brut:?}");
        }
    }

    /// La forme d'un jeton se décrit sans en révéler un caractère : c'est ce
    /// qui distingue « ce n'est pas un jeton » de « ce jeton a expiré », et
    /// les deux n'appellent pas le même geste.
    #[test]
    fn la_forme_du_jeton_ne_fuit_rien() {
        let secret = "github_pat_11ABCDEFG0123456789";
        let dit = forme_du_jeton(secret);
        assert!(dit.contains("à portée fine"), "{dit}");
        assert!(dit.contains(&secret.chars().count().to_string()), "{dit}");
        // Aucun morceau du secret au-delà du préfixe public.
        assert!(!dit.contains("ABCDEFG"), "{dit}");
        assert!(!dit.contains("0123456789"), "{dit}");

        assert!(forme_du_jeton("ghp_xyz").contains("classique"));
        assert!(forme_du_jeton("moncap").contains("format inconnu"));
    }

    /// Seules les Releases de tuiles qui portent une archive sont proposées.
    #[test]
    fn les_regions_sans_archive_ne_sont_pas_proposees() {
        let page = serde_json::json!([
            {"tag_name": "tuiles-eu-monaco", "name": "  Monaco ",
             "assets": [{"name": "monaco.json", "id": 1},
                        {"name": "monaco.tar", "id": 2, "size": 1234,
                         "updated_at": "2026-08-01T00:00:00Z"}]},
            {"tag_name": "tuiles-eu-andorre", "name": "",
             "assets": [{"name": "andorre.tar", "id": 3}]},
            {"tag_name": "tuiles-eu-rate", "assets": [{"name": "rate.json"}]},
            {"tag_name": "v1.1.0", "assets": [{"name": "app.tar"}]}
        ]);
        let regions = regions_de(page.as_array().unwrap().clone());
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].nom, "Monaco");
        assert_eq!(regions[0].octets, 1234);
        assert_eq!(regions[1].nom, "tuiles-eu-andorre");
        let json = serde_json::to_value(&regions).unwrap();
        assert_eq!(json[0]["construit"], "2026-08-01T00:00:00Z");
        assert!(json[1].get("construit").is_none());

        let release = &page[0];
        assert_eq!(piece_jointe(release, ".tar"), Some(2));
        assert_eq!(piece_jointe(release, ".json"), Some(1));

        // Recopiée : la date des notes passe devant celle de l'archive.
        let copie = serde_json::json!([
            {"tag_name": "tuiles-eu-monaco", "name": "Monaco",
             "body": "Tuiles Valhalla\n\nconstruit: 2026-09-20T08:56:34Z",
             "assets": [{"name": "monaco.tar", "size": 1,
                         "updated_at": "2026-10-30T01:00:00Z"}]}
        ]);
        let regions = regions_de(copie.as_array().unwrap().clone());
        assert_eq!(regions[0].construit, "2026-09-20T08:56:34Z");
        assert_eq!(piece_jointe(release, ".rues.zz"), None);
    }
}
