//! La position de l'appareil, par l'API de géolocalisation du navigateur.

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{PositionError, PositionOptions};

use crate::types::Coord;

fn geolocalisation() -> Option<web_sys::Geolocation> {
    web_sys::window()?.navigator().geolocation().ok()
}

pub fn disponible() -> bool {
    geolocalisation().is_some()
}

fn coord(v: JsValue) -> Coord {
    let p: web_sys::Position = v.unchecked_into();
    let c = p.coords();
    Coord {
        lat: c.latitude(),
        lon: c.longitude(),
    }
}

/// Le message d'une erreur de géolocalisation, en français.
pub fn message(code: u16) -> &'static str {
    match code {
        1 => "Partage de position refusé.",
        3 => "La position tarde à venir.",
        _ => "Position indisponible.",
    }
}

fn erreur(v: JsValue) -> &'static str {
    let e: PositionError = v.unchecked_into();
    message(e.code())
}

/// Une surveillance de la position. La lâcher l'arrête.
pub struct Surveillance {
    id: i32,
    _ok: Closure<dyn FnMut(JsValue)>,
    _ko: Closure<dyn FnMut(JsValue)>,
}

impl Drop for Surveillance {
    fn drop(&mut self) {
        if let Some(g) = geolocalisation() {
            g.clear_watch(self.id);
        }
    }
}

/// Suit la position, en haute précision. `None` si le navigateur n'a pas de
/// géolocalisation.
pub fn surveille(
    mut sur_position: impl FnMut(Coord) + 'static,
    mut sur_erreur: impl FnMut(&'static str) + 'static,
) -> Option<Surveillance> {
    let g = geolocalisation()?;
    let ok = Closure::<dyn FnMut(JsValue)>::new(move |v| sur_position(coord(v)));
    let ko = Closure::<dyn FnMut(JsValue)>::new(move |v| sur_erreur(erreur(v)));
    let options = PositionOptions::new();
    options.set_enable_high_accuracy(true);
    options.set_maximum_age(2000);
    let id = g
        .watch_position_with_error_callback_and_options(
            ok.as_ref().unchecked_ref(),
            Some(ko.as_ref().unchecked_ref()),
            &options,
        )
        .ok()?;
    Some(Surveillance {
        id,
        _ok: ok,
        _ko: ko,
    })
}

/// Demande la position une fois.
pub fn une_fois(
    sur_position: impl FnOnce(Coord) + 'static,
    sur_erreur: impl FnOnce(&'static str) + 'static,
) -> bool {
    let Some(g) = geolocalisation() else {
        return false;
    };
    // Des rappels à usage unique, que le navigateur garde vivants jusqu'à
    // l'appel.
    let ok = Closure::once_into_js(move |v: JsValue| sur_position(coord(v)));
    let ko = Closure::once_into_js(move |v: JsValue| sur_erreur(erreur(v)));
    g.get_current_position_with_error_callback(ok.unchecked_ref(), Some(ko.unchecked_ref()))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_erreurs_parlent_francais() {
        assert_eq!(message(1), "Partage de position refusé.");
        assert_eq!(message(2), "Position indisponible.");
        assert_eq!(message(3), "La position tarde à venir.");
    }
}
