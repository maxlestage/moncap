//! La géométrie de la carte : Web Mercator, tuiles, cadrage.
//!
//! Ce que Leaflet calculait pour nous, et qu'il fallait croire sur parole.
//! Ici chaque règle est une fonction pure, testée sans navigateur — c'est
//! le seul endroit où une erreur de signe se voit avant d'être à l'écran.
//!
//! Les positions sont tenues en **Mercator normalisé** : `x` et `y` vont de
//! 0 à 1 sur le monde entier, `y` vers le sud. Le pixel d'un zoom donné
//! s'obtient en multipliant par [`echelle`].

use crate::types::Coord;

/// Le côté d'une tuile, en pixels.
pub const TUILE: f64 = 256.0;
/// Les bornes du zoom. Au-delà de 19, les tuiles d'OpenStreetMap n'existent
/// pas ; en deçà de 2, le monde est plus petit qu'un écran de téléphone.
pub const ZOOM_MIN: f64 = 2.0;
pub const ZOOM_MAX: f64 = 19.0;
/// La latitude où le Mercator s'arrête : le monde y devient carré.
pub const LAT_MAX: f64 = 85.051_128_78;

/// Le modèle d'adresse des tuiles. Une seule adresse, sans les
/// sous-domaines `a.`, `b.`, `c.` d'autrefois : l'équipe d'OpenStreetMap
/// demande de les abandonner — HTTP/2 et HTTP/3 ont rendu inutile la
/// répartition sur plusieurs noms, et les anciens peuvent cesser de
/// répondre.
pub const MODELE_TUILES: &str = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";

/// La taille du monde, en pixels, à un zoom donné.
pub fn echelle(zoom: f64) -> f64 {
    TUILE * 2f64.powf(zoom)
}

/// Un point en Mercator normalisé.
pub fn projette(c: Coord) -> (f64, f64) {
    let lat = c.lat.clamp(-LAT_MAX, LAT_MAX).to_radians();
    let x = (c.lon + 180.0) / 360.0;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0;
    (x, y)
}

/// L'inverse de [`projette`].
pub fn deprojette(x: f64, y: f64) -> Coord {
    let lon = x * 360.0 - 180.0;
    let n = std::f64::consts::PI * (1.0 - 2.0 * y);
    let lat = n.sinh().atan().to_degrees();
    Coord { lat, lon }
}

/// Ce que montre la carte : un centre et un zoom, fractionnaire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vue {
    pub centre: (f64, f64),
    pub zoom: f64,
}

impl Vue {
    pub fn sur(c: Coord, zoom: f64) -> Vue {
        Vue {
            centre: projette(c),
            zoom,
        }
        .bornee()
    }

    /// Le zoom dans ses bornes, le centre dans le monde. En longitude le
    /// monde boucle ; en latitude il s'arrête aux pôles du Mercator.
    pub fn bornee(self) -> Vue {
        Vue {
            centre: (self.centre.0, self.centre.1.clamp(0.0, 1.0)),
            zoom: self.zoom.clamp(ZOOM_MIN, ZOOM_MAX),
        }
    }

    /// Où tombe un point du monde, en pixels depuis le coin de la carte.
    pub fn vers_ecran(&self, p: (f64, f64), taille: (f64, f64)) -> (f64, f64) {
        let s = echelle(self.zoom);
        (
            (p.0 - self.centre.0) * s + taille.0 / 2.0,
            (p.1 - self.centre.1) * s + taille.1 / 2.0,
        )
    }

    /// Le point du monde sous un pixel de la carte.
    pub fn depuis_ecran(&self, e: (f64, f64), taille: (f64, f64)) -> (f64, f64) {
        let s = echelle(self.zoom);
        (
            self.centre.0 + (e.0 - taille.0 / 2.0) / s,
            self.centre.1 + (e.1 - taille.1 / 2.0) / s,
        )
    }

    /// La carte suit le doigt : un glissement de `(dx, dy)` pixels.
    pub fn glisse(&self, dx: f64, dy: f64) -> Vue {
        let s = echelle(self.zoom);
        Vue {
            centre: (self.centre.0 - dx / s, self.centre.1 - dy / s),
            zoom: self.zoom,
        }
        .bornee()
    }

    /// Change de zoom en gardant immobile le point sous `ancre` — le
    /// curseur, ou le milieu des deux doigts.
    pub fn zoome_autour(&self, zoom: f64, ancre: (f64, f64), taille: (f64, f64)) -> Vue {
        let fixe = self.depuis_ecran(ancre, taille);
        let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        let s = echelle(zoom);
        Vue {
            centre: (
                fixe.0 - (ancre.0 - taille.0 / 2.0) / s,
                fixe.1 - (ancre.1 - taille.1 / 2.0) / s,
            ),
            zoom,
        }
        .bornee()
    }

    /// Entre deux vues, à l'avancement `t` (0 → 1).
    pub fn vers(&self, autre: &Vue, t: f64) -> Vue {
        let l = |a: f64, b: f64| a + (b - a) * t;
        Vue {
            centre: (
                l(self.centre.0, autre.centre.0),
                l(self.centre.1, autre.centre.1),
            ),
            zoom: l(self.zoom, autre.zoom),
        }
    }
}

/// La vue qui montre tous ces points, avec une marge, sans dépasser
/// `zoom_max`. Un point seul est centré à `zoom_max`.
pub fn cadre(points: &[Coord], taille: (f64, f64), marge: f64, zoom_max: f64) -> Option<Vue> {
    let premiers = points.first()?;
    let (mut x0, mut y0) = projette(*premiers);
    let (mut x1, mut y1) = (x0, y0);
    for p in points {
        let (x, y) = projette(*p);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let centre = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (dx, dy) = (x1 - x0, y1 - y0);
    let utile = (
        (taille.0 - 2.0 * marge).max(1.0),
        (taille.1 - 2.0 * marge).max(1.0),
    );
    let zoom = if dx <= 0.0 && dy <= 0.0 {
        zoom_max
    } else {
        // À quel zoom la boîte remplit-elle l'espace utile ?
        let zx = if dx > 0.0 {
            (utile.0 / (dx * TUILE)).log2()
        } else {
            f64::INFINITY
        };
        let zy = if dy > 0.0 {
            (utile.1 / (dy * TUILE)).log2()
        } else {
            f64::INFINITY
        };
        zx.min(zy).min(zoom_max)
    };
    Some(Vue { centre, zoom }.bornee())
}

/// Le niveau de tuiles à charger pour un zoom fractionnaire : le plus
/// proche, comme Leaflet. Les tuiles sont alors affichées entre 0,71 et
/// 1,41 fois leur taille — toujours nettes, jamais pixellisées.
pub fn niveau(zoom: f64) -> u32 {
    zoom.round().clamp(0.0, ZOOM_MAX) as u32
}

/// Une tuile, et sa place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Tuile {
    pub z: u32,
    /// Colonne **non repliée** : à droite de l'antiméridien, la carte
    /// continue, et la tuile y garde sa place même si son image est celle de
    /// la colonne repliée.
    pub x: i64,
    pub y: i64,
}

impl Tuile {
    pub fn url(&self) -> String {
        let n = 1i64 << self.z;
        let x = self.x.rem_euclid(n);
        MODELE_TUILES
            .replace("{z}", &self.z.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &self.y.to_string())
    }

    pub fn cle(&self) -> String {
        format!("{}/{}/{}", self.z, self.x, self.y)
    }

    /// Le coin haut-gauche, en Mercator normalisé.
    pub fn coin(&self) -> (f64, f64) {
        let n = (1u64 << self.z) as f64;
        (self.x as f64 / n, self.y as f64 / n)
    }
}

/// Les tuiles d'un niveau qui couvrent l'écran, du centre vers les bords :
/// celles qu'on regarde se chargent d'abord.
pub fn tuiles_visibles(vue: &Vue, taille: (f64, f64), z: u32) -> Vec<Tuile> {
    let n = (1u64 << z) as f64;
    let haut_gauche = vue.depuis_ecran((0.0, 0.0), taille);
    let bas_droit = vue.depuis_ecran(taille, taille);
    let x0 = (haut_gauche.0 * n).floor() as i64;
    let x1 = (bas_droit.0 * n).floor() as i64;
    let y0 = ((haut_gauche.1 * n).floor() as i64).max(0);
    let y1 = ((bas_droit.1 * n).floor() as i64).min(n as i64 - 1);
    let (cx, cy) = (vue.centre.0 * n, vue.centre.1 * n);
    let mut v = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            v.push(Tuile { z, x, y });
        }
    }
    v.sort_by(|a, b| {
        let d = |t: &Tuile| (t.x as f64 + 0.5 - cx).powi(2) + (t.y as f64 + 0.5 - cy).powi(2);
        d(a).partial_cmp(&d(b)).unwrap_or(std::cmp::Ordering::Equal)
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const BORDEAUX: Coord = Coord {
        lat: 44.8378,
        lon: -0.5792,
    };
    const LIMOGES: Coord = Coord {
        lat: 45.8336,
        lon: 1.2611,
    };

    fn proche(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn la_projection_fait_l_aller_retour() {
        let (x, y) = projette(BORDEAUX);
        let c = deprojette(x, y);
        assert!(proche(c.lat, BORDEAUX.lat, 1e-9) && proche(c.lon, BORDEAUX.lon, 1e-9));
    }

    #[test]
    fn le_meridien_et_l_equateur_sont_au_milieu() {
        assert_eq!(projette(Coord { lat: 0.0, lon: 0.0 }), (0.5, 0.5));
    }

    #[test]
    fn les_poles_sont_bornes() {
        let (_, y) = projette(Coord {
            lat: 90.0,
            lon: 0.0,
        });
        assert!(proche(y, 0.0, 1e-9), "{y}");
    }

    #[test]
    fn la_tuile_de_bordeaux_au_zoom_dix() {
        // La tuile de référence d'OpenStreetMap pour ce point.
        let (x, y) = projette(BORDEAUX);
        let n = 1024.0;
        assert_eq!(((x * n).floor(), (y * n).floor()), (510.0, 369.0));
    }

    #[test]
    fn l_ecran_et_le_monde_se_repondent() {
        let v = Vue::sur(BORDEAUX, 12.3);
        let taille = (390.0, 420.0);
        let p = projette(LIMOGES);
        let e = v.vers_ecran(p, taille);
        let retour = v.depuis_ecran(e, taille);
        assert!(proche(retour.0, p.0, 1e-12) && proche(retour.1, p.1, 1e-12));
        // Le centre de la vue tombe au centre de l'écran.
        assert_eq!(v.vers_ecran(v.centre, taille), (195.0, 210.0));
    }

    #[test]
    fn zoomer_garde_le_point_sous_le_curseur() {
        let taille = (800.0, 600.0);
        let v = Vue::sur(BORDEAUX, 10.0);
        let curseur = (620.0, 120.0);
        let avant = v.depuis_ecran(curseur, taille);
        let apres = v.zoome_autour(13.5, curseur, taille);
        let encore = apres.depuis_ecran(curseur, taille);
        assert!(proche(avant.0, encore.0, 1e-12) && proche(avant.1, encore.1, 1e-12));
        assert_eq!(apres.zoom, 13.5);
    }

    #[test]
    fn la_carte_suit_le_doigt() {
        let taille = (400.0, 400.0);
        let v = Vue::sur(BORDEAUX, 11.0);
        let p = projette(LIMOGES);
        let avant = v.vers_ecran(p, taille);
        let apres = v.glisse(30.0, -12.0).vers_ecran(p, taille);
        assert!(proche(apres.0 - avant.0, 30.0, 1e-6));
        assert!(proche(apres.1 - avant.1, -12.0, 1e-6));
    }

    #[test]
    fn le_zoom_reste_dans_ses_bornes() {
        let v = Vue::sur(BORDEAUX, 40.0);
        assert_eq!(v.zoom, ZOOM_MAX);
        let v = v.zoome_autour(-3.0, (0.0, 0.0), (100.0, 100.0));
        assert_eq!(v.zoom, ZOOM_MIN);
    }

    #[test]
    fn le_cadrage_montre_tous_les_points() {
        let taille = (390.0, 420.0);
        let v = cadre(&[BORDEAUX, LIMOGES], taille, 40.0, 17.0).unwrap();
        for c in [BORDEAUX, LIMOGES] {
            let (x, y) = v.vers_ecran(projette(c), taille);
            assert!((39.0..=351.0).contains(&x), "x hors marge : {x}");
            assert!((39.0..=381.0).contains(&y), "y hors marge : {y}");
        }
        // Et pas de beaucoup plus loin qu'il ne faut : un cran de zoom de
        // plus ferait sortir l'un des deux.
        let plus = Vue {
            zoom: v.zoom + 1.0,
            ..v
        };
        let hors = [BORDEAUX, LIMOGES].iter().any(|c| {
            let (x, y) = plus.vers_ecran(projette(*c), taille);
            !(0.0..=390.0).contains(&x) || !(0.0..=420.0).contains(&y)
        });
        assert!(hors, "le cadrage pourrait zoomer davantage");
    }

    #[test]
    fn un_point_seul_est_centre_au_zoom_maximal() {
        let v = cadre(&[BORDEAUX], (300.0, 300.0), 20.0, 12.0).unwrap();
        assert_eq!(v.zoom, 12.0);
        assert_eq!(v.centre, projette(BORDEAUX));
        assert!(cadre(&[], (300.0, 300.0), 20.0, 12.0).is_none());
    }

    #[test]
    fn les_tuiles_couvrent_l_ecran_et_commencent_au_centre() {
        let taille = (390.0, 420.0);
        let v = Vue::sur(BORDEAUX, 10.0);
        let t = tuiles_visibles(&v, taille, niveau(v.zoom));
        // 390 px de large tiennent sur deux ou trois colonnes de 256 px.
        assert!((4..=9).contains(&t.len()), "{} tuiles", t.len());
        assert_eq!(
            (t[0].x, t[0].y),
            (510, 369),
            "la première est sous le centre"
        );
    }

    #[test]
    fn les_tuiles_bouclent_en_longitude() {
        let t = Tuile { z: 2, x: 5, y: 1 };
        assert_eq!(t.url(), "https://tile.openstreetmap.org/2/1/1.png");
        let t = Tuile { z: 2, x: -1, y: 1 };
        assert_eq!(t.url(), "https://tile.openstreetmap.org/2/3/1.png");
    }

    #[test]
    fn pas_de_tuile_hors_du_monde_en_latitude() {
        let v = Vue {
            centre: (0.5, 0.0),
            zoom: 3.0,
        };
        let t = tuiles_visibles(&v, (800.0, 800.0), 3);
        assert!(t.iter().all(|t| (0..8).contains(&t.y)));
    }

    #[test]
    fn le_niveau_est_le_plus_proche() {
        assert_eq!(niveau(10.4), 10);
        assert_eq!(niveau(10.6), 11);
        assert_eq!(niveau(25.0), 19);
    }

    #[test]
    fn le_vol_passe_par_le_milieu() {
        let a = Vue::sur(BORDEAUX, 8.0);
        let b = Vue::sur(LIMOGES, 12.0);
        let m = a.vers(&b, 0.5);
        assert_eq!(m.zoom, 10.0);
        assert_eq!(a.vers(&b, 1.0), b);
    }
}
