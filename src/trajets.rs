//! Les trajets parcourus (et leur requalification après coup) et les
//! recherches de destination récentes.

use std::collections::HashSet;

use paquets::prelude::*;
use serde::Deserialize;

use crate::base::{Recherche, Trajet, maintenant};
use crate::commun::{coord_valide, nettoie_label, verifie_coord};
use crate::connexion;
use crate::erreur::{Erreur, Resultat, corps_json, param};
use crate::positions::{Coord, route_legs_km};

/// Au plus tant de points par trajet enregistré.
const MAX_POINTS: usize = 10_000;
/// Nombre maximal de recherches récentes conservées par utilisateur.
const MAX_RECENTS: usize = 12;

pub fn routes(app: &mut App) {
    app.get("/trips", liste_trajets);
    app.post("/trips", ajoute_trajet);
    app.patch("/trips/:id", requalifie_trajet);
    app.delete("/trips/:id", supprime_trajet);
    app.get("/searches", liste_recherches);
    app.post("/searches", ajoute_recherche);
    app.delete("/searches", efface_recherches);
    app.delete("/searches/:id", supprime_recherche);
}

// ---------------------------------------------------------------------------
// Les trajets.
// ---------------------------------------------------------------------------

/// Trajet parcouru envoyé par le client pour être enregistré.
#[derive(Deserialize)]
struct NewTrip {
    #[serde(default)]
    label: String,
    /// Points parcourus (dans l'ordre). La distance est recalculée côté serveur.
    points: Vec<Coord>,
    /// Durée réelle du trajet en minutes (mesurée par le client).
    #[serde(default)]
    duration_min: f64,
    /// Trajet professionnel. Absent d'un vieux client : faux, donc personnel
    /// — on ne qualifie jamais un trajet à la place de quelqu'un.
    #[serde(default)]
    pro: bool,
    /// Le motif, libre.
    #[serde(default)]
    motif: String,
}

/// Requalification d'un trajet déjà enregistré.
///
/// On ne classe pas ses trajets au moment où on les roule : on les classe le
/// dimanche soir, ou à la fin du mois, devant la liste. Les deux champs sont
/// optionnels et indépendants : cocher « professionnel » ne doit pas effacer
/// un motif déjà écrit.
#[derive(Deserialize)]
struct MajTrip {
    #[serde(default)]
    pro: Option<bool>,
    #[serde(default)]
    motif: Option<String>,
}

/// GET /trips — les trajets de l'utilisateur, les plus récents d'abord.
async fn liste_trajets(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let items = Trajet::query()
        .filter(Trajet::USER_ID.eq(qui.user_id))
        .order_by(Trajet::CREATED_AT.desc())
        .order_by(Trajet::ID.desc())
        .all(req.db())
        .await?;
    Ok(Response::new().json(items))
}

/// POST /trips — enregistre un trajet parcouru par l'utilisateur.
async fn ajoute_trajet(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let t: NewTrip = corps_json(&req).await?;
    // Au moins deux points valides pour former un tracé.
    let points: Vec<Coord> = t
        .points
        .into_iter()
        .filter(|p| coord_valide(p.lat, p.lon))
        .take(MAX_POINTS)
        .collect();
    if points.len() < 2 {
        return Err(Erreur::requete("un trajet requiert au moins deux points"));
    }
    let distance_km = route_legs_km(&points).iter().sum::<f64>() + 0.0;
    let label = {
        let l = nettoie_label(&t.label);
        if l.is_empty() {
            format!("Trajet de {distance_km:.1} km")
        } else {
            l
        }
    };
    let mut trajet = Trajet {
        id: 0,
        label,
        distance_km,
        duration_min: t.duration_min.max(0.0),
        polyline: encode_polyline(&points),
        created_at: maintenant(),
        user_id: qui.user_id,
        pro: t.pro,
        motif: nettoie_label(&t.motif),
    };
    trajet.insert(req.db()).await?;
    Ok(Response::new().json(trajet))
}

/// PATCH /trips/:id — requalifie un trajet : professionnel ou non, et motif.
///
/// Le filtre sur `user_id` n'est pas une précaution de style : sans lui,
/// n'importe qui requalifierait les trajets d'autrui en connaissant un
/// identifiant. Un trajet qui n'appartient pas au demandeur se comporte comme
/// un trajet qui n'existe pas — 404, et rien de plus.
async fn requalifie_trajet(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let id: i32 = param(&req, "id")?;
    let maj: MajTrip = corps_json(&req).await?;
    let db = req.db();
    let Some(mut trajet) = Trajet::query()
        .filter(Trajet::ID.eq(id))
        .filter(Trajet::USER_ID.eq(qui.user_id))
        .first(db)
        .await?
    else {
        return Err(Erreur::introuvable("trajet introuvable"));
    };
    if maj.pro.is_none() && maj.motif.is_none() {
        return Ok(Response::new().json(trajet));
    }
    // Chaque champ est indépendant : cocher « professionnel » ne doit pas
    // effacer un motif déjà écrit, et l'inverse non plus.
    if let Some(pro) = maj.pro {
        trajet.pro = pro;
    }
    if let Some(motif) = maj.motif {
        trajet.motif = nettoie_label(&motif);
    }
    let n = Trajet::update_many()
        .set(Trajet::PRO, trajet.pro)
        .set(Trajet::MOTIF, trajet.motif.clone())
        .filter(Trajet::ID.eq(id))
        .filter(Trajet::USER_ID.eq(qui.user_id))
        .execute(db)
        .await?;
    if n == 0 {
        // Supprimé entre-temps.
        return Err(Erreur::introuvable("trajet introuvable"));
    }
    Ok(Response::new().json(trajet))
}

/// DELETE /trips/:id — supprime un trajet de l'utilisateur.
async fn supprime_trajet(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let id: i32 = param(&req, "id")?;
    let n = Trajet::delete_many()
        .filter(Trajet::ID.eq(id))
        .filter(Trajet::USER_ID.eq(qui.user_id))
        .execute(req.db())
        .await?;
    if n == 0 {
        return Err(Erreur::introuvable(""));
    }
    Ok(Response::new().status(204))
}

/// Encode un tracé en `lat,lon;lat,lon;…` (6 décimales, ~0,1 m de précision).
fn encode_polyline(points: &[Coord]) -> String {
    points
        .iter()
        .map(|c| format!("{:.6},{:.6}", c.lat, c.lon))
        .collect::<Vec<_>>()
        .join(";")
}

// ---------------------------------------------------------------------------
// Les recherches récentes.
// ---------------------------------------------------------------------------

/// Recherche de destination envoyée par le client pour être mémorisée.
#[derive(Deserialize)]
struct NewSearch {
    name: String,
    #[serde(default)]
    subtitle: String,
    lat: f64,
    lon: f64,
}

/// Les recherches d'un compte, les plus récentes d'abord.
async fn recherches_de(db: &Db, uid: i32) -> Resultat<Vec<Recherche>> {
    Ok(Recherche::query()
        .filter(Recherche::USER_ID.eq(uid))
        .order_by(Recherche::CREATED_AT.desc())
        .order_by(Recherche::ID.desc())
        .all(db)
        .await?)
}

/// GET /searches — recherches de destination récentes de l'utilisateur.
async fn liste_recherches(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    Ok(Response::new().json(recherches_de(req.db(), qui.user_id).await?))
}

/// POST /searches — mémorise une recherche (dédoublonne et plafonne la liste).
async fn ajoute_recherche(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let s: NewSearch = corps_json(&req).await?;
    verifie_coord(s.lat, s.lon)?;
    let name = nettoie_label(&s.name);
    if name.is_empty() {
        return Err(Erreur::requete("nom de recherche vide"));
    }
    let db = req.db();
    let mut recherche = Recherche {
        id: 0,
        name,
        subtitle: nettoie_label(&s.subtitle),
        lat: s.lat,
        lon: s.lon,
        created_at: maintenant(),
        user_id: qui.user_id,
    };
    recherche.insert(db).await?;

    // Dédoublonne (même lieu) et ne garde que les MAX_RECENTS plus récentes.
    let toutes = recherches_de(db, qui.user_id).await?;
    let elaguees = recents_to_prune(&toutes, recherche.id, MAX_RECENTS);
    if !elaguees.is_empty() {
        Recherche::delete_many()
            .filter(Recherche::USER_ID.eq(qui.user_id))
            .filter(Recherche::ID.is_in(elaguees))
            .execute(db)
            .await?;
    }
    Ok(Response::new().json(recherche))
}

/// DELETE /searches/:id — supprime une recherche récente de l'utilisateur.
async fn supprime_recherche(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let id: i32 = param(&req, "id")?;
    let n = Recherche::delete_many()
        .filter(Recherche::ID.eq(id))
        .filter(Recherche::USER_ID.eq(qui.user_id))
        .execute(req.db())
        .await?;
    if n == 0 {
        return Err(Erreur::introuvable(""));
    }
    Ok(Response::new().status(204))
}

/// DELETE /searches — efface toutes les recherches récentes de l'utilisateur.
async fn efface_recherches(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    Recherche::delete_many()
        .filter(Recherche::USER_ID.eq(qui.user_id))
        .execute(req.db())
        .await?;
    Ok(Response::new().status(204))
}

/// Clé de dédoublonnage d'une recherche : nom normalisé (minuscules, espaces
/// internes réduits) + coordonnées arrondies (~10 m). Deux recherches de même
/// clé désignent le même lieu.
fn recent_key(name: &str, lat: f64, lon: f64) -> String {
    let norm = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    format!("{norm}|{lat:.4},{lon:.4}")
}

/// À partir des recherches d'un utilisateur (les plus récentes d'abord),
/// renvoie les identifiants à supprimer : doublons d'un lieu déjà vu et tout
/// ce qui dépasse `max`. L'entrée `keep_id` (celle qu'on vient d'insérer) est
/// toujours conservée et fait autorité : un doublon plus ancien du même lieu
/// est élagué, quel que soit l'ordre des égalités de date.
fn recents_to_prune(items: &[Recherche], keep_id: i32, max: usize) -> Vec<i32> {
    let mut seen = HashSet::new();
    let mut kept = 0usize;
    // On amorce avec l'entrée conservée pour que ses doublons plus anciens
    // soient élagués même si l'ordre par date les place avant elle.
    if let Some(k) = items.iter().find(|i| i.id == keep_id) {
        seen.insert(recent_key(&k.name, k.lat, k.lon));
        kept = 1;
    }
    let mut prune = Vec::new();
    for it in items {
        if it.id == keep_id {
            continue;
        }
        let key = recent_key(&it.name, it.lat, it.lon);
        // Doublon d'un lieu déjà retenu, ou dépassement du plafond → à élaguer.
        if !seen.insert(key) || kept >= max {
            prune.push(it.id);
        } else {
            kept += 1;
        }
    }
    prune
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_search(id: i32, name: &str, lat: f64, lon: f64) -> Recherche {
        Recherche {
            id,
            name: name.to_string(),
            subtitle: String::new(),
            lat,
            lon,
            created_at: id as i64,
            user_id: 1,
        }
    }

    #[test]
    fn recents_prune_removes_duplicate_place() {
        // Plus récentes d'abord ; id=5 vient d'être inséré (keep_id).
        let items = vec![
            mk_search(5, "124 Rue Billaudel", 44.83, -0.57),
            mk_search(4, "Gare", 44.82, -0.55),
            mk_search(3, " 124  RUE  billaudel ", 44.83, -0.57), // doublon (casse + espaces)
            mk_search(2, "Parc", 44.84, -0.58),
        ];
        // Le doublon plus ancien (id=3) est supprimé ; le neuf (id=5) reste.
        assert_eq!(recents_to_prune(&items, 5, 12), vec![3]);
    }

    #[test]
    fn recents_prune_caps_to_max() {
        let items = vec![
            mk_search(3, "A", 1.0, 1.0),
            mk_search(2, "B", 2.0, 2.0),
            mk_search(1, "C", 3.0, 3.0),
        ];
        // max=2 : on garde le neuf (id=3) + un autre, on élague le plus ancien.
        assert_eq!(recents_to_prune(&items, 3, 2), vec![1]);
    }

    #[test]
    fn recents_prune_keeps_new_entry_even_if_old_duplicate() {
        // L'entrée fraîche (keep_id=2) est un doublon d'une plus ancienne (id=1).
        let items = vec![
            mk_search(2, "Lieu", 10.0, 10.0),
            mk_search(1, "lieu", 10.0, 10.0),
        ];
        // C'est l'ancienne (id=1) qui part, pas la nouvelle.
        assert_eq!(recents_to_prune(&items, 2, 12), vec![1]);
    }

    #[test]
    fn recents_prune_nothing_when_all_unique_under_cap() {
        let items = vec![
            mk_search(3, "A", 1.0, 1.0),
            mk_search(2, "B", 2.0, 2.0),
            mk_search(1, "C", 3.0, 3.0),
        ];
        assert!(recents_to_prune(&items, 3, 12).is_empty());
    }

    #[test]
    fn polyline_six_decimales() {
        let pts = [
            Coord { lat: 1.0, lon: 2.0 },
            Coord {
                lat: 44.1234567,
                lon: -0.5,
            },
        ];
        assert_eq!(
            encode_polyline(&pts),
            "1.000000,2.000000;44.123457,-0.500000"
        );
    }
}

#[cfg(test)]
mod essais {
    use serde_json::{Value, json};

    use crate::essais::{bearer, client_ou_rien, inscrit};

    #[tokio::test]
    async fn trips_round_trip() {
        let c = client_ou_rien!();
        let (_, jeton) = inscrit(&c, "trip").await;
        let r = c
            .post("/trips")
            .header("authorization", &bearer(&jeton))
            .json(&json!({
                "label": "Balade",
                "points": [{"lat": 48.85, "lon": 2.35}, {"lat": 48.86, "lon": 2.36}],
                "duration_min": 12.0
            }))
            .await;
        assert_eq!(r.status(), 200, "{}", r.text());
        let t: Value = r.json();
        assert_eq!(t["polyline"], "48.850000,2.350000;48.860000,2.360000");
        assert!(t.get("user_id").is_none());

        let r = c
            .get("/trips")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 200);
        let liste: Vec<Value> = r.json();
        assert!(liste.iter().any(|t| t["label"] == "Balade"));

        let id = t["id"].as_i64().unwrap();
        let r = c
            .delete(&format!("/trips/{id}"))
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .delete(&format!("/trips/{id}"))
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 404);
    }

    #[tokio::test]
    async fn un_trajet_sans_nom_ni_points_suffisants() {
        let c = client_ou_rien!();
        let (_, jeton) = inscrit(&c, "trip-nom").await;
        let r = c
            .post("/trips")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"points": [{"lat": 0.0, "lon": 0.0}, {"lat": 99.0, "lon": 0.0}]}))
            .await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.text(), "un trajet requiert au moins deux points");
        let r = c
            .post("/trips")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"points": [{"lat": 0.0, "lon": 0.0}, {"lat": 0.0, "lon": 0.0127}], "duration_min": -3.0}))
            .await;
        assert_eq!(r.status(), 200);
        let t: Value = r.json();
        assert_eq!(t["label"], "Trajet de 1.4 km");
        assert_eq!(t["duration_min"], 0.0);
    }

    /// La nature d'un trajet se décide après coup — le dimanche soir, devant
    /// la liste — et jamais à la place de quelqu'un.
    #[tokio::test]
    async fn un_trajet_se_requalifie_apres_coup() {
        let c = client_ou_rien!();
        let (_, jeton) = inscrit(&c, "requal").await;
        let r = c
            .post("/trips")
            .header("authorization", &bearer(&jeton))
            .json(&json!({
                "label": "Tournée",
                "points": [{"lat": 44.83, "lon": -0.57}, {"lat": 44.84, "lon": -0.58}],
                "duration_min": 20.0
            }))
            .await;
        let cree: Value = r.json();
        let id = cree["id"].as_i64().unwrap();
        // Non qualifié à l'enregistrement : personnel, et sans motif.
        assert_eq!(cree["pro"], json!(false));
        assert_eq!(cree["motif"], json!(""));

        let r = c
            .patch(&format!("/trips/{id}"))
            .header("authorization", &bearer(&jeton))
            .json(&json!({"pro": true, "motif": "Livraison Dupont"}))
            .await;
        assert_eq!(r.status(), 200);
        let maj: Value = r.json();
        assert_eq!(maj["pro"], json!(true));
        assert_eq!(maj["motif"], json!("Livraison Dupont"));
        assert_eq!(maj["label"], json!("Tournée"));

        // Les deux champs sont indépendants.
        let r = c
            .patch(&format!("/trips/{id}"))
            .header("authorization", &bearer(&jeton))
            .json(&json!({"pro": false}))
            .await;
        let maj: Value = r.json();
        assert_eq!(maj["pro"], json!(false));
        assert_eq!(
            maj["motif"],
            json!("Livraison Dupont"),
            "le motif survit à un changement de nature"
        );

        // Un corps vide rend le trajet tel quel.
        let r = c
            .patch(&format!("/trips/{id}"))
            .header("authorization", &bearer(&jeton))
            .json(&json!({}))
            .await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.json::<Value>()["motif"], json!("Livraison Dupont"));
    }

    /// Le trajet d'autrui se comporte comme un trajet qui n'existe pas.
    #[tokio::test]
    async fn on_ne_requalifie_pas_le_trajet_d_autrui() {
        let c = client_ou_rien!();
        let (_, sien) = inscrit(&c, "proprio").await;
        let (_, autre) = inscrit(&c, "voisin").await;
        let r = c
            .post("/trips")
            .header("authorization", &bearer(&sien))
            .json(&json!({
                "label": "Privé",
                "points": [{"lat": 44.83, "lon": -0.57}, {"lat": 44.84, "lon": -0.58}],
                "duration_min": 5.0
            }))
            .await;
        let id = r.json::<Value>()["id"].as_i64().unwrap();

        let r = c
            .patch(&format!("/trips/{id}"))
            .header("authorization", &bearer(&autre))
            .json(&json!({"pro": true}))
            .await;
        assert_eq!(r.status(), 404);
        assert_eq!(r.text(), "trajet introuvable");

        let r = c
            .get("/trips")
            .header("authorization", &bearer(&sien))
            .await;
        let liste: Vec<Value> = r.json();
        let mien = liste
            .iter()
            .find(|t| t["id"].as_i64() == Some(id))
            .expect("le trajet doit rester au propriétaire");
        assert_eq!(mien["pro"], json!(false));
    }

    #[tokio::test]
    async fn les_recherches_se_dedoublonnent_et_se_plafonnent() {
        let c = client_ou_rien!();
        let (_, jeton) = inscrit(&c, "rech").await;
        let r = c
            .post("/searches")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"name": "   ", "lat": 1.0, "lon": 1.0}))
            .await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.text(), "nom de recherche vide");
        let r = c
            .post("/searches")
            .header("authorization", &bearer(&jeton))
            .json(&json!({"name": "", "lat": 100.0, "lon": 1.0}))
            .await;
        assert_eq!(r.text(), "coordonnées invalides");

        for i in 0..14 {
            let r = c
                .post("/searches")
                .header("authorization", &bearer(&jeton))
                .json(&json!({"name": format!("Lieu {i}"), "lat": 44.0 + f64::from(i) * 0.01, "lon": -0.5}))
                .await;
            assert_eq!(r.status(), 200);
        }
        // Le même lieu, autrement écrit : l'ancien part.
        let r = c
            .post("/searches")
            .header("authorization", &bearer(&jeton))
            .json(
                &json!({"name": "  lieu   13 ", "subtitle": "Bordeaux", "lat": 44.13, "lon": -0.5}),
            )
            .await;
        let neuf: Value = r.json();
        assert_eq!(neuf["subtitle"], "Bordeaux");
        let r = c
            .get("/searches")
            .header("authorization", &bearer(&jeton))
            .await;
        let liste: Vec<Value> = r.json();
        assert_eq!(liste.len(), 12);
        assert_eq!(liste[0]["id"], neuf["id"]);
        assert_eq!(
            liste.iter().filter(|s| s["lat"] == 44.13).count(),
            1,
            "un seul exemplaire du même lieu"
        );

        let id = liste[1]["id"].as_i64().unwrap();
        let r = c
            .delete(&format!("/searches/{id}"))
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .delete(&format!("/searches/{id}"))
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 404);
        let r = c
            .delete("/searches")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 204);
        let r = c
            .get("/searches")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.json::<Value>(), json!([]));
    }
}
