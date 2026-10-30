//! La trace d'un partage de position, gardée dans IndexedDB.
//!
//! Chaque position envoyée au lien de suivi s'y ajoute : après un
//! rechargement, la page sait encore combien de points sont partis et
//! quand, sans rien demander au serveur. IndexedDB plutôt que
//! `localStorage` : une trace grossit d'un point toutes les cinq secondes,
//! et `localStorage` plafonne vers 5 Mo pour tout le site — Mémoire y garde
//! déjà le profil. La trace s'efface quand le partage s'arrête.
//!
//! Base `moncap`, magasin `trace`, clés automatiques. Hors navigateur, ou
//! quand IndexedDB est refusé (navigation privée stricte), tout se réduit à
//! rien : la trace est un plus, jamais une condition du partage.

use serde::{Deserialize, Serialize};

/// Un point envoyé.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// L'heure de l'envoi, en millisecondes depuis l'époque Unix.
    pub t: f64,
    pub lat: f64,
    pub lon: f64,
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

/// Ajoute un point à la trace.
pub async fn ajoute(p: Point) {
    #[cfg(target_arch = "wasm32")]
    let _ = idb::ajoute(p).await;
    #[cfg(not(target_arch = "wasm32"))]
    let _ = p;
}

/// Toute la trace, dans l'ordre d'envoi.
pub async fn tous() -> Vec<Point> {
    #[cfg(target_arch = "wasm32")]
    {
        idb::tous().await.unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Vec::new()
    }
}

/// Efface la trace.
pub async fn efface() {
    #[cfg(target_arch = "wasm32")]
    let _ = idb::efface().await;
}

#[cfg(target_arch = "wasm32")]
mod idb {
    use std::cell::RefCell;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{
        IdbDatabase, IdbObjectStore, IdbObjectStoreParameters, IdbRequest, IdbTransactionMode,
    };

    use super::Point;

    const BASE: &str = "moncap";
    const VERSION: u32 = 1;
    const MAGASIN: &str = "trace";

    thread_local! {
        /// La base ouverte, gardée pour les envois suivants.
        static OUVERTE: RefCell<Option<IdbDatabase>> = const { RefCell::new(None) };
    }

    /// Attend la fin d'une requête IndexedDB : son résultat, ou l'échec.
    async fn attends(req: &IdbRequest) -> Result<JsValue, JsValue> {
        let promesse = js_sys::Promise::new(&mut |resous, rejette| {
            let r = req.clone();
            let ok = Closure::once_into_js(move |_: JsValue| {
                let v = r.result().unwrap_or(JsValue::UNDEFINED);
                let _ = resous.call1(&JsValue::NULL, &v);
            });
            let ko = Closure::once_into_js(move |e: JsValue| {
                let _ = rejette.call1(&JsValue::NULL, &e);
            });
            req.set_onsuccess(Some(ok.unchecked_ref()));
            req.set_onerror(Some(ko.unchecked_ref()));
        });
        JsFuture::from(promesse).await
    }

    async fn base() -> Result<IdbDatabase, JsValue> {
        if let Some(b) = OUVERTE.with(|o| o.borrow().clone()) {
            return Ok(b);
        }
        let fabrique = web_sys::window()
            .ok_or(JsValue::NULL)?
            .indexed_db()?
            .ok_or(JsValue::NULL)?;
        let ouverture = fabrique.open_with_u32(BASE, VERSION)?;
        // La première fois (ou à un changement de version) : le magasin.
        let req = ouverture.clone();
        let prepare = Closure::once_into_js(move |_: JsValue| {
            if let Ok(b) = req.result() {
                let b: IdbDatabase = b.unchecked_into();
                if !b.object_store_names().contains(MAGASIN) {
                    let params = IdbObjectStoreParameters::new();
                    params.set_auto_increment(true);
                    let _ = b.create_object_store_with_optional_parameters(MAGASIN, &params);
                }
            }
        });
        ouverture.set_onupgradeneeded(Some(prepare.unchecked_ref()));
        let b: IdbDatabase = attends(&ouverture).await?.unchecked_into();
        OUVERTE.with(|o| *o.borrow_mut() = Some(b.clone()));
        Ok(b)
    }

    async fn magasin(mode: IdbTransactionMode) -> Result<IdbObjectStore, JsValue> {
        base()
            .await?
            .transaction_with_str_and_mode(MAGASIN, mode)?
            .object_store(MAGASIN)
    }

    pub async fn ajoute(p: Point) -> Result<(), JsValue> {
        let json = serde_json::to_string(&p).map_err(|_| JsValue::NULL)?;
        let valeur = js_sys::JSON::parse(&json)?;
        let m = magasin(IdbTransactionMode::Readwrite).await?;
        attends(&m.add(&valeur)?).await.map(drop)
    }

    pub async fn tous() -> Result<Vec<Point>, JsValue> {
        let m = magasin(IdbTransactionMode::Readonly).await?;
        let tout = attends(&m.get_all()?).await?;
        let json: String = js_sys::JSON::stringify(&tout)?.into();
        Ok(serde_json::from_str(&json).unwrap_or_default())
    }

    pub async fn efface() -> Result<(), JsValue> {
        let m = magasin(IdbTransactionMode::Readwrite).await?;
        attends(&m.clear()?).await.map(drop)
    }
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
}
