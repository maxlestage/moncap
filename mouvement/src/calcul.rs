//! Les calculs du mouvement, sans navigateur.
//!
//! Tout ce qui décide d'une position, d'un délai ou d'un nombre affiché vit
//! ici, en fonctions pures : la règle se teste en natif, là où une
//! bibliothèque d'animation ne se teste qu'en regardant l'écran.

/// Interpolation linéaire entre `a` et `b`.
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Borne `x` dans `[min, max]`.
pub fn borne(x: f64, min: f64, max: f64) -> f64 {
    x.max(min).min(max)
}

/// Le facteur d'amortissement pour un intervalle quelconque.
///
/// Lenis applique son `lerp` (0,1 par défaut) une fois par image. Sur un
/// écran à 120 Hz, la page rejoindrait donc sa cible deux fois plus vite
/// qu'à 60 Hz : le même geste ne donnerait pas la même sensation selon
/// l'appareil. On ramène le facteur « par image à 60 Hz » à l'intervalle
/// réellement écoulé :
///
/// `1 − (1 − k)^(dt × 60)`
///
/// À 60 Hz on retrouve `k` exactement ; deux images à 120 Hz donnent le même
/// chemin qu'une seule à 60.
pub fn amorti(k: f64, dt_s: f64) -> f64 {
    let k = borne(k, 0.0, 1.0);
    1.0 - (1.0 - k).powf(borne(dt_s, 0.0, 0.25) * 60.0)
}

/// La courbe d'arrivée des sites qui bougent bien : un départ franc, une
/// fin qui se pose. C'est l'`expo.out` de GSAP, et la courbe par défaut de
/// Lenis.
pub fn expo_sortie(t: f64) -> f64 {
    if t >= 1.0 {
        1.0
    } else if t <= 0.0 {
        0.0
    } else {
        1.0 - 2f64.powf(-10.0 * t)
    }
}

/// Un pas du défilement inertiel : où en est la page, et si elle est
/// arrivée.
///
/// En dessous d'un demi-pixel l'écart ne se voit plus : on se pose sur la
/// cible plutôt que de courir après pendant des centaines d'images.
pub fn pas_inertiel(actuel: f64, cible: f64, k: f64, dt_s: f64) -> (f64, bool) {
    let suivant = lerp(actuel, cible, amorti(k, dt_s));
    if (cible - suivant).abs() < 0.5 {
        (cible, true)
    } else {
        (suivant, false)
    }
}

/// Le delta d'une molette, en pixels.
///
/// Les navigateurs ne s'accordent pas : Chrome compte en pixels, Firefox
/// souvent en lignes, et certains pilotes en pages. Sans cette conversion,
/// un cran de molette ferait bouger la page de trois pixels sous Firefox.
pub fn delta_pixels(delta: f64, mode: u32, hauteur_ecran: f64) -> f64 {
    match mode {
        1 => delta * 16.0,
        2 => delta * hauteur_ecran,
        _ => delta,
    }
}

/// La progression d'un élément à travers l'écran : 0 quand son haut touche
/// le bas de l'écran, 1 quand son bas en quitte le haut.
///
/// C'est le `scrub` de ScrollTrigger, réduit à ce qu'il calcule vraiment.
pub fn traversee(haut: f64, hauteur: f64, ecran: f64) -> f64 {
    let course = ecran + hauteur;
    if course <= 0.0 {
        return 0.0;
    }
    borne((ecran - haut) / course, 0.0, 1.0)
}

/// La progression de la page entière, de 0 en haut à 1 en bas.
pub fn progression_page(y: f64, hauteur_page: f64, hauteur_ecran: f64) -> f64 {
    let course = hauteur_page - hauteur_ecran;
    if course <= 0.0 {
        return 0.0;
    }
    borne(y / course, 0.0, 1.0)
}

/// L'élan du défilement, ramené entre −1 et 1.
///
/// Sert aux effets qui réagissent à la vitesse (un bandeau qui s'incline
/// quand on descend vite). Deux mille pixels par seconde, c'est un coup de
/// molette appuyé : au-delà, l'effet sature au lieu de s'emballer.
pub fn elan(vitesse_px_s: f64) -> f64 {
    borne(vitesse_px_s / 2000.0, -1.0, 1.0)
}

/// Le délai d'un élément dans une cascade.
///
/// Plafonné : quand quinze éléments entrent ensemble, le dernier ne doit pas
/// attendre une seconde pour se montrer. Au-delà de `plafond`, tous partent
/// avec le même retard.
pub fn retard(rang: usize, pas_s: f64, plafond: usize) -> f64 {
    rang.min(plafond) as f64 * pas_s
}

/// Le décalage d'un bouton magnétique : il suit le pointeur d'une fraction
/// de la distance à son centre, et pas plus loin que `max` pixels.
pub fn aimant(pointeur: (f64, f64), centre: (f64, f64), force: f64, max: f64) -> (f64, f64) {
    let dx = (pointeur.0 - centre.0) * force;
    let dy = (pointeur.1 - centre.1) * force;
    let d = (dx * dx + dy * dy).sqrt();
    if d <= max || d == 0.0 {
        (dx, dy)
    } else {
        (dx / d * max, dy / d * max)
    }
}

/// Les mots d'un texte, pour les révéler un à un derrière un masque.
///
/// Coupe sur l'espace ordinaire **seulement**. Les espaces insécables du
/// français (« 3,99 € », « Comparez ? ») restent dans le mot : couper là
/// ferait passer le symbole à la ligne, seul, ce que l'insécable existe
/// précisément pour empêcher.
pub fn mots(texte: &str) -> impl Iterator<Item = &str> {
    texte.split([' ', '\n', '\t']).filter(|m| !m.is_empty())
}

/// Un nombre affiché, décomposé pour être compté : ce qui le précède, sa
/// valeur, son nombre de décimales, son séparateur, ce qui le suit.
#[derive(Clone, Debug, PartialEq)]
pub struct Chiffre {
    pub avant: String,
    pub valeur: f64,
    pub decimales: usize,
    pub separateur: char,
    pub apres: String,
}

/// Décompose le premier nombre d'un texte : « 3,99 € » donne 3.99, deux
/// décimales, la virgule, et « € » derrière.
///
/// Rend `None` quand il n'y a rien à compter, ou quand le nombre est nul :
/// compter de zéro à zéro n'anime rien.
pub fn decompose(texte: &str) -> Option<Chiffre> {
    let debut = texte.find(|c: char| c.is_ascii_digit())?;
    let reste = &texte[debut..];
    let mut fin = reste
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(reste.len());
    let mut separateur = '.';
    let mut decimales = 0;
    let mut chars = reste[fin..].chars();
    if let (Some(s @ ('.' | ',')), Some(d)) = (chars.next(), chars.next()) {
        if d.is_ascii_digit() {
            separateur = s;
            let apres_sep = &reste[fin + 1..];
            decimales = apres_sep
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(apres_sep.len());
            fin += 1 + decimales;
        }
    }
    // Deux entiers plutôt qu'un flottant : `str::parse::<f64>` embarque les
    // tables de l'analyseur de flottants de la bibliothèque standard —
    // une dizaine de kilooctets de WebAssembly, mesurés avec `twiggy`, pour
    // lire « 3,99 ».
    let nombre = &reste[..fin];
    let (entier, fraction) = match nombre.split_once(['.', ',']) {
        Some((e, f)) => (e, f),
        None => (nombre, ""),
    };
    let entier: u64 = entier.parse().ok()?;
    let fraction: u64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().ok()?
    };
    let valeur = entier as f64 + fraction as f64 / 10f64.powi(decimales as i32);
    if valeur == 0.0 {
        return None;
    }
    Some(Chiffre {
        avant: texte[..debut].to_string(),
        valeur,
        decimales,
        separateur,
        apres: reste[fin..].to_string(),
    })
}

/// Recompose le texte pour une valeur intermédiaire, au format d'origine.
pub fn compose(c: &Chiffre, v: f64) -> String {
    let nombre = format!("{:.*}", c.decimales, v);
    let nombre = if c.separateur == ',' {
        nombre.replace('.', ",")
    } else {
        nombre
    };
    format!("{}{}{}", c.avant, nombre, c.apres)
}

/// La valeur affichée à l'instant `t` (entre 0 et 1) d'un compteur.
pub fn valeur_comptee(c: &Chiffre, t: f64) -> String {
    compose(c, c.valeur * expo_sortie(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_amortissement_ne_depend_pas_de_la_cadence() {
        // Une image à 60 Hz, ou deux à 120 Hz : le même chemin parcouru.
        let une = amorti(0.1, 1.0 / 60.0);
        let deux = {
            let k = amorti(0.1, 1.0 / 120.0);
            1.0 - (1.0 - k) * (1.0 - k)
        };
        assert!((une - 0.1).abs() < 1e-9, "à 60 Hz on retrouve k : {une}");
        assert!((une - deux).abs() < 1e-9, "{une} ≠ {deux}");
    }

    #[test]
    fn un_onglet_reveille_ne_fait_pas_sauter_la_page() {
        // Après une longue pause, `dt` est énorme : le facteur reste borné,
        // la page glisse au lieu de se téléporter dans un sens imprévu.
        assert!(amorti(0.1, 30.0) <= 1.0);
        assert!(amorti(0.1, -1.0) >= 0.0);
    }

    #[test]
    fn le_defilement_finit_par_se_poser() {
        let (mut y, mut fini) = (0.0, false);
        let mut images = 0;
        while !fini {
            (y, fini) = pas_inertiel(y, 1000.0, 0.1, 1.0 / 60.0);
            images += 1;
            assert!(images < 200, "le défilement ne se pose jamais");
        }
        assert_eq!(y, 1000.0);
        // Ni instantané, ni interminable : la glisse de Lenis dure de
        // l'ordre d'une seconde.
        assert!((40..120).contains(&images), "{images} images");
    }

    #[test]
    fn la_courbe_part_vite_et_se_pose() {
        assert_eq!(expo_sortie(0.0), 0.0);
        assert_eq!(expo_sortie(1.0), 1.0);
        assert!(
            expo_sortie(0.25) > 0.8,
            "les trois quarts du chemin d'emblée"
        );
        assert!(expo_sortie(0.5) < expo_sortie(0.6));
    }

    #[test]
    fn la_molette_se_compte_en_pixels() {
        assert_eq!(delta_pixels(100.0, 0, 800.0), 100.0);
        assert_eq!(delta_pixels(3.0, 1, 800.0), 48.0);
        assert_eq!(delta_pixels(1.0, 2, 800.0), 800.0);
    }

    #[test]
    fn la_traversee_va_de_zero_a_un() {
        // Le haut de l'élément au bas de l'écran : rien n'est encore vu.
        assert_eq!(traversee(900.0, 300.0, 900.0), 0.0);
        // Son bas au haut de l'écran : il est sorti.
        assert_eq!(traversee(-300.0, 300.0, 900.0), 1.0);
        // Au milieu de sa course.
        assert!((traversee(300.0, 300.0, 900.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn une_page_trop_courte_ne_progresse_pas() {
        assert_eq!(progression_page(0.0, 500.0, 900.0), 0.0);
        assert_eq!(progression_page(1100.0, 2000.0, 900.0), 1.0);
    }

    #[test]
    fn la_cascade_est_plafonnee() {
        assert_eq!(retard(0, 0.07, 6), 0.0);
        assert!((retard(2, 0.07, 6) - 0.14).abs() < 1e-9);
        assert_eq!(retard(40, 0.07, 6), retard(6, 0.07, 6));
    }

    #[test]
    fn l_aimant_suit_sans_s_echapper() {
        assert_eq!(aimant((110.0, 100.0), (100.0, 100.0), 0.3, 8.0), (3.0, 0.0));
        let (x, y) = aimant((400.0, 100.0), (100.0, 100.0), 0.3, 8.0);
        assert!((x - 8.0).abs() < 1e-9 && y == 0.0, "borné à 8 px : {x}");
        assert_eq!(aimant((5.0, 5.0), (5.0, 5.0), 0.3, 8.0), (0.0, 0.0));
    }

    #[test]
    fn les_insecables_restent_dans_le_mot() {
        let m: Vec<&str> = mots("Comparez,\u{a0}puis roulez  3,99\u{a0}€").collect();
        assert_eq!(m, ["Comparez,\u{a0}puis", "roulez", "3,99\u{a0}€"]);
    }

    #[test]
    fn un_chiffre_se_decompose_et_se_recompose() {
        for texte in [
            "15",
            "212",
            "3,99 €",
            "3.99 €",
            "3,99\u{a0}€",
            "≈ 222,5 km",
            "x49",
        ] {
            let c = decompose(texte).unwrap_or_else(|| panic!("{texte}"));
            assert_eq!(compose(&c, c.valeur), texte, "aller-retour de {texte:?}");
        }
        let c = decompose("3,99 €").unwrap();
        assert_eq!((c.valeur, c.decimales, c.separateur), (3.99, 2, ','));
        assert_eq!(c.apres, " €");
    }

    #[test]
    fn rien_a_compter_rien_a_animer() {
        assert_eq!(decompose("sans compte"), None);
        assert_eq!(decompose("0 €"), None);
    }

    #[test]
    fn un_point_final_n_est_pas_une_decimale() {
        let c = decompose("42.").unwrap();
        assert_eq!((c.valeur, c.decimales), (42.0, 0));
        assert_eq!(compose(&c, 42.0), "42.");
    }

    #[test]
    fn le_compteur_finit_sur_la_valeur_exacte() {
        let c = decompose("3,99 €").unwrap();
        assert_eq!(valeur_comptee(&c, 1.0), "3,99 €");
        assert_eq!(valeur_comptee(&c, 0.0), "0,00 €");
    }
}
