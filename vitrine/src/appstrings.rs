//! Généré par `frontend/traductions.py` — ne pas modifier à la main.
//!
//! Les libellés que les maquettes reproduisent, dans les trois langues,
//! tirés du même tableau que le catalogue de l'application. Un écran
//! dessiné sur le site dit ainsi exactement ce que l'app affiche.

use crate::contenu::Langue;

/// Les libellés d'une langue, en paires (clé, traduction).
type Table = &'static [(&'static str, &'static str)];

const FR: Table = &[
    ("À pied", "À pied"),
    ("Vélo", "Vélo"),
    ("Voiture", "Voiture"),
    ("Transports", "Transports"),
    ("Démarrer", "Démarrer"),
    ("Le plus simple", "Le plus simple"),
    ("Le plus rapide", "Le plus rapide"),
    ("Le plus court", "Le plus court"),
    ("Essence", "Essence"),
    ("Parking", "Parking"),
    ("Recharge", "Recharge"),
    ("Restaurants", "Restaurants"),
    ("Hôtels", "Hôtels"),
    ("Supermarché", "Supermarché"),
    ("Pharmacie", "Pharmacie"),
    ("Distributeur", "Distributeur"),
    ("Toilettes", "Toilettes"),
    ("Eau potable", "Eau potable"),
    ("Handicapé", "Handicapé"),
    ("Baignade", "Baignade"),
    ("Surf", "Surf"),
    ("Skatepark", "Skatepark"),
    ("Camping", "Camping"),
    ("Point de vue", "Point de vue"),
    ("Fast-food", "Fast-food"),
    ("Tourisme", "Tourisme"),
    ("Sorties", "Sorties"),
    ("Éclaircies", "Éclaircies"),
    ("Boulangerie", "Boulangerie"),
    ("Café", "Café"),
    ("Bars", "Bars"),
    ("Banque", "Banque"),
    ("Poste", "Poste"),
    ("Hôpital", "Hôpital"),
    ("Médecin", "Médecin"),
    ("Vétérinaire", "Vétérinaire"),
    ("Garage", "Garage"),
    ("Lavage auto", "Lavage auto"),
    ("Pique-nique", "Pique-nique"),
    ("Parcs", "Parcs"),
    ("Aire de jeux", "Aire de jeux"),
    ("Gares", "Gares"),
    ("Vélos", "Vélos"),
    ("Marché", "Marché"),
    ("Cinéma", "Cinéma"),
    ("Bibliothèque", "Bibliothèque"),
    ("Salle de sport", "Salle de sport"),
    ("Épicerie", "Épicerie"),
    ("Coiffeur", "Coiffeur"),
    ("Recyclage", "Recyclage"),
    ("Aire de repos", "Aire de repos"),
    ("Sortie %@", "Sortie %@"),
    ("sortie", "sortie"),
    ("Droite", "Droite"),
    ("Quitter", "Quitter"),
    ("%.1f km restants", "%.1f km restants"),
    (
        "%.0f min · %@ · %d virages · arr. %@",
        "%.0f min · %@ · %d virages · arr. %@",
    ),
    (
        "↗ %.0f m de côte · ↘ %.0f m de descente",
        "↗ %.0f m de côte · ↘ %.0f m de descente",
    ),
    ("⚠️ Forte côte (%.0f m)", "⚠️ Forte côte (%.0f m)"),
    ("⛽ ~%.2f € · sans péage", "⛽ ~%.2f € · sans péage"),
    (
        "⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €",
        "⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €",
    ),
];

const ES: Table = &[
    ("À pied", "A pie"),
    ("Vélo", "Bici"),
    ("Voiture", "Coche"),
    ("Transports", "Transporte"),
    ("Démarrer", "Empezar"),
    ("Le plus simple", "La más sencilla"),
    ("Le plus rapide", "La más rápida"),
    ("Le plus court", "La más corta"),
    ("Essence", "Gasolinera"),
    ("Parking", "Aparcamiento"),
    ("Recharge", "Recarga"),
    ("Restaurants", "Restaurantes"),
    ("Hôtels", "Hoteles"),
    ("Supermarché", "Supermercado"),
    ("Pharmacie", "Farmacia"),
    ("Distributeur", "Cajero"),
    ("Toilettes", "Aseos"),
    ("Eau potable", "Agua potable"),
    ("Handicapé", "Accesible"),
    ("Baignade", "Baño"),
    ("Surf", "Surf"),
    ("Skatepark", "Skatepark"),
    ("Camping", "Camping"),
    ("Point de vue", "Mirador"),
    ("Fast-food", "Comida rápida"),
    ("Tourisme", "Turismo"),
    ("Sorties", "Ocio"),
    ("Éclaircies", "Claros"),
    ("Boulangerie", "Panadería"),
    ("Café", "Cafetería"),
    ("Bars", "Bares"),
    ("Banque", "Banco"),
    ("Poste", "Correos"),
    ("Hôpital", "Hospital"),
    ("Médecin", "Médico"),
    ("Vétérinaire", "Veterinario"),
    ("Garage", "Taller"),
    ("Lavage auto", "Lavado de coches"),
    ("Pique-nique", "Pícnic"),
    ("Parcs", "Parques"),
    ("Aire de jeux", "Parque infantil"),
    ("Gares", "Estaciones"),
    ("Vélos", "Bicis"),
    ("Marché", "Mercado"),
    ("Cinéma", "Cine"),
    ("Bibliothèque", "Biblioteca"),
    ("Salle de sport", "Gimnasio"),
    ("Épicerie", "Tienda de barrio"),
    ("Coiffeur", "Peluquería"),
    ("Recyclage", "Reciclaje"),
    ("Aire de repos", "Área de descanso"),
    ("Sortie %@", "Salida %@"),
    ("sortie", "salida"),
    ("Droite", "Derecha"),
    ("Quitter", "Salir"),
    ("%.1f km restants", "%.1f km restantes"),
    (
        "%.0f min · %@ · %d virages · arr. %@",
        "%.0f min · %@ · %d giros · lleg. %@",
    ),
    (
        "↗ %.0f m de côte · ↘ %.0f m de descente",
        "↗ %.0f m de subida · ↘ %.0f m de bajada",
    ),
    ("⚠️ Forte côte (%.0f m)", "⚠️ Subida fuerte (%.0f m)"),
    ("⛽ ~%.2f € · sans péage", "⛽ ~%.2f € · sin peaje"),
    (
        "⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €",
        "⛽ ~%.2f € · 🛣️ peajes ~%.2f € · total ~%.2f €",
    ),
];

const EN: Table = &[
    ("À pied", "On foot"),
    ("Vélo", "Bike"),
    ("Voiture", "Car"),
    ("Transports", "Transit"),
    ("Démarrer", "Start"),
    ("Le plus simple", "The simplest"),
    ("Le plus rapide", "The fastest"),
    ("Le plus court", "The shortest"),
    ("Essence", "Fuel"),
    ("Parking", "Parking"),
    ("Recharge", "Charging"),
    ("Restaurants", "Restaurants"),
    ("Hôtels", "Hotels"),
    ("Supermarché", "Supermarket"),
    ("Pharmacie", "Pharmacy"),
    ("Distributeur", "ATM"),
    ("Toilettes", "Toilets"),
    ("Eau potable", "Drinking water"),
    ("Handicapé", "Accessible"),
    ("Baignade", "Swimming"),
    ("Surf", "Surf"),
    ("Skatepark", "Skatepark"),
    ("Camping", "Campsite"),
    ("Point de vue", "Viewpoint"),
    ("Fast-food", "Fast food"),
    ("Tourisme", "Sightseeing"),
    ("Sorties", "Nightlife"),
    ("Éclaircies", "Partly cloudy"),
    ("Boulangerie", "Bakery"),
    ("Café", "Coffee"),
    ("Bars", "Bars"),
    ("Banque", "Bank"),
    ("Poste", "Post office"),
    ("Hôpital", "Hospital"),
    ("Médecin", "Doctor"),
    ("Vétérinaire", "Vet"),
    ("Garage", "Garage"),
    ("Lavage auto", "Car wash"),
    ("Pique-nique", "Picnic"),
    ("Parcs", "Parks"),
    ("Aire de jeux", "Playground"),
    ("Gares", "Train stations"),
    ("Vélos", "Bikes"),
    ("Marché", "Market"),
    ("Cinéma", "Cinema"),
    ("Bibliothèque", "Library"),
    ("Salle de sport", "Gym"),
    ("Épicerie", "Convenience store"),
    ("Coiffeur", "Hairdresser"),
    ("Recyclage", "Recycling"),
    ("Aire de repos", "Rest area"),
    ("Sortie %@", "Exit %@"),
    ("sortie", "exit"),
    ("Droite", "Right"),
    ("Quitter", "Exit"),
    ("%.1f km restants", "%.1f km left"),
    (
        "%.0f min · %@ · %d virages · arr. %@",
        "%.0f min · %@ · %d turns · arr. %@",
    ),
    (
        "↗ %.0f m de côte · ↘ %.0f m de descente",
        "↗ %.0f m of climb · ↘ %.0f m of descent",
    ),
    ("⚠️ Forte côte (%.0f m)", "⚠️ Steep climb (%.0f m)"),
    ("⛽ ~%.2f € · sans péage", "⛽ ~%.2f € · no tolls"),
    (
        "⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €",
        "⛽ ~%.2f € · 🛣️ tolls ~%.2f € · total ~%.2f €",
    ),
];

/// Le libellé de l'application dans la langue de la page.
///
/// Une clé absente retombe sur elle-même — c'est aussi ce que fait
/// SwiftUI, et cela évite qu'un oubli de traduction laisse un vide.
pub fn app(langue: Langue, cle: &str) -> &str {
    let table = match langue {
        Langue::Fr => FR,
        Langue::Es => ES,
        Langue::En => EN,
    };
    table
        .iter()
        .find(|(k, _)| *k == cle)
        .map(|(_, v)| *v)
        .unwrap_or(cle)
}

/// Le nombre de libellés par langue — ce que les tests vérifient.
pub const NOMBRE: usize = 61;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_trois_langues_ont_le_meme_nombre_de_libelles() {
        assert_eq!(FR.len(), NOMBRE);
        assert_eq!(ES.len(), NOMBRE);
        assert_eq!(EN.len(), NOMBRE);
    }

    #[test]
    fn les_trois_langues_ont_les_memes_cles_dans_le_meme_ordre() {
        // Les tables sont parcourues par clé : un décalage passerait
        // inaperçu jusqu'à ce qu'un mot espagnol s'affiche en anglais.
        for i in 0..NOMBRE {
            assert_eq!(FR[i].0, ES[i].0);
            assert_eq!(FR[i].0, EN[i].0);
        }
    }

    #[test]
    fn une_cle_inconnue_retombe_sur_elle_meme() {
        assert_eq!(app(Langue::Es, "Rue de la Paix"), "Rue de la Paix");
    }

    #[test]
    fn les_libelles_connus_sont_bien_traduits() {
        assert_eq!(app(Langue::Fr, "Voiture"), "Voiture");
        assert_ne!(app(Langue::Es, "Voiture"), "Voiture");
        assert_ne!(app(Langue::En, "Voiture"), "Voiture");
    }
}
