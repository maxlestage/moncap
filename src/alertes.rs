//! Les signalements communautaires (police, accident, bouchon, danger…).

use paquets::orm::{Database, Executor};
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
    let siens = || Signalement::query().filter(Signalement::USER_ID.eq(uid));
    let (recents, total) = tokio::try_join!(
        siens()
            .filter(Signalement::CREATED_AT.gt(t - 120))
            .count(db),
        siens().count(db),
    )?;
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
    Ok(Signalement::delete_many()
        .filter(Signalement::EXPIRES_AT.le(maintenant()))
        .execute(db)
        .await?)
}

/// Ce qu'un vote fait d'un signalement.
#[derive(Debug)]
enum Issue {
    /// Il reste, ainsi compté.
    Garde(Signalement),
    /// Assez infirmé : il est retiré.
    Retire,
}

/// Le signalement après un vote, à l'instant `t`. « Toujours là » le
/// prolonge, sans dépasser deux heures de vie ; « plus là » le retire dès
/// deux infirmations qui l'emportent sur les confirmations.
fn apres_vote(mut s: Signalement, up: bool, t: i64) -> Issue {
    if up {
        s.confirms += 1;
        s.expires_at = (s.expires_at.max(t) + PROLONGATION).min(s.created_at + VIE_MAX);
        Issue::Garde(s)
    } else if s.denies + 1 >= 2 && s.denies + 1 > s.confirms {
        Issue::Retire
    } else {
        s.denies += 1;
        Issue::Garde(s)
    }
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
/// Lu et écrit dans une transaction : deux votes simultanés comptent tous
/// deux (le second est relancé sur le signalement déjà compté).
async fn vote(req: Request) -> Resultat<Response> {
    connexion::utilisateur(&req).await?;
    let id: i32 = param(&req, "id")?;
    let Vote { up } = corps_json(&req).await?;
    let issue = req
        .db()
        .transaction(|tx| {
            Box::pin(async move {
                let Some(s) = Signalement::find(tx, id).await? else {
                    return Ok(None);
                };
                let issue = apres_vote(s, up, maintenant());
                match &issue {
                    Issue::Garde(s) => {
                        s.update_only(
                            tx,
                            (
                                Signalement::CONFIRMS,
                                Signalement::DENIES,
                                Signalement::EXPIRES_AT,
                            ),
                        )
                        .await?;
                    }
                    Issue::Retire => {
                        Signalement::delete_by_key(tx, id).await?;
                    }
                }
                Ok(Some(issue))
            })
        })
        .await?;
    let hub = hub(&req);
    match issue {
        None => return Err(Erreur::introuvable("")),
        Some(Issue::Garde(s)) => hub.diffuse(&Evenement::Alert(Alerte::from(&s))),
        Some(Issue::Retire) => hub.diffuse(&Evenement::AlertGone { id: id as i64 }),
    }
    Ok(Response::new().status(204))
}

#[cfg(test)]
mod essais {
    use serde_json::{Value, json};

    use super::*;
    use crate::essais::{bearer, client, inscrit};

    #[tokio::test]
    async fn creation_bornee_et_votes() {
        let c = client().await;
        let (nom, jeton) = inscrit(&c, "alerte").await;
        let uid = crate::base::Utilisateur::query()
            .filter(crate::base::Utilisateur::CLE.eq(crate::connexion::cle_identifiant(&nom)))
            .one(&c.db)
            .await
            .unwrap()
            .id;

        // Trois en deux minutes, pas une de plus.
        let mut ids = Vec::new();
        for _ in 0..3 {
            let a = cree(&c.db, uid, "police", 44.8, -0.5, "  Contrôle ")
                .await
                .unwrap()
                .expect("dans la limite");
            assert_eq!(a.label, "Contrôle");
            ids.push(a.id);
        }
        assert!(
            cree(&c.db, uid, "police", 44.8, -0.5, "")
                .await
                .unwrap()
                .is_none()
        );

        let r = c
            .get("/alerts")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.json::<Vec<Value>>().len(), 3);

        // « Toujours là » : prolongé, sans dépasser deux heures de vie.
        let id = ids[0];
        for _ in 0..20 {
            let r = c
                .post(&format!("/alerts/{id}/vote"))
                .header("authorization", &bearer(&jeton))
                .json(&json!({"up": true}))
                .await;
            assert_eq!(r.status(), 204);
        }
        let s = Signalement::get(&c.db, id as i32).await.unwrap();
        assert_eq!(s.expires_at - s.created_at, VIE_MAX);
        assert_eq!(s.confirms, 20);

        // Deux « plus là » sur une alerte jamais confirmée : elle part.
        let id = ids[1];
        for attendu in [204, 204, 404] {
            let r = c
                .post(&format!("/alerts/{id}/vote"))
                .header("authorization", &bearer(&jeton))
                .json(&json!({"up": false}))
                .await;
            assert_eq!(r.status(), attendu);
        }
        let r = c
            .get("/alerts")
            .header("authorization", &bearer(&jeton))
            .await;
        assert_eq!(r.json::<Vec<Value>>().len(), 2);

        // Expirée : la purge l'emporte.
        Signalement::update_many()
            .set(Signalement::EXPIRES_AT, 0)
            .filter(Signalement::ID.eq(ids[2] as i32))
            .execute(&c.db)
            .await
            .unwrap();
        assert_eq!(purge(&c.db).await.unwrap(), 1);
    }

    #[test]
    fn un_vote_prolonge_ou_retire() {
        let s = Signalement {
            id: 1,
            category: "police".into(),
            label: String::new(),
            lat: 0.0,
            lon: 0.0,
            created_at: 1000,
            expires_at: 1000 + TTL,
            confirms: 0,
            denies: 0,
            user_id: 1,
        };
        // Confirmé : dix minutes de plus, sans dépasser deux heures de vie.
        let Issue::Garde(plus) = apres_vote(s.clone(), true, 1000) else {
            panic!("une confirmation garde le signalement");
        };
        assert_eq!(
            (plus.confirms, plus.expires_at),
            (1, 1000 + TTL + PROLONGATION)
        );
        let tard = Signalement {
            expires_at: 1000 + VIE_MAX - 60,
            ..s.clone()
        };
        let Issue::Garde(plafonne) = apres_vote(tard, true, 1000) else {
            panic!("une confirmation garde le signalement");
        };
        assert_eq!(plafonne.expires_at, 1000 + VIE_MAX);
        // Infirmé : la première infirmation compte, la deuxième retire.
        let Issue::Garde(moins) = apres_vote(s.clone(), false, 1000) else {
            panic!("une seule infirmation garde le signalement");
        };
        assert_eq!(moins.denies, 1);
        assert!(matches!(apres_vote(moins, false, 1000), Issue::Retire));
        // Plus confirmé qu'infirmé : il reste.
        let confirme = Signalement {
            confirms: 3,
            denies: 1,
            ..s
        };
        assert!(matches!(apres_vote(confirme, false, 1000), Issue::Garde(_)));
    }
}
