//! La trace d'un partage de position, gardée dans IndexedDB.
//!
//! Chaque position envoyée au lien de suivi s'y ajoute : après un
//! rechargement, la page sait encore combien de points sont partis et
//! quand, sans rien demander au serveur. IndexedDB plutôt que
//! `localStorage` : une trace grossit d'un point toutes les cinq secondes,
//! et `localStorage` plafonne vers 5 Mo pour tout le site — Mémoire y garde
//! déjà le profil. La trace s'efface quand le partage s'arrête.
//!
//! Une base Mémoire (table `trace`, un point par ligne) rangée dans la base
//! IndexedDB `moncap.memoire` par `paquets::IndexedDbStorage`. Hors
//! navigateur, ou quand IndexedDB est refusé (navigation privée stricte),
//! tout se réduit à rien : la trace est un plus, jamais une condition du
//! partage.

use std::cell::RefCell;
use std::rc::Rc;

use paquets::orm::web::LocalStorageDb;
use paquets::orm::{Executor, Model};
use serde::{Deserialize, Serialize};

/// Un point envoyé.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// L'heure de l'envoi, en millisecondes depuis l'époque Unix.
    pub t: f64,
    pub lat: f64,
    pub lon: f64,
}

/// Un point tel que la base le range : l'identifiant suit l'ordre d'envoi.
#[paquets::model(table = "trace")]
struct PointTrace {
    id: i64,
    t: f64,
    lat: f64,
    lon: f64,
}

/// Ce que la page montre de la trace.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Resume {
    pub points: usize,
    pub dernier: Option<Point>,
}

impl Resume {
    pub fn de(points: &[Point]) -> Resume {
        Resume {
            points: points.len(),
            dernier: points.iter().copied().max_by(|a, b| a.t.total_cmp(&b.t)),
        }
    }

    pub fn ajoute(&mut self, p: Point) {
        self.points += 1;
        self.dernier = Some(p);
    }
}

thread_local! {
    /// La base ouverte, gardée pour les envois suivants.
    static OUVERTE: RefCell<Option<Rc<LocalStorageDb>>> = const { RefCell::new(None) };
}

/// La base de la trace, ouverte une fois ; `None` sans IndexedDB.
async fn base() -> Option<Rc<LocalStorageDb>> {
    if let Some(db) = OUVERTE.with(|o| o.borrow().clone()) {
        return Some(db);
    }
    let stockage = paquets::IndexedDbStorage::open("moncap.memoire")
        .await
        .ok()?;
    let db = LocalStorageDb::open_with("trace", stockage).ok()?;
    db.create_table_if_not_exists::<PointTrace>().await.ok()?;
    // Deux appels ont pu ouvrir en même temps : le premier arrivé sert à
    // tous, pour qu'une seule copie des lignes écrive.
    Some(OUVERTE.with(|o| o.borrow_mut().get_or_insert(Rc::new(db)).clone()))
}

/// Ajoute un point à la trace.
pub async fn ajoute(p: Point) {
    if let Some(db) = base().await {
        ajoute_dans(&db, p).await;
    }
}

/// Toute la trace, dans l'ordre d'envoi.
pub async fn tous() -> Vec<Point> {
    match base().await {
        Some(db) => tous_dans(&db).await,
        None => Vec::new(),
    }
}

/// Efface la trace.
pub async fn efface() {
    if let Some(db) = base().await {
        efface_dans(&db).await;
    }
}

async fn ajoute_dans(db: &LocalStorageDb, p: Point) {
    let mut ligne = PointTrace {
        id: 0,
        t: p.t,
        lat: p.lat,
        lon: p.lon,
    };
    let _ = ligne.insert(db).await;
}

async fn tous_dans(db: &LocalStorageDb) -> Vec<Point> {
    PointTrace::query()
        .order_by(PointTrace::ID.asc())
        .all(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|l| Point {
            t: l.t,
            lat: l.lat,
            lon: l.lon,
        })
        .collect()
}

async fn efface_dans(db: &LocalStorageDb) {
    let _ = PointTrace::delete_many().execute(db).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_resume_suit_le_dernier_point() {
        let a = Point {
            t: 2.0,
            lat: 1.0,
            lon: 1.0,
        };
        let b = Point {
            t: 1.0,
            lat: 0.0,
            lon: 0.0,
        };
        let mut r = Resume::de(&[a, b]);
        assert_eq!(r.points, 2);
        assert_eq!(r.dernier, Some(a));
        r.ajoute(b);
        assert_eq!((r.points, r.dernier), (3, Some(b)));
        assert_eq!(Resume::de(&[]), Resume::default());
    }

    #[test]
    fn la_base_garde_les_points_dans_l_ordre_puis_s_efface() {
        use paquets::orm::web::MemoryStorage;
        paquets::orm::block_on(async {
            let db = LocalStorageDb::open_with("trace", MemoryStorage::new()).unwrap();
            db.create_table_if_not_exists::<PointTrace>().await.unwrap();
            let a = Point {
                t: 1.0,
                lat: 44.8,
                lon: -0.6,
            };
            let b = Point {
                t: 2.0,
                lat: 45.8,
                lon: 1.3,
            };
            ajoute_dans(&db, a).await;
            ajoute_dans(&db, b).await;
            assert_eq!(tous_dans(&db).await, [a, b]);
            efface_dans(&db).await;
            assert_eq!(tous_dans(&db).await, []);
        });
    }

    #[test]
    fn hors_navigateur_la_trace_est_vide() {
        paquets::orm::block_on(async {
            ajoute(Point {
                t: 1.0,
                lat: 0.0,
                lon: 0.0,
            })
            .await;
            assert!(tous().await.is_empty());
        });
    }
}
