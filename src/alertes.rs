//! Les signalements communautaires (police, accident, bouchon, danger…).

use paquets::orm::{Executor, params};
use paquets::prelude::*;
use serde::{Deserialize, Serialize};

use crate::base::{Signalement, maintenant};
use crate::commun::nettoie_label;
use crate::connexion;
use crate::erreur::{Erreur, Resultat, corps_json, param};
use crate::temps_reel::{Evenement, hub};

/// Durée de vie d'un signalement (prolongeable par confirmation).
const TTL: i64 = 30 * 60;
/// Prolongation apportée par un vote « toujours là ».
const PROLONGATION: i64 = 10 * 60;
/// Durée de vie maximale d'un signalement, même très confirmé.
const VIE_MAX: i64 = 2 * 60 * 60;

/// Un signalement tel qu'échangé avec les clients.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Alerte {
    pub id: i64,
    pub category: String,
    pub lat: f64,
    pub lon: f64,
    pub label: String,
    /// Création, en millisecondes.
    pub ts: u64,
    #[serde(default)]
    pub confirms: i32,
    #[serde(default)]
    pub denies: i32,
}

impl From<&Signalement> for Alerte {
    fn from(s: &Signalement) -> Self {
        Alerte {
            id: s.id as i64,
            category: s.category.clone(),
            lat: s.lat,
            lon: s.lon,
            label: s.label.clone(),
            ts: (s.created_at.max(0) as u64) * 1000,
            confirms: s.confirms,
            denies: s.denies,
        }
    }
}

pub fn routes(app: &mut App) {
    app.get("/alerts", liste);
    app.post("/alerts/:id/vote", vote);
}

/// Les signalements non expirés, du plus récent au plus ancien.
pub async fn actives(db: &Db) -> Resultat<Vec<Alerte>> {
    let items = Signalement::query()
        .filter(Signalement::EXPIRES_AT.gt(maintenant()))
        .order_by(Signalement::CREATED_AT.desc())
        .order_by(Signalement::ID.desc())
        .all(db)
        .await?;
    Ok(items.iter().map(Alerte::from).collect())
}

/// Crée un signalement, sauf abus : au plus 3 par 2 minutes et 10 au total
/// par utilisateur (au-delà, rien n'est créé). `None` quand c'est refusé.
pub async fn cree(
    db: &Db,
    uid: i32,
    categorie: &str,
    lat: f64,
    lon: f64,
    label: &str,
) -> Resultat<Option<Alerte>> {
    let t = maintenant();
    let comptes: Vec<(i64, i64)> = db
        .raw(
            "SELECT count(*) FILTER (WHERE created_at > $2), count(*) FROM alerts WHERE user_id = $1",
            &params![uid, t - 120],
        )
        .await?;
    let (recents, total) = comptes.first().copied().unwrap_or((0, 0));
    if recents >= 3 || total >= 10 {
        return Ok(None);
    }
    let mut s = Signalement {
        id: 0,
        category: nettoie_label(categorie),
        label: nettoie_label(label),
        lat,
        lon,
        created_at: t,
        expires_at: t + TTL,
        confirms: 0,
        denies: 0,
        user_id: uid,
    };
    s.insert(db).await?;
    Ok(Some(Alerte::from(&s)))
}

/// Supprime les signalements expirés (tâche de fond, toutes les minutes).
pub async fn purge<E: Executor>(db: &E) -> Resultat<u64> {
    Ok(db
        .raw_execute(
            "DELETE FROM alerts WHERE expires_at <= $1",
            &params![maintenant()],
        )
        .await?)
}

/// GET /alerts — signalements en cours.
async fn liste(req: Request) -> Resultat<Response> {
    connexion::utilisateur(&req).await?;
    Ok(Response::new().json(actives(req.db()).await?))
}

#[derive(Deserialize)]
struct Vote {
    up: bool,
}

/// POST /alerts/:id/vote — confirme (prolonge) ou infirme (peut supprimer).
/// Les mises à jour sont atomiques : deux votes simultanés comptent tous deux.
async fn vote(req: Request) -> Resultat<Response> {
    connexion::utilisateur(&req).await?;
    let id: i32 = param(&req, "id")?;
    let v: Vote = corps_json(&req).await?;
    let db = req.db();
    let t = maintenant();
    let hub = hub(&req);
    if v.up {
        let maj: Vec<Signalement> = db
            .raw(
                "UPDATE alerts SET confirms = confirms + 1, \
                 expires_at = LEAST(GREATEST(expires_at, $2) + $3, created_at + $4) \
                 WHERE id = $1 RETURNING id, category, label, lat, lon, created_at, \
                 expires_at, confirms, denies, user_id",
                &params![id, t, PROLONGATION, VIE_MAX],
            )
            .await?;
        let Some(s) = maj.first() else {
            return Err(Erreur::introuvable(""));
        };
        hub.diffuse(&Evenement::Alert(Alerte::from(s)));
    } else {
        let supprime: Vec<(i32,)> = db
            .raw(
                "DELETE FROM alerts WHERE id = $1 AND denies + 1 >= 2 AND denies + 1 > confirms \
                 RETURNING id",
                &params![id],
            )
            .await?;
        if !supprime.is_empty() {
            hub.diffuse(&Evenement::AlertGone { id: id as i64 });
        } else {
            let maj: Vec<Signalement> = db
                .raw(
                    "UPDATE alerts SET denies = denies + 1 WHERE id = $1 RETURNING id, category, \
                     label, lat, lon, created_at, expires_at, confirms, denies, user_id",
                    &params![id],
                )
                .await?;
            let Some(s) = maj.first() else {
                return Err(Erreur::introuvable(""));
            };
            hub.diffuse(&Evenement::Alert(Alerte::from(s)));
        }
    }
    Ok(Response::new().status(204))
}
