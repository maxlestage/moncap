//! Remplit les formats de l'application avec des valeurs d'exemple.
//!
//! Les maquettes n'écrivent pas de phrases toutes faites : elles reprennent
//! les formats du catalogue iOS et y versent des nombres. Un écran dessiné
//! sur le site dit donc exactement ce que l'application affiche, jusqu'à la
//! ponctuation — et le jour où une traduction change, elle change pour les
//! deux à la fois.
//!
//! Volontairement minimal : seuls les spécificateurs employés par
//! l'application sont reconnus — `%.0f`, `%.1f`, `%.2f`, `%d`, `%@` — dans
//! l'ordre où ils apparaissent. Il ne s'agit pas de réimplémenter printf
//! mais de verser quatre valeurs dans une phrase déjà traduite.

use crate::appstrings::app;
use crate::contenu::Langue;

/// Une valeur à verser dans un format.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Valeur {
    Nombre(f64),
    Texte(&'static str),
}

impl From<f64> for Valeur {
    fn from(v: f64) -> Self {
        Valeur::Nombre(v)
    }
}

impl From<u32> for Valeur {
    fn from(v: u32) -> Self {
        Valeur::Nombre(v as f64)
    }
}

impl From<&'static str> for Valeur {
    fn from(v: &'static str) -> Self {
        Valeur::Texte(v)
    }
}

/// Remplit un modèle à la façon de `String(format:)`.
///
/// Les décimales gardent le **point**, parce que `String(format:)` sans
/// locale fait de même : l'application écrit « 24.3 km » y compris en
/// français, et une maquette qui écrirait « 24,3 » mentirait sur ce
/// détail-là.
///
/// Un spécificateur sans valeur correspondante est laissé tel quel : mieux
/// vaut voir `%@` à l'écran et comprendre qu'il manque un argument que de
/// lire une phrase amputée dont rien ne dit qu'elle est fausse.
pub fn format(modele: &str, valeurs: &[Valeur]) -> String {
    let mut sortie = String::with_capacity(modele.len() + 16);
    let mut reste = modele;
    let mut i = 0;

    while let Some(pos) = reste.find('%') {
        sortie.push_str(&reste[..pos]);
        let apres = &reste[pos..];

        let (largeur, rendu) = match lire_specificateur(apres) {
            Some(v) => v,
            None => {
                // Pas un spécificateur connu (un « % » isolé, ou « 100 % ») :
                // on le recopie et on avance d'un caractère pour ne pas
                // boucler indéfiniment dessus.
                sortie.push('%');
                reste = &apres[1..];
                continue;
            }
        };

        match valeurs.get(i) {
            Some(v) => sortie.push_str(&rendu(*v)),
            None => sortie.push_str(&apres[..largeur]),
        }
        i += 1;
        reste = &apres[largeur..];
    }
    sortie.push_str(reste);
    sortie
}

/// Comment écrire une valeur : la fonction que le spécificateur désigne.
type Rendu = fn(Valeur) -> String;

/// Un spécificateur reconnu : sa longueur en octets, et de quoi l'écrire.
type Specificateur = (usize, Rendu);

/// Reconnaît un spécificateur en tête de `s`.
fn lire_specificateur(s: &str) -> Option<Specificateur> {
    let o = s.as_bytes();
    if o.len() >= 2 && o[0] == b'%' {
        match o[1] {
            b'd' => return Some((2, rend_entier)),
            b'@' => return Some((2, rend_texte)),
            _ => {}
        }
    }
    // `%.Nf`, N d'un seul chiffre : c'est tout ce que l'application emploie.
    if o.len() >= 4 && o[0] == b'%' && o[1] == b'.' && o[2].is_ascii_digit() && o[3] == b'f' {
        return Some((
            4,
            match o[2] {
                b'0' => rend_f0,
                b'1' => rend_f1,
                _ => rend_f2,
            },
        ));
    }
    None
}

fn rend_entier(v: Valeur) -> String {
    match v {
        Valeur::Nombre(n) => format!("{}", n.round() as i64),
        Valeur::Texte(t) => t.to_string(),
    }
}

fn rend_texte(v: Valeur) -> String {
    match v {
        Valeur::Nombre(n) => format!("{n}"),
        Valeur::Texte(t) => t.to_string(),
    }
}

fn rend_f0(v: Valeur) -> String {
    match v {
        Valeur::Nombre(n) => format!("{n:.0}"),
        Valeur::Texte(t) => t.to_string(),
    }
}

fn rend_f1(v: Valeur) -> String {
    match v {
        Valeur::Nombre(n) => format!("{n:.1}"),
        Valeur::Texte(t) => t.to_string(),
    }
}

fn rend_f2(v: Valeur) -> String {
    match v {
        Valeur::Nombre(n) => format!("{n:.2}"),
        Valeur::Texte(t) => t.to_string(),
    }
}

/// Applique `format` à un libellé traduit de l'application.
pub fn app_format(langue: Langue, cle: &str, valeurs: &[Valeur]) -> String {
    format(app(langue, cle), valeurs)
}

#[cfg(test)]
mod tests {
    use super::Valeur::{Nombre, Texte};
    use super::*;

    #[test]
    fn les_decimales_gardent_le_point() {
        // La raison d'être de cette fonction : `String(format:)` sans locale
        // écrit « 24.3 », y compris en français. Mettre une virgule ici
        // ferait mentir la maquette sur ce que l'app affiche vraiment.
        assert_eq!(format("%.1f km", &[Nombre(24.3)]), "24.3 km");
        assert_eq!(format("~%.2f €", &[Nombre(2.6)]), "~2.60 €");
    }

    #[test]
    fn le_format_du_comparateur_se_remplit_dans_l_ordre() {
        assert_eq!(
            format(
                "%.0f min · %@ · %d virages · arr. %@",
                &[Nombre(28.0), Texte("24.3 km"), Nombre(6.0), Texte("18:42")]
            ),
            "28 min · 24.3 km · 6 virages · arr. 18:42"
        );
    }

    #[test]
    fn les_entiers_s_arrondissent() {
        assert_eq!(format("%d", &[Nombre(5.6)]), "6");
        assert_eq!(format("%.0f", &[Nombre(5.6)]), "6");
        assert_eq!(format("%.0f", &[Nombre(5.4)]), "5");
    }

    /// Un pourcentage littéral ne doit pas être pris pour un spécificateur,
    /// et surtout ne doit pas faire tourner la boucle sans fin.
    #[test]
    fn un_pourcent_isole_est_recopie() {
        assert_eq!(format("62 % installés", &[]), "62 % installés");
        assert_eq!(format("100 %", &[]), "100 %");
    }

    /// Mieux vaut voir qu'il manque une valeur que lire une phrase amputée.
    #[test]
    fn un_specificateur_sans_valeur_reste_visible() {
        assert_eq!(format("%@ et %@", &[Texte("un")]), "un et %@");
    }

    #[test]
    fn les_valeurs_en_trop_sont_ignorees() {
        assert_eq!(format("%@", &[Texte("un"), Texte("deux")]), "un");
    }

    #[test]
    fn un_modele_sans_specificateur_ne_change_pas() {
        assert_eq!(format("Démarrer", &[Nombre(1.0)]), "Démarrer");
    }

    /// Les formats de l'app contiennent des emoji et des accents : découper
    /// une chaîne UTF-8 à l'octet près la casserait, et Rust paniquerait.
    #[test]
    fn les_emoji_et_accents_traversent_sans_dommage() {
        assert_eq!(
            format(
                "⛽ ~%.2f € · 🛣️ péages ~%.2f €",
                &[Nombre(3.4), Nombre(1.9)]
            ),
            "⛽ ~3.40 € · 🛣️ péages ~1.90 €"
        );
        assert_eq!(
            format("↗ %.0f m de côte", &[Nombre(180.0)]),
            "↗ 180 m de côte"
        );
    }

    #[test]
    fn le_format_passe_par_la_traduction() {
        // En espagnol, le libellé change ; le nombre, non.
        let fr = app_format(Langue::Fr, "%.1f km restants", &[Nombre(8.4)]);
        let es = app_format(Langue::Es, "%.1f km restants", &[Nombre(8.4)]);
        assert!(fr.contains("8.4"));
        assert!(es.contains("8.4"));
        assert_ne!(fr, es);
    }
}
