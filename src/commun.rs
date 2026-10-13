//! Petits outils partagés par les routes.

use crate::erreur::{Erreur, Resultat};

/// Longueur maximale d'une étiquette (en caractères).
pub const MAX_LABEL: usize = 120;

/// Des coordonnées finies, sur le globe.
pub fn coord_valide(lat: f64, lon: f64) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
}

/// 400 `coordonnées invalides` si elles ne tiennent pas sur le globe.
pub fn verifie_coord(lat: f64, lon: f64) -> Resultat<()> {
    if coord_valide(lat, lon) {
        Ok(())
    } else {
        Err(Erreur::requete("coordonnées invalides"))
    }
}

/// Une étiquette sans espaces autour, coupée à 120 caractères.
pub fn nettoie_label(s: &str) -> String {
    s.trim().chars().take(MAX_LABEL).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coord_validation() {
        assert!(coord_valide(48.85, 2.35));
        assert!(coord_valide(-90.0, 180.0));
        assert!(!coord_valide(91.0, 0.0));
        assert!(!coord_valide(0.0, -181.0));
        assert!(!coord_valide(f64::NAN, 0.0));
        assert!(!coord_valide(0.0, f64::INFINITY));
    }

    #[test]
    fn label_is_trimmed_and_capped() {
        assert_eq!(nettoie_label("  Paris  "), "Paris");
        assert_eq!(nettoie_label(&"é".repeat(200)).chars().count(), MAX_LABEL);
    }
}
