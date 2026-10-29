//! Les trajets parcourus (et leur requalification après coup) et les
//! destinations choisies — toutes, sans plafond.

use paquets::orm::params;
use paquets::prelude::*;
use serde::Deserialize;

use crate::base::{Recherche, Trajet, maintenant};
use crate::commun::{coord_valide, nettoie_label, verifie_coord};
use crate::connexion;
use crate::erreur::{Erreur, Resultat, corps_json, param};
use crate::positions::{Coord, route_legs_km};

/// Au plus tant de points par trajet enregistré.
const MAX_POINTS: usize = 10_000;

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
// Les destinations.
// ---------------------------------------------------------------------------

/// Destination choisie, envoyée par le client pour être enregistrée.
#[derive(Deserialize)]
struct NewSearch {
    name: String,
    #[serde(default)]
    subtitle: String,
    lat: f64,
    lon: f64,
}

/// Les destinations d'un compte, les plus récentes d'abord (`limite` : les
/// N premières seulement).
async fn recherches_de(db: &Db, uid: i32, limite: Option<u64>) -> Resultat<Vec<Recherche>> {
    let mut requete = Recherche::query()
        .filter(Recherche::USER_ID.eq(uid))
        .order_by(Recherche::RANG.desc());
    if let Some(n) = limite {
        requete = requete.limit(n);
    }
    Ok(requete.all(db).await?)
}

/// GET /searches[?limit=N] — toutes les destinations de l'utilisateur, les
/// plus récentes d'abord ; `limit` n'en rend que les N premières.
async fn liste_recherches(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let limite = match req.query("limit") {
        None => None,
        Some(v) => Some(
            v.trim()
                .parse::<u64>()
                .map_err(|_| Erreur::requete("limit attendu : un entier"))?,
        ),
    };
    Ok(Response::new().json(recherches_de(req.db(), qui.user_id, limite).await?))
}

/// POST /searches — enregistre une destination. **Toutes** sont gardées : un
/// lieu déjà choisi (même nom, à une dizaine de mètres près) remonte en tête
/// et compte une visite de plus, au lieu de faire un doublon. Une seule
/// requête, atomique : deux envois simultanés du même lieu font deux visites
/// d'une même ligne.
async fn ajoute_recherche(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    let s: NewSearch = corps_json(&req).await?;
    verifie_coord(s.lat, s.lon)?;
    let name = nettoie_label(&s.name);
    if name.is_empty() {
        return Err(Erreur::requete("nom de recherche vide"));
    }
    let cle = recent_key(&name, s.lat, s.lon);
    let lignes: Vec<Recherche> = req
        .db()
        .raw(
            "INSERT INTO searches \
             (name, subtitle, lat, lon, created_at, user_id, cle, fois, rang) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, \
             (SELECT coalesce(max(rang), 0) + 1 FROM searches WHERE user_id = ?6)) \
             ON CONFLICT (user_id, cle) DO UPDATE SET name = excluded.name, \
             subtitle = excluded.subtitle, lat = excluded.lat, lon = excluded.lon, \
             created_at = excluded.created_at, fois = fois + 1, rang = excluded.rang \
             RETURNING id, name, subtitle, lat, lon, created_at, user_id, cle, fois, rang",
            &params![
                name,
                nettoie_label(&s.subtitle),
                s.lat,
                s.lon,
                maintenant(),
                qui.user_id,
                cle
            ],
        )
        .await?;
    let recherche = lignes
        .into_iter()
        .next()
        .ok_or_else(|| Erreur::interne("destination non enregistrée"))?;
    Ok(Response::new().json(recherche))
}

/// DELETE /searches/:id — supprime une destination de l'utilisateur.
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

/// DELETE /searches — efface toutes les destinations de l'utilisateur.
async fn efface_recherches(req: Request) -> Resultat<Response> {
    let qui = connexion::utilisateur(&req).await?;
    Recherche::delete_many()
        .filter(Recherche::USER_ID.eq(qui.user_id))
        .execute(req.db())
        .await?;
    Ok(Response::new().status(204))
}

/// Clé de dédoublonnage d'une destination : nom normalisé (minuscules,
/// espaces internes réduits) + coordonnées arrondies (~10 m). Deux
/// destinations de même clé désignent le même lieu.
fn recent_key(name: &str, lat: f64, lon: f64) -> String {
    let norm = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    format!("{norm}|{lat:.4},{lon:.4}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_cle_d_un_lieu_ignore_casse_espaces_et_quelques_metres() {
        let k = recent_key("124 Rue Billaudel", 44.83001, -0.57002);
        assert_eq!(k, "124 rue billaudel|44.8300,-0.5700");
        assert_eq!(recent_key(" 124  RUE  billaudel ", 44.83004, -0.57), k);
        assert_ne!(recent_key("124 Rue Billaudel", 44.832, -0.57), k);
        assert_ne!(recent_key("Gare", 44.83, -0.57), k);
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

    use crate::essais::{bearer, client, inscrit};

    #[tokio::test]
    async fn trips_round_trip() {
        let c = client().await;
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
        let c = client().await;
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
        let c = client().await;
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
        let c = client().await;
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
    async fn toutes_les_destinations_sont_gardees_sans_doublon() {
        let c = client().await;
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

        // Bien plus que les douze d'avant : aucune ne se perd.
        for i in 0..30 {
            let r = c
                .post("/searches")
                .header("authorization", &bearer(&jeton))
                .json(&json!({"name": format!("Lieu {i}"), "lat": 44.0 + f64::from(i) * 0.01, "lon": -0.5}))
                .await;
            assert_eq!(r.status(), 200);
            assert_eq!(r.json::<Value>()["fois"], 1);
        }
        // Le même lieu, autrement écrit : la même ligne remonte, une visite de plus.
        let r = c
            .post("/searches")
            .header("authorization", &bearer(&jeton))
            .json(
                &json!({"name": "  lieu   13 ", "subtitle": "Bordeaux", "lat": 44.13, "lon": -0.5}),
            )
            .await;
        let revu: Value = r.json();
        assert_eq!(revu["subtitle"], "Bordeaux");
        assert_eq!(revu["name"], "lieu   13");
        assert_eq!(revu["fois"], 2);
        let r = c
            .get("/searches")
            .header("authorization", &bearer(&jeton))
            .await;
        let liste: Vec<Value> = r.json();
        assert_eq!(liste.len(), 30);
        assert_eq!(liste[0]["id"], revu["id"]);
        for cache in ["user_id", "cle", "rang"] {
            assert!(liste[0].get(cache).is_none(), "{cache}");
        }
        assert_eq!(
            liste.iter().filter(|s| s["lat"] == 44.13).count(),
            1,
            "un seul exemplaire du même lieu"
        );
        let r = c
            .get("/searches?limit=5")
            .header("authorization", &bearer(&jeton))
            .await;
        let cinq: Vec<Value> = r.json();
        assert_eq!(cinq.len(), 5);
        assert_eq!(cinq[0]["id"], revu["id"]);
        let r = c
            .get("/searches?limit=beaucoup")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.status(), 400);

        // Les destinations des autres ne se voient pas.
        let (_, autre) = inscrit(&c, "rech-autre").await;
        let r = c
            .get("/searches")
            .header("authorization", &bearer(&autre))
            .await;
        assert_eq!(r.json::<Value>(), json!([]));

        let id = liste[1]["id"].as_i64().unwrap();
        let r = c
            .delete(&format!("/searches/{id}"))
            .header("authorization", &bearer(&autre))
            .await;
        assert_eq!(
            r.status(),
            404,
            "on ne supprime pas la destination d'autrui"
        );
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
