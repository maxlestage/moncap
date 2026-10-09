//! La géographie du client : distances, caps, et la règle d'envoi de la
//! position en direct. Fonctions pures, testées sans navigateur.

use crate::types::Coord;

/// Rayon terrestre moyen, en kilomètres — celui du serveur.
const RAYON_KM: f64 = 6371.0;

/// Distance à vol d'oiseau, en kilomètres (formule de Haversine).
pub fn distance_km(a: Coord, b: Coord) -> f64 {
    let d_lat = (b.lat - a.lat).to_radians();
    let d_lon = (b.lon - a.lon).to_radians();
    let la1 = a.lat.to_radians();
    let la2 = b.lat.to_radians();
    let h = (d_lat / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * RAYON_KM * h.sqrt().asin()
}

/// Le cap initial de `a` vers `b`, en degrés (0 = nord, sens horaire).
pub fn cap(a: Coord, b: Coord) -> f64 {
    let d_lon = (b.lon - a.lon).to_radians();
    let la1 = a.lat.to_radians();
    let la2 = b.lat.to_radians();
    let y = d_lon.sin() * la2.cos();
    let x = la1.cos() * la2.sin() - la1.sin() * la2.cos() * d_lon.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Le point cardinal d'un cap, en français.
pub fn boussole(cap_deg: f64) -> &'static str {
    const POINTS: [&str; 8] = [
        "Nord",
        "Nord-Est",
        "Est",
        "Sud-Est",
        "Sud",
        "Sud-Ouest",
        "Ouest",
        "Nord-Ouest",
    ];
    let i = ((cap_deg.rem_euclid(360.0) / 45.0).round() as usize) % 8;
    POINTS[i]
}

/// Une distance écrite comme la route se parle : des mètres sous le
/// kilomètre, des kilomètres au-delà.
///
/// La règle de l'app iOS (`Distance.courte`), mot pour mot : le seuil porte
/// sur la valeur **arrondie** — 999,6 m s'écrit « 1.0 km » et non
/// « 1000 m » —, et le point décimal est celui de l'app.
pub fn distance_courte(metres: f64) -> String {
    let m = metres.max(0.0);
    if m.round() < 1000.0 {
        format!("{m:.0} m")
    } else {
        format!("{:.1} km", m / 1000.0)
    }
}

/// Une durée en minutes : « 42 min », « 2 h 05 ».
pub fn duree_courte(minutes: f64) -> String {
    let total = minutes.max(0.0).round() as u64;
    if total < 60 {
        format!("{total} min")
    } else {
        format!("{} h {:02}", total / 60, total % 60)
    }
}

/// Le battement de présence : même immobile, on redit où l'on est.
///
/// Les autres oublient une voiture muette au bout de quinze secondes, et le
/// serveur ne garde pas les positions : un conducteur arrêté disparaissait
/// donc de la carte des autres, et celui qui arrivait ne voyait jamais ceux
/// qui étaient déjà là sans bouger. L'ancien front avait ce défaut.
pub const BATTEMENT_MS: f64 = 5000.0;

/// Faut-il envoyer cette position au temps réel ?
///
/// Le GPS d'un téléphone répond plusieurs fois par seconde ; tout relayer
/// inonderait le WebSocket pour des déplacements invisibles sur la carte.
/// On envoie la première, puis au plus une toutes les 1,5 s si l'on a bougé
/// d'au moins huit mètres — et, immobile, une toutes les cinq secondes.
pub fn doit_envoyer(precedent: Option<(f64, Coord)>, maintenant_ms: f64, ici: Coord) -> bool {
    let Some((t, avant)) = precedent else {
        return true;
    };
    let ecoule = maintenant_ms - t;
    ecoule >= BATTEMENT_MS || (ecoule >= 1500.0 && distance_km(avant, ici) * 1000.0 >= 8.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARIS: Coord = Coord {
        lat: 48.8566,
        lon: 2.3522,
    };
    const LYON: Coord = Coord {
        lat: 45.7640,
        lon: 4.8357,
    };

    #[test]
    fn paris_lyon_comme_le_serveur() {
        // Le chiffre du README du serveur, pour la même formule.
        assert!((distance_km(PARIS, LYON) - 391.5).abs() < 1.0);
    }

    #[test]
    fn lyon_est_au_sud_est_de_paris() {
        let c = cap(PARIS, LYON);
        assert!((c - 150.5).abs() < 1.0, "{c}");
        assert_eq!(boussole(c), "Sud-Est");
    }

    #[test]
    fn la_boussole_boucle() {
        assert_eq!(boussole(0.0), "Nord");
        assert_eq!(boussole(359.0), "Nord");
        assert_eq!(boussole(-90.0), "Ouest");
        assert_eq!(boussole(270.0), "Ouest");
    }

    #[test]
    fn des_metres_sous_le_kilometre() {
        assert_eq!(distance_courte(400.0), "400 m");
        assert_eq!(distance_courte(999.4), "999 m");
        // Arrondi à 1000 : c'est un kilomètre, et il s'écrit comme tel.
        assert_eq!(distance_courte(999.6), "1.0 km");
        assert_eq!(distance_courte(1234.0), "1.2 km");
        assert_eq!(distance_courte(-5.0), "0 m");
    }

    #[test]
    fn les_durees_passent_aux_heures() {
        assert_eq!(duree_courte(42.4), "42 min");
        assert_eq!(duree_courte(125.0), "2 h 05");
    }

    #[test]
    fn la_position_part_sans_inonder() {
        let t0 = 10_000.0;
        assert!(doit_envoyer(None, t0, PARIS), "la première part toujours");
        let avant = Some((t0, PARIS));
        // Trop tôt, même en ayant bougé.
        let loin = Coord {
            lat: PARIS.lat + 0.001,
            lon: PARIS.lon,
        };
        assert!(!doit_envoyer(avant, t0 + 500.0, loin));
        // Assez tard, mais immobile (moins de huit mètres).
        let pres = Coord {
            lat: PARIS.lat + 0.00003,
            lon: PARIS.lon,
        };
        assert!(!doit_envoyer(avant, t0 + 3000.0, pres));
        // Immobile, mais assez longtemps pour redire qu'on est là.
        assert!(doit_envoyer(avant, t0 + BATTEMENT_MS, pres));
        // Assez tard et assez loin.
        assert!(doit_envoyer(avant, t0 + 1600.0, loin));
    }
}
