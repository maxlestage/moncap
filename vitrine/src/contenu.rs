//! Le contenu de la vitrine : trois langues, et rien d'autre.
//!
//! Écrit une fois en Rust et compilé dans les deux cibles — le WebAssembly
//! qui rend la page, et le binaire natif qui fabrique les coquilles HTML.
//! C'est ce que le passage à Rust achète : le texte n'existe qu'ici, et le
//! compilateur refuse qu'une langue oublie un champ que les autres ont.
//!
//! Tout est `&'static str` : aucune allocation, aucun analyseur embarqué,
//! rien à faire au démarrage. Le texte est dans le binaire, point.
//!
//! Issu d'une conversion mécanique depuis l'ancien `langues.ts`
//! (`outils/ts_vers_rust.py`), puis relu. Le TypeScript n'existe plus.

/// Une section de la page : un titre, un paragraphe d'introduction.
#[derive(Clone, Copy, PartialEq)]
pub struct Section {
    pub titre: &'static str,
    pub intro: &'static str,
}

/// Le comparateur a un paragraphe de plus, après la maquette.
#[derive(Clone, Copy, PartialEq)]
pub struct SectionApres {
    pub titre: &'static str,
    pub intro: &'static str,
    pub apres: &'static str,
}

/// Un chiffre du bandeau. `valeur` est une chaîne et non un nombre : il y a
/// « 3,99 € » parmi eux, et la virgule décimale change selon la langue.
#[derive(Clone, Copy, PartialEq)]
pub struct Chiffre {
    pub valeur: &'static str,
    pub legende: &'static str,
}

/// Une carte d'atout : un emoji, un titre, un texte.
#[derive(Clone, Copy, PartialEq)]
pub struct Atout {
    pub emoji: &'static str,
    pub titre: &'static str,
    pub texte: &'static str,
}

/// Une formule tarifaire.
#[derive(Clone, Copy, PartialEq)]
pub struct Formule {
    pub nom: &'static str,
    pub prix: &'static str,
    pub periode: &'static str,
    pub points: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq)]
pub struct Tarifs {
    pub titre: &'static str,
    pub intro: &'static str,
    pub plus: Formule,
    pub pro: Formule,
    pub note: &'static str,
}

/// La mesure mise en avant, versée au milieu d'une phrase.
#[derive(Clone, Copy, PartialEq)]
pub struct Preuve {
    pub titre: &'static str,
    pub texte: &'static str,
    pub mesure: &'static str,
    pub note: &'static str,
}

/// Un domaine technique et ses outils.
#[derive(Clone, Copy, PartialEq)]
pub struct Pile {
    pub domaine: &'static str,
    pub outils: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq)]
pub struct Legendes {
    pub itineraires: &'static str,
    pub guidage: &'static str,
    pub montre: &'static str,
    pub lieux: &'static str,
    pub regions: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Chevrons {
    pub descendre: &'static str,
    pub remonter: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Theme {
    pub libelle: &'static str,
    pub noms: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq)]
pub struct Maquette {
    pub installee: &'static str,
}

/// Une ancre du pied de page.
#[derive(Clone, Copy, PartialEq)]
pub struct Ancre {
    pub id: &'static str,
    pub nom: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Pied {
    pub slogan: &'static str,
    pub decouvrir: &'static str,
    pub ancres: &'static [Ancre],
    pub donnees: &'static str,
    pub legal_titre: &'static str,
    pub copyright: &'static str,
}

/// Une section d'une page légale : un titre, des paragraphes.
#[derive(Clone, Copy, PartialEq)]
pub struct SectionLegale {
    pub titre: &'static str,
    pub corps: &'static [&'static str],
}

/// Une page légale entière.
#[derive(Clone, Copy, PartialEq)]
pub struct PageLegale {
    pub chemin: &'static str,
    pub titre: &'static str,
    pub description: &'static str,
    pub retour: &'static str,
    pub sections: &'static [SectionLegale],
}

/// Tout le texte d'une langue.
#[derive(Clone, Copy, PartialEq)]
pub struct Contenu {
    pub code: &'static str,
    pub chemin: &'static str,
    pub nom: &'static str,
    pub titre: &'static str,
    pub description: &'static str,
    pub h1: &'static [&'static str],
    pub chapeau: &'static str,
    pub se_connecter: &'static str,
    pub chevrons: Chevrons,
    pub theme: Theme,
    pub langue_libelle: &'static str,
    pub chiffres: &'static [Chiffre],
    pub comparateur: SectionApres,
    pub guidage: Section,
    pub montre: Section,
    pub lieux: Section,
    pub hors_ligne: Section,
    pub atouts_titre: &'static str,
    pub atouts: &'static [Atout],
    pub hors_ligne_cartes: &'static [Atout],
    pub tarifs: Tarifs,
    pub preuve: Preuve,
    pub pile_titre: &'static str,
    pub pile: &'static [Pile],
    pub franchise_titre: &'static str,
    pub franchise: &'static [&'static str],
    pub legendes: Legendes,
    pub reste: &'static str,
    pub maquette: Maquette,
    pub signature: &'static str,
    pub credits: &'static str,
    pub pied: Pied,
    pub mentions: PageLegale,
    pub confidentialite: PageLegale,
}

/// Les trois langues de la vitrine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Langue {
    Fr,
    Es,
    En,
}

impl Langue {
    pub const TOUTES: [Langue; 3] = [Langue::Fr, Langue::Es, Langue::En];

    /// Le contenu de cette langue.
    pub const fn contenu(self) -> &'static Contenu {
        match self {
            Langue::Fr => &FR,
            Langue::Es => &ES,
            Langue::En => &EN,
        }
    }

    /// Le code BCP 47, tel qu'il part dans `lang` et `hreflang`.
    pub const fn code(self) -> &'static str {
        self.contenu().code
    }

    /// Le chemin de la page d'accueil de cette langue.
    pub const fn chemin(self) -> &'static str {
        self.contenu().chemin
    }

    /// Reconnaît une langue à son code, pour la lire depuis la page.
    pub fn depuis_code(code: &str) -> Option<Langue> {
        Langue::TOUTES.into_iter().find(|l| l.code() == code)
    }
}

// --- Ce qui ne se traduit pas ---------------------------------------

/// Le plafond annoncé partout : le bandeau de chiffres le cite, et la
/// maquette ne doit jamais en montrer davantage.
pub const ITINERAIRES_MAX: u32 = 15;

pub const AUTEUR: &str = "Maxime Nathan Lestage";
pub const CONTACT: &str = "maxlestage@icloud.com";
pub const CONNEXION: &str = "/app";
pub const MODE_ACTIF: &str = "Voiture";

/// Les quatre modes de déplacement, dans l'ordre du sélecteur.
pub const MODES: &[&str] = &["À pied", "Vélo", "Voiture", "Transports"];

/// La teinte d'une étiquette d'itinéraire.
///
/// Une énumération et non une chaîne : le TypeScript écrivait
/// `"verte" | "bleue" | "orange"`, ce que Rust sait dire pour de bon.
/// Une quatrième couleur devient une erreur de compilation, plus une
/// classe CSS absente qu'on découvre à l'écran.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Teinte {
    Verte,
    Bleue,
    Orange,
}

impl Teinte {
    /// Le nom de la classe CSS, inchangé depuis la feuille existante.
    pub const fn classe(self) -> &'static str {
        match self {
            Teinte::Verte => "verte",
            Teinte::Bleue => "bleue",
            Teinte::Orange => "orange",
        }
    }
}

/// Un itinéraire d'exemple pour la maquette du comparateur.
///
/// Le TypeScript rangeait ces valeurs dans des tuples — `trajet` mêlait
/// deux nombres et deux chaînes, `cout` avait une ou trois cases selon
/// qu'il y a des péages. Des champs nommés disent la même chose sans
/// qu'il faille se souvenir d'un ordre, et `Option` dit mieux
/// « il n'y en a pas » qu'une longueur de tableau.
///
/// Le total des coûts n'est pas stocké : il valait exactement la somme
/// du carburant et des péages, et deux façons d'écrire le même nombre
/// finissent toujours par diverger. Voir `total()`.
#[derive(Clone, Copy, PartialEq)]
pub struct Itineraire {
    pub etiquette: &'static str,
    pub teinte: Teinte,
    pub minutes: u32,
    /// Déjà mise en forme : l'application écrit des mètres sous le
    /// kilomètre, et « 0.8 km » n'est pas ce qu'elle dit.
    pub distance: &'static str,
    pub virages: u32,
    pub arrivee: &'static str,
    /// Côte puis descente, en mètres.
    pub denivele: Option<(u32, u32)>,
    pub carburant: f64,
    /// Seulement quand l'itinéraire en emprunte.
    pub peages: Option<f64>,
    /// Mètres de côte, quand elle mérite un avertissement.
    pub avertissement: Option<u32>,
}

impl Itineraire {
    /// Ce que coûte le trajet, carburant et péages compris.
    pub fn total(&self) -> f64 {
        self.carburant + self.peages.unwrap_or(0.0)
    }
}

pub const MAQUETTE_ITINERAIRES: &[Itineraire] = &[
    Itineraire {
        etiquette: "Le plus simple",
        teinte: Teinte::Verte,
        minutes: 28,
        distance: "24.3 km",
        virages: 6,
        arrivee: "18:42",
        denivele: None,
        carburant: 2.6,
        peages: None,
        avertissement: None,
    },
    Itineraire {
        etiquette: "Le plus rapide",
        teinte: Teinte::Bleue,
        minutes: 22,
        distance: "31.8 km",
        virages: 11,
        arrivee: "18:36",
        denivele: None,
        carburant: 3.4,
        peages: Some(1.9),
        avertissement: None,
    },
    Itineraire {
        etiquette: "Le plus court",
        teinte: Teinte::Orange,
        minutes: 34,
        distance: "21.7 km",
        virages: 19,
        arrivee: "18:48",
        denivele: Some((180, 165)),
        carburant: 2.3,
        peages: None,
        avertissement: Some(180),
    },
];

/// Un lieu d'une catégorie, pour la maquette.
#[derive(Clone, Copy, PartialEq)]
pub struct Lieu {
    pub emoji: &'static str,
    pub nom: &'static str,
}

/// Les 42 catégories annoncées par le bandeau de chiffres.
pub const LIEUX: &[Lieu] = &[
    Lieu {
        emoji: "⛽",
        nom: "Essence",
    },
    Lieu {
        emoji: "🅿️",
        nom: "Parking",
    },
    Lieu {
        emoji: "⚡",
        nom: "Recharge",
    },
    Lieu {
        emoji: "🍽️",
        nom: "Restaurants",
    },
    Lieu {
        emoji: "🏨",
        nom: "Hôtels",
    },
    Lieu {
        emoji: "🛒",
        nom: "Supermarché",
    },
    Lieu {
        emoji: "💊",
        nom: "Pharmacie",
    },
    Lieu {
        emoji: "🏧",
        nom: "Distributeur",
    },
    Lieu {
        emoji: "🚻",
        nom: "Toilettes",
    },
    Lieu {
        emoji: "🚰",
        nom: "Eau potable",
    },
    Lieu {
        emoji: "♿",
        nom: "Handicapé",
    },
    Lieu {
        emoji: "🏊",
        nom: "Baignade",
    },
    Lieu {
        emoji: "🏄",
        nom: "Surf",
    },
    Lieu {
        emoji: "🛹",
        nom: "Skatepark",
    },
    Lieu {
        emoji: "🏕️",
        nom: "Camping",
    },
    Lieu {
        emoji: "⛰️",
        nom: "Point de vue",
    },
    Lieu {
        emoji: "🍔",
        nom: "Fast-food",
    },
    Lieu {
        emoji: "🏛️",
        nom: "Tourisme",
    },
    Lieu {
        emoji: "🎉",
        nom: "Sorties",
    },
    Lieu {
        emoji: "🥖",
        nom: "Boulangerie",
    },
    Lieu {
        emoji: "☕",
        nom: "Café",
    },
    Lieu {
        emoji: "🍺",
        nom: "Bars",
    },
    Lieu {
        emoji: "🏦",
        nom: "Banque",
    },
    Lieu {
        emoji: "📮",
        nom: "Poste",
    },
    Lieu {
        emoji: "🏥",
        nom: "Hôpital",
    },
    Lieu {
        emoji: "🩺",
        nom: "Médecin",
    },
    Lieu {
        emoji: "🐾",
        nom: "Vétérinaire",
    },
    Lieu {
        emoji: "🔧",
        nom: "Garage",
    },
    Lieu {
        emoji: "🧽",
        nom: "Lavage auto",
    },
    Lieu {
        emoji: "🧺",
        nom: "Pique-nique",
    },
    Lieu {
        emoji: "🌳",
        nom: "Parcs",
    },
    Lieu {
        emoji: "🛝",
        nom: "Aire de jeux",
    },
    Lieu {
        emoji: "🚉",
        nom: "Gares",
    },
    Lieu {
        emoji: "🚲",
        nom: "Vélos",
    },
    Lieu {
        emoji: "🍅",
        nom: "Marché",
    },
    Lieu {
        emoji: "🎬",
        nom: "Cinéma",
    },
    Lieu {
        emoji: "📚",
        nom: "Bibliothèque",
    },
    Lieu {
        emoji: "🏋️",
        nom: "Salle de sport",
    },
    Lieu {
        emoji: "🏪",
        nom: "Épicerie",
    },
    Lieu {
        emoji: "💇",
        nom: "Coiffeur",
    },
    Lieu {
        emoji: "♻️",
        nom: "Recyclage",
    },
    Lieu {
        emoji: "💤",
        nom: "Aire de repos",
    },
];

/// Une région téléchargeable, avec son poids.
#[derive(Clone, Copy, PartialEq)]
pub struct Region {
    pub nom: &'static str,
    pub poids: &'static str,
    /// Pourcentage téléchargé. `None` quand elle est déjà installée —
    /// ce que le TypeScript disait par l'absence du champ.
    pub avancement: Option<u32>,
}

pub const MAQUETTE_REGIONS: &[Region] = &[
    Region {
        nom: "France · Nouvelle-Aquitaine",
        poids: "499 Mo",
        avancement: None,
    },
    Region {
        nom: "France · Zone Paris–Méditerranée",
        poids: "1,7 Go",
        avancement: None,
    },
    Region {
        nom: "Spain · Cataluña",
        poids: "320 Mo",
        avancement: Some(62),
    },
    Region {
        nom: "Monaco",
        poids: "1,1 Mo",
        avancement: None,
    },
];

/// Les outils, par domaine — affichés dans « Ce qu'il y a dessous ».
pub const OUTILS: &[&[&str]] = &[
    &[
        "Swift",
        "SwiftUI",
        "MapKit",
        "MapLibre",
        "Core Location",
        "AVFoundation",
        "WatchConnectivity",
    ],
    &["Rust", "Axum", "Tokio", "SeaORM", "PostgreSQL", "Redis"],
    &[
        "Valhalla",
        "OpenStreetMap",
        "Overpass",
        "Osmium",
        "PMTiles",
        "IGN",
    ],
    &[
        "TomTom",
        "open-meteo",
        "NASA FIRMS",
        "data.economie.gouv.fr",
    ],
    &["React", "TypeScript", "Bun"],
    &["Heroku", "GitHub Actions", "Docker"],
];

/// Une source de données, créditée en pied de page.
#[derive(Clone, Copy, PartialEq)]
pub struct Source {
    pub nom: &'static str,
    pub url: &'static str,
}

pub const SOURCES: &[Source] = &[
    Source {
        nom: "OpenStreetMap",
        url: "https://www.openstreetmap.org/copyright",
    },
    Source {
        nom: "Valhalla",
        url: "https://valhalla.github.io/valhalla/",
    },
    Source {
        nom: "Transitous",
        url: "https://transitous.org",
    },
    Source {
        nom: "Open-Meteo",
        url: "https://open-meteo.com",
    },
    Source {
        nom: "NASA FIRMS",
        url: "https://firms.modaps.eosdis.nasa.gov",
    },
    Source {
        nom: "data.economie.gouv.fr",
        url: "https://data.economie.gouv.fr",
    },
    Source {
        nom: "IGN",
        url: "https://www.ign.fr",
    },
    Source {
        nom: "TomTom",
        url: "https://www.tomtom.com",
    },
];

// --- Français --------------------------------------------------------

pub const FR: Contenu = Contenu {
    code: "fr",
    chemin: "/",
    nom: "Français",
    titre: "MonCap GPS — comparez, puis roulez, avec ou sans réseau",
    description: "Un GPS iOS qui compare jusqu'à 15 itinéraires — durée, virages, dénivelé, carburant, péages — puis vous guide. 212 régions d'Europe téléchargeables pour rouler — et pédaler, et marcher — sans réseau. Trams et bus compris.",
    h1: &["Comparez, puis roulez", "avec ou sans réseau"],
    chapeau: "Un GPS pour iPhone qui met les itinéraires côte à côte — durée, virages, dénivelé, carburant, péages — puis vous guide. Téléchargez votre région une fois, et il continue de calculer tout seul : vallées, tunnels, étranger.",
    se_connecter: "Se connecter",
    chevrons: Chevrons { descendre: "Section suivante", remonter: "Remonter en haut" },
    theme: Theme { libelle: "Thème", noms: &["Auto", "Clair", "Sombre", "Orange"] },
    langue_libelle: "Langue",
    chiffres: &[
        Chiffre { valeur: "15", legende: "itinéraires au maximum" },
        Chiffre { valeur: "42", legende: "catégories de lieux" },
        Chiffre { valeur: "212", legende: "régions téléchargeables" },
        Chiffre { valeur: "49", legende: "pays d'Europe" },
        Chiffre { valeur: "3,99 €", legende: "par mois après 14 jours d'essai" },
    ],
    comparateur: SectionApres {
        titre: "Le trajet vous est présenté, pas imposé",
        intro: "Jusqu'à 15 itinéraires côte à côte, chacun étiqueté le plus simple, le plus rapide ou le plus court. Pour chacun : la durée, la distance, le nombre de virages, l'heure d'arrivée, le dénivelé — et ce que ça coûte.",
        apres: "Le carburant est calculé avec la consommation de votre véhicule et le prix réel des stations autour de vous (open data officiel), pas une moyenne nationale. Les péages s'ajoutent quand l'itinéraire en emprunte. Une forte côte est signalée avant le départ, pas découverte au milieu. À pied, à vélo ou en voiture, et l'on peut demander d'éviter péages ou autoroutes. À vélo, on dit même lequel — route, ville ou VTT — et s'il faut fuir les côtes.",
    },
    guidage: Section {
        titre: "Puis il vous emmène",
        intro: "La flèche suit la manœuvre réelle, la distance au prochain virage se met à jour, et le numéro de sortie s'affiche sous la consigne. À l'approche d'une intersection, les voies à prendre s'allument. La vitesse du moment est comparée à la limite de la route, lue sur OpenStreetMap : elle passe au rouge quand vous la dépassez. En bas, l'heure d'arrivée vit à côté des minutes restantes, et les commandes de la carte tiennent dans un seul panneau — dégagé de tout ce qui ne sert pas au volant.",
    },
    montre: Section {
        titre: "Et au poignet",
        intro: "La montre a sa propre application MonCap — pas un simple reflet de l'iPhone. Elle s'installe avec l'app, se pose en complication sur le cadran, et s'ouvre d'un toucher : la manœuvre en grand, la distance, le restant et l'heure d'arrivée, plein écran. Un tapotement à chaque nouvelle consigne — on sent le virage venir sans regarder l'écran. Le dernier état envoyé survit à la fermeture : on lève le poignet, la consigne est déjà là.",
    },
    lieux: Section {
        titre: "Ce qu'il y a autour",
        intro: "Un bandeau de catégories au-dessus de la carte : une touche, et les lieux apparaissent dans la zone visible. L'essence affiche le prix à la pompe pour le carburant que vous avez choisi.",
    },
    hors_ligne: Section {
        titre: "Quand le réseau tombe",
        intro: "212 régions dans 49 pays d'Europe, téléchargeables une par une. Plusieurs peuvent cohabiter sur le téléphone, et le transfert d'un paquet ne demande pas de rester devant l'écran.",
    },
    atouts_titre: "Et aussi",
    atouts: &[
        Atout {
            emoji: "🚦",
            titre: "Le trafic et les signalements",
            texte: "Les bouchons en temps réel, et les signalements de ceux qui roulent devant vous — police, accident, ralentissement. Chacun se confirme ou s'infirme d'un geste, avec le décompte des deux.",
        },
        Atout {
            emoji: "🚗",
            titre: "À plusieurs, en direct",
            texte: "La position de ceux qui font la route avec vous, en temps réel sur la même carte. Et l'heure d'arrivée qui se partage en un bouton : « J'arrive vers 18:42 ».",
        },
        Atout {
            emoji: "🔥",
            titre: "Carte des feux",
            texte: "Les foyers détectés par satellite, dans le monde entier, sur la zone que vous regardez. Utile l'été, sur les routes du Sud.",
        },
        Atout {
            emoji: "🚊",
            titre: "Trams, bus et trains",
            texte: "Un quatrième mode, à côté de la voiture : les itinéraires en transports en commun, avec les lignes aux couleurs du réseau, les correspondances et les heures réelles — retard du moment compris. On part maintenant, à une heure choisie, ou pour arriver avant l'heure dite.",
        },
        Atout {
            emoji: "🫁",
            titre: "Qualité de l'air",
            texte: "Une carte dédiée : l'indice européen peint en continu sur la région, du turquoise au pourpre, et la mesure de là où vous êtes lisible sans un geste.",
        },
        Atout {
            emoji: "🔊",
            titre: "Guidage vocal",
            texte: "Les annonces au bon moment, la carte qui pivote dans le sens de la marche, le recalcul dès qu'on s'écarte. Une navigation interrompue se reprend là où elle s'était arrêtée.",
        },
        Atout {
            emoji: "📍",
            titre: "Des étapes en chemin",
            texte: "On pose une étape d'un doigt sur la carte, on passe chez Paul avant la mer. Un bouton remet les étapes dans l'ordre le plus court, et chacune peut porter son temps d'arrêt — l'heure d'arrivée le compte, au lieu de mentir.",
        },
        Atout {
            emoji: "🌦️",
            titre: "Ce que la route rencontrera",
            texte: "Avant de partir, chaque itinéraire dit ce qu'il traverse : un feu à moins de deux kilomètres du tracé, la pluie à l'heure où vous y passerez — nommée par la ville —, l'air qu'on respire à l'arrivée.",
        },
        Atout {
            emoji: "🥾",
            titre: "Une trace GPX à suivre",
            texte: "Une randonnée ou un parcours vélo exporté d'une autre application s'importe d'un geste : la trace devient l'itinéraire, consignes comprises, et se suit jusqu'au bout en mode avion.",
        },
        Atout {
            emoji: "🔗",
            titre: "Un trajet qui se partage",
            texte: "Le trajet part en lien — destination, étapes, mode. Qui a l'application l'ouvre sur le même trajet, prêt à partir ; qui ne l'a pas voit la carte et rejoint par Plans ou Google Maps.",
        },
        Atout {
            emoji: "🗣️",
            titre: "« Dis Siri, ramène-moi à la maison »",
            texte: "Le guidage démarre à la voix, vers le domicile ou le travail enregistrés — même sans réseau, leurs positions sont déjà sur l'appareil. Et dans Raccourcis, une adresse dictée devient un itinéraire.",
        },
        Atout {
            emoji: "🔒",
            titre: "Le guidage sans déverrouiller",
            texte: "La flèche, la consigne, la distance et l'heure d'arrivée vivent sur l'écran verrouillé et dans la Dynamic Island. Un coup d'œil suffit, le téléphone reste dans sa poche.",
        },
        Atout {
            emoji: "🚘",
            titre: "Prête pour CarPlay",
            texte: "L'écran voiture est écrit : la même carte, le tracé du guidage, la consigne du moment et sa distance. Il s'allumera dès l'agrément CarPlay d'Apple — brancher le téléphone suffira.",
        },
        Atout {
            emoji: "⛽",
            titre: "Le plein au bon endroit",
            texte: "Les stations le long de l'itinéraire, classées par prix réel (open data officiel), avec l'économie dite sur un plein. La moins chère s'ajoute en étape d'un geste — cinq minutes d'arrêt comprises, le temps du plein.",
        },
        Atout {
            emoji: "🍃",
            titre: "Les zones à faibles émissions, annoncées",
            texte: "Au choix de la route, chaque variante dit les ZFE qu'elle traverse — le Grand Paris comme leurs cousines européennes. La zone est nommée, la vignette reste ton affaire : ses règles changent trop pour être devinées.",
        },
        Atout {
            emoji: "🏍️",
            titre: "Le mode moto",
            texte: "La même carte, une autre fonction de coût : des itinéraires pensés deux-roues. Et l'option « routes plaisir » fuit l'autoroute pour chercher les virages — pour la balade, pas pour arriver tôt.",
        },
        Atout {
            emoji: "🚚",
            titre: "Le fourgon qui évite les ponts bas",
            texte: "Hauteur, largeur, poids déclarés carte grise en main : les ponts trop bas et les rues interdites au gabarit sortent des itinéraires, même hors ligne. L'outil Pro des utilitaires.",
        },
        Atout {
            emoji: "📋",
            titre: "La tournée collée d'un bloc",
            texte: "Vingt adresses copiées depuis les notes, une par ligne : chacune devient un arrêt, les introuvables sont montrées au lieu d'être avalées, et « Au plus court » remet l'ordre. L'outil Pro du livreur.",
        },
        Atout {
            emoji: "🔧",
            titre: "L'entretien qui se rappelle",
            texte: "Vidange, contrôle technique, pneus : des rappels posés « dans tant de kilomètres », déclenchés par les kilomètres réellement parcourus du journal — une voiture s'use en roulant, pas en attendant.",
        },
        Atout {
            emoji: "👋",
            titre: "Prévenir qu'on arrive",
            texte: "À dix minutes de l'arrivée, l'app propose le message tout prêt — minutes restantes, lien de suivi en direct compris. Celui qui attend n'a plus à demander « t'es où ? ».",
        },
        Atout {
            emoji: "🏠",
            titre: "Depuis l'écran d'accueil",
            texte: "Un widget Domicile / Travail : un toucher, et les itinéraires se comparent déjà. Il ne stocke rien — l'app résout l'adresse au réveil.",
        },
        Atout {
            emoji: "✨",
            titre: "Mon année, en une image",
            texte: "Kilomètres, heures de route, jours au volant, plus long trajet : le journal se raconte en une carte aux couleurs de la maison, prête à partager.",
        },
        Atout {
            emoji: "⭐",
            titre: "MonCap Plus",
            texte: "Quatorze jours d'essai, tout ouvert — puis une formule : Plus, 3,99 € par mois, pour l'app entière (hors ligne, GPX) ; Pro, 9,99 € par mois, pour les tournées optimisées, le journal kilométrique et la facturation. Au-delà, l'app se verrouille : tout est payant, c'est la règle.",
        },
        Atout {
            emoji: "🧾",
            titre: "Les kilomètres qui se facturent",
            texte: "Pour ceux qui roulent pour travailler : le journal des trajets s'exporte en CSV, une facture PDF se génère par mois au taux kilométrique choisi, et l'attestation fiscale annuelle s'établit au barème officiel, majoration électrique comprise — même sans réseau.",
        },
        Atout {
            emoji: "🔁",
            titre: "Domicile et travail suivent le compte",
            texte: "Les favoris se rangent dans le compte : un nouvel iPhone les retrouve à la première connexion. En cas de désaccord, l'appareil qu'on a sous les yeux reste maître — c'est lui qu'on vient de modifier.",
        },
        Atout {
            emoji: "📦",
            titre: "Ses données, en un fichier",
            texte: "Un bouton exporte tout ce que le compte connaît — trajets, recherches, signalements, préférences — en un fichier remis par la feuille de partage. Sans le mot de passe, même haché : un secret dérivé n'appartient à personne.",
        },
        Atout {
            emoji: "⏰",
            titre: "Partir à l'heure",
            texte: "On donne l'heure d'arrivée voulue : l'app soustrait le trajet estimé et cinq minutes de marge, puis pose la notification « Pars maintenant ». L'estimation est celle du moment, et la feuille le dit — promettre un recalcul en arrière-plan serait mentir.",
        },
        Atout {
            emoji: "🕙",
            titre: "Les créneaux de livraison",
            texte: "« Avant 10 h » se pose sur une étape, dans le même menu que le temps d'arrêt. L'heure de passage est estimée, et le chip s'alarme en orange dès qu'elle dépasse. Ancré sur l'étape elle-même : il survit au réordonnancement. L'outil Pro du livreur.",
        },
    ],
    hors_ligne_cartes: &[
        Atout {
            emoji: "🧭",
            titre: "Calculé sur l'appareil",
            texte: "Les itinéraires sont calculés par votre iPhone, pas par un serveur. Rater une sortie au fond d'une vallée ne laisse plus l'écran figé sur « recalcul » : la route se refait immédiatement, sans une barre de réseau.",
        },
        Atout {
            emoji: "🗺️",
            titre: "L'Europe, région par région",
            texte: "De Monaco (1,1 Mo) à la Catalogne (320 Mo), on ne télécharge que ce qu'on traverse. Des zones plus larges couvrent les longs trajets — Paris → Marseille tient dans un seul paquet.",
        },
        Atout {
            emoji: "🔀",
            titre: "Comparer sans réseau",
            texte: "Les itinéraires sont calculés sur l'appareil, jusqu'à quinze comme en ligne, avec leurs durées, leurs virages et leurs péages. Et si vous ratez une sortie, le recalcul repart de votre position — sans une barre de réseau.",
        },
        Atout {
            emoji: "📥",
            titre: "Téléchargement en arrière-plan",
            texte: "Un paquet de deux gigaoctets ne demande pas de garder l'écran allumé. Le transfert continue pendant que vous faites autre chose, et même après avoir quitté l'application.",
        },
        Atout {
            emoji: "💊",
            titre: "Les lieux et les limites, embarqués",
            texte: "L'annuaire des 19 catégories d'origine et les limites de vitesse descendent avec la région, tirés du même extrait OpenStreetMap que les routes. Chercher une pharmacie ou lire la limite du moment marche en mode avion ; les catégories ajoutées depuis demandent encore le réseau.",
        },
        Atout {
            emoji: "🔎",
            titre: "La recherche qui trouve en mode avion",
            texte: "Quand la recherche en ligne ne rend rien, l'annuaire des régions installées prend le relais : les lieux nommés se cherchent par nom, plis et accents ignorés, les plus proches d'abord. Les adresses, elles, restent l'affaire du réseau — l'annuaire ne porte que des lieux nommés.",
        },
    ],
    tarifs: Tarifs {
        titre: "Le prix, dit simplement",
        intro: "Pas de pièges : quatorze jours pour tout essayer, puis une formule au mois — Plus ou Pro, selon l'usage.",
        plus: Formule {
            nom: "MonCap Plus",
            prix: "3,99 €",
            periode: "par mois, sans engagement",
            points: &[
                "L'app entière : guidage, comparateur, transports",
                "212 régions hors ligne — itinéraires, lieux, limites de vitesse",
                "Import de traces GPX",
                "La station-service la moins chère sur ta route",
                "La consigne au poignet, sur Apple Watch",
            ],
        },
        pro: Formule {
            nom: "MonCap Pro",
            prix: "9,99 €",
            periode: "par mois, pour ceux qui roulent pour travailler",
            points: &[
                "Tout MonCap Plus",
                "Tournées optimisées — collées d'un bloc, une adresse par ligne",
                "Créneaux de livraison : le chip s'alarme avant l'heure",
                "Journal kilométrique (CSV) et attestation fiscale au barème",
                "Facturation PDF au taux kilométrique choisi",
                "Gabarit utilitaire : les ponts bas s'écartent du trajet",
            ],
        },
        note: "Passé les quatorze jours d'essai, l'app se verrouille jusqu'à l'abonnement — tout est payant, c'est la règle de la maison. L'abonnement se prend dans l'application, se gère et se résilie dans les réglages iOS.",
    },
    preuve: Preuve {
        titre: "Une mesure vaut mieux qu'une promesse",
        texte: "Bordeaux → Limoges, {mesure}, calculé sans aucun réseau sur le paquet Nouvelle-Aquitaine. Ce trajet traverse une ancienne frontière de découpage : les routes y sont recousues à la construction, pas rafistolées sur le téléphone.",
        mesure: "222,5 km en 2 h 16",
        note: "Cette vérification tourne chaque semaine, sur le paquet réellement publié.",
    },
    pile_titre: "Ce qu'il y a dessous",
    pile: &[
        Pile { domaine: "Application iOS", outils: &["Swift", "SwiftUI", "MapKit", "MapLibre", "Core Location", "AVFoundation", "WatchConnectivity"] },
        Pile { domaine: "Serveur", outils: &["Rust", "Axum", "Tokio", "SeaORM", "PostgreSQL", "Redis"] },
        Pile { domaine: "Cartes et itinéraires", outils: &["Valhalla", "OpenStreetMap", "Overpass", "Osmium", "PMTiles", "IGN"] },
        Pile { domaine: "Données en direct", outils: &["TomTom", "open-meteo", "NASA FIRMS", "data.economie.gouv.fr"] },
        Pile { domaine: "Web", outils: &["React", "TypeScript", "Bun"] },
        Pile { domaine: "Déploiement", outils: &["Heroku", "GitHub Actions", "Docker"] },
    ],
    franchise_titre: "Ce qu'elle ne fait pas",
    franchise: &[
        "Les péages sont estimés d'après la part d'autoroute, au tarif moyen français classe 1 — pas le tarif exact du concessionnaire.",
        "Hors ligne, le calcul propose les variantes qu'il trouve : sur un trajet court elles sont moins nombreuses qu'en ligne. Ni le trafic en direct ni les signalements ne sont disponibles.",
        "Un trajet hors réseau doit tenir entier dans un même paquet : c'est à cela que servent les zones, qui se chevauchent.",
        "L'application suit la langue du téléphone parmi le français, l'espagnol et l'anglais ; ailleurs, elle reste en français.",
        "L'app montre affiche ce que l'iPhone lui envoie : sans guidage en cours sur le téléphone, elle attend.",
        "L'application est en test sur TestFlight, pas encore sur l'App Store.",
    ],
    legendes: Legendes {
        itineraires: "Dessin de l'écran de comparaison — mise en page et étiquettes réelles, valeurs d'exemple.",
        guidage: "Le bandeau de manœuvre, la vitesse et la limite, la météo, le panneau de commandes et la barre d'arrivée — dessin d'écran, valeurs d'exemple.",
        montre: "Dessin de l'app Apple Watch pendant un guidage — mise en page réelle, valeurs d'exemple.",
        lieux: "Les 42 catégories, avec les libellés de l'application — montrées ici d'un seul tenant, alors que le bandeau défile sur le téléphone.",
        regions: "Noms et poids réels, lus sur les Releases publiées ; seul l'avancement est mis en scène.",
    },
    reste: "et 12 autres itinéraires",
    maquette: Maquette { installee: "✓ Installée" },
    signature: "Conçu et développé par",
    credits: "Cartes et données routières OpenStreetMap (ODbL), calcul d'itinéraire Valhalla, trafic TomTom, météo open-meteo, prix des carburants data.economie.gouv.fr, foyers NASA FIRMS, photos aériennes IGN.",
    pied: Pied {
        slogan: "Le GPS qui compare, puis vous guide — avec ou sans réseau.",
        decouvrir: "Découvrir",
        ancres: &[
            Ancre { id: "comparateur-titre", nom: "Le comparateur" },
            Ancre { id: "guidage-titre", nom: "Le guidage" },
            Ancre { id: "montre-titre", nom: "Au poignet" },
            Ancre { id: "lieux-titre", nom: "Autour de vous" },
            Ancre { id: "atouts-titre", nom: "Et aussi" },
            Ancre { id: "hors-ligne-titre", nom: "Hors ligne" },
            Ancre { id: "tarifs-titre", nom: "Le prix" },
            Ancre { id: "preuve-titre", nom: "La preuve" },
            Ancre { id: "pile-titre", nom: "Sous le capot" },
            Ancre { id: "franchise-titre", nom: "Ce qu'elle ne fait pas" },
        ],
        donnees: "Données et moteurs",
        legal_titre: "Légal",
        copyright: "Tous droits réservés.",
    },
    mentions: PageLegale {
        chemin: "/mentions-legales",
        titre: "Mentions légales",
        description: "Éditeur, hébergement et licences des données du site et de l'application MonCap GPS.",
        retour: "← Retour au site",
        sections: &[
            SectionLegale {
                titre: "Éditeur",
                corps: &[
                    "Ce site et l'application MonCap GPS sont édités par Maxime Nathan Lestage.",
                    "Contact : maxlestage@icloud.com.",
                ],
            },
            SectionLegale {
                titre: "Hébergement",
                corps: &[
                    "Le site et son serveur sont hébergés par Heroku (Salesforce, Inc.), 415 Mission Street, San Francisco, CA 94105, États-Unis.",
                ],
            },
            SectionLegale {
                titre: "Propriété intellectuelle",
                corps: &[
                    "Le code, les textes et la marque MonCap GPS appartiennent à leur auteur. Les cartes et données routières proviennent d'OpenStreetMap, © les contributeurs OpenStreetMap, sous licence ODbL.",
                    "Les autres données affichées — trafic, météo, qualité de l'air, prix des carburants, foyers d'incendie, photos aériennes — restent la propriété de leurs sources respectives, chacune sous sa propre licence.",
                ],
            },
        ],
    },
    confidentialite: PageLegale {
        chemin: "/confidentialite",
        titre: "Confidentialité",
        description: "Ce que MonCap GPS collecte, ce qui reste sur votre téléphone, et vos droits.",
        retour: "← Retour au site",
        sections: &[
            SectionLegale {
                titre: "Ce site",
                corps: &[
                    "Ce site ne dépose aucun cookie, ne charge aucun script, n'embarque ni traceur ni mesure d'audience. Les seules requêtes que votre navigateur envoie servent à afficher la page.",
                ],
            },
            SectionLegale {
                titre: "Le compte",
                corps: &[
                    "L'application fonctionne avec un compte : votre adresse e-mail sert d'identifiant, accompagnée d'un mot de passe (conservé haché) et de vos recherches récentes de destination. L'adresse ne sert à rien d'autre : aucun courriel en dehors de la réinitialisation de mot de passe que vous demandez, aucune lettre d'information, aucune revente. Supprimer le compte supprime tout, l'adresse comprise.",
                ],
            },
            SectionLegale {
                titre: "La position",
                corps: &[
                    "Votre position est traitée sur l'appareil pour le guidage. Elle n'est envoyée au serveur que lorsque vous partagez votre trajet en direct, le temps de ce partage. Les favoris Domicile et Travail restent sur le téléphone.",
                ],
            },
            SectionLegale {
                titre: "Les tiers",
                corps: &[
                    "Certaines données affichées sont demandées directement à leurs sources — météo, qualité de l'air, altitude. Ces services reçoivent alors les coordonnées de la zone demandée, jamais votre identité ni votre compte.",
                ],
            },
            SectionLegale {
                titre: "Publicité",
                corps: &[
                    "Aucune publicité, aucune revente de données, aucune mesure d'audience. L'application ne vit pas de vos données.",
                ],
            },
            SectionLegale {
                titre: "Vos droits",
                corps: &[
                    "Vous pouvez demander l'accès, la rectification ou la suppression de ce qui est rattaché à votre compte en écrivant à maxlestage@icloud.com. La suppression du compte emporte celle de ses données.",
                ],
            },
        ],
    },
};

// --- Español ---------------------------------------------------------

pub const ES: Contenu = Contenu {
    code: "es",
    chemin: "/es",
    nom: "Español",
    titre: "MonCap GPS — compara y conduce, con o sin cobertura",
    description: "Un GPS para iOS que compara hasta 15 rutas — duración, giros, desnivel, combustible, peajes — y luego te guía. 212 regiones de Europa descargables para conducir — y pedalear, y caminar — sin cobertura. Tranvías y buses incluidos.",
    h1: &["Compara y conduce", "con o sin cobertura"],
    chapeau: "Un GPS para iPhone que pone las rutas una al lado de otra — duración, giros, desnivel, combustible, peajes — y luego te guía. Descarga tu región una vez y sigue calculando solo: valles, túneles, extranjero.",
    se_connecter: "Iniciar sesión",
    chevrons: Chevrons { descendre: "Sección siguiente", remonter: "Volver arriba" },
    theme: Theme { libelle: "Tema", noms: &["Auto", "Claro", "Oscuro", "Naranja"] },
    langue_libelle: "Idioma",
    chiffres: &[
        Chiffre { valeur: "15", legende: "rutas como máximo" },
        Chiffre { valeur: "42", legende: "categorías de lugares" },
        Chiffre { valeur: "212", legende: "regiones descargables" },
        Chiffre { valeur: "49", legende: "países de Europa" },
        Chiffre { valeur: "3,99 €", legende: "al mes tras 14 días de prueba" },
    ],
    comparateur: SectionApres {
        titre: "La ruta se te propone, no se te impone",
        intro: "Hasta 15 rutas una al lado de otra, cada una etiquetada como la más sencilla, la más rápida o la más corta. Para cada una: la duración, la distancia, el número de giros, la hora de llegada, el desnivel — y lo que cuesta.",
        apres: "El combustible se calcula con el consumo de tu vehículo y el precio real de las gasolineras a tu alrededor (datos públicos oficiales), no con una media nacional. Los peajes se suman cuando la ruta los usa. Una cuesta pronunciada se avisa antes de salir, no se descubre a mitad de camino. A pie, en bici o en coche, y se puede pedir evitar peajes o autopistas. En bici, hasta se dice cuál — carretera, ciudad o montaña — y si hay que huir de las cuestas.",
    },
    guidage: Section {
        titre: "Y luego te lleva",
        intro: "La flecha sigue la maniobra real, la distancia al siguiente giro se actualiza, y el número de salida aparece bajo la indicación. Al acercarse a un cruce, los carriles a tomar se iluminan. La velocidad del momento se compara con el límite de la vía, leído en OpenStreetMap: se pone en rojo cuando lo superas. Abajo, la hora de llegada vive junto a los minutos restantes, y los mandos del mapa caben en un solo panel — despejado de todo lo que no sirve al volante.",
    },
    montre: Section {
        titre: "Y en la muñeca",
        intro: "El reloj tiene su propia aplicación MonCap — no un simple reflejo del iPhone. Se instala con la app, se coloca como complicación en la esfera y se abre con un toque: la maniobra en grande, la distancia, lo que queda y la hora de llegada, a pantalla completa. Un toque en la muñeca con cada nueva indicación — la curva se siente venir sin mirar la pantalla. El último estado enviado sobrevive al cierre: levantas la muñeca y la indicación ya está ahí.",
    },
    lieux: Section {
        titre: "Lo que hay alrededor",
        intro: "Una barra de categorías sobre el mapa: un toque, y los lugares aparecen en la zona visible. La gasolina muestra el precio en el surtidor para el carburante que hayas elegido.",
    },
    hors_ligne: Section {
        titre: "Cuando se cae la cobertura",
        intro: "212 regiones en 49 países de Europa, descargables una a una. Varias pueden convivir en el teléfono, y la transferencia de un paquete no exige quedarse delante de la pantalla.",
    },
    atouts_titre: "Y además",
    atouts: &[
        Atout {
            emoji: "🚦",
            titre: "El tráfico y los avisos",
            texte: "Los atascos en tiempo real, y los avisos de quienes circulan por delante — policía, accidente, retención. Cada uno se confirma o se desmiente con un gesto, con el recuento de ambos.",
        },
        Atout {
            emoji: "🚗",
            titre: "En grupo, en directo",
            texte: "La posición de quienes hacen el camino contigo, en tiempo real sobre el mismo mapa. Y la hora de llegada que se comparte con un botón: «Llego sobre las 18:42».",
        },
        Atout {
            emoji: "🔥",
            titre: "Mapa de incendios",
            texte: "Los focos detectados por satélite, en el mundo entero, sobre la zona que estás mirando. Útil en verano, en las carreteras del sur.",
        },
        Atout {
            emoji: "🚊",
            titre: "Tranvías, buses y trenes",
            texte: "Un cuarto modo, junto al coche: las rutas en transporte público, con las líneas en los colores de la red, los transbordos y las horas reales — retraso del momento incluido. Se sale ahora, a una hora elegida, o para llegar antes de la hora dicha.",
        },
        Atout {
            emoji: "🫁",
            titre: "Calidad del aire",
            texte: "Un mapa dedicado: el índice europeo pintado en continuo sobre la región, del turquesa al púrpura, y la medición de donde estás legible sin un gesto.",
        },
        Atout {
            emoji: "🔊",
            titre: "Guía por voz",
            texte: "Los avisos en el momento justo, el mapa que gira en el sentido de la marcha, el recálculo en cuanto te desvías. Una navegación interrumpida se retoma donde se quedó.",
        },
        Atout {
            emoji: "📍",
            titre: "Paradas por el camino",
            texte: "Una parada se pone con un dedo sobre el mapa: se pasa por casa de Pablo antes del mar. Un botón reordena las paradas al camino más corto, y cada una puede llevar su tiempo de parada — la hora de llegada lo cuenta, en vez de mentir.",
        },
        Atout {
            emoji: "🌦️",
            titre: "Lo que la ruta va a encontrar",
            texte: "Antes de salir, cada ruta dice lo que atraviesa: un incendio a menos de dos kilómetros del trazado, la lluvia a la hora en que pasarás — nombrada por la ciudad —, el aire que se respira al llegar.",
        },
        Atout {
            emoji: "🥾",
            titre: "Una traza GPX que seguir",
            texte: "Una caminata o una ruta en bici exportada de otra aplicación se importa con un gesto: la traza se vuelve la ruta, indicaciones incluidas, y se sigue hasta el final en modo avión.",
        },
        Atout {
            emoji: "🔗",
            titre: "Una ruta que se comparte",
            texte: "La ruta se envía como enlace — destino, paradas, modo. Quien tiene la aplicación la abre sobre la misma ruta, lista para salir; quien no la tiene ve el mapa y llega con Plans o Google Maps.",
        },
        Atout {
            emoji: "🗣️",
            titre: "«Oye Siri, llévame a casa»",
            texte: "La guía arranca con la voz, hacia la casa o el trabajo guardados — incluso sin cobertura, sus posiciones ya están en el dispositivo. Y en Atajos, una dirección dictada se convierte en ruta.",
        },
        Atout {
            emoji: "🔒",
            titre: "La guía sin desbloquear",
            texte: "La flecha, la indicación, la distancia y la hora de llegada viven en la pantalla bloqueada y en la Dynamic Island. Basta un vistazo, el teléfono se queda en el bolsillo.",
        },
        Atout {
            emoji: "🚘",
            titre: "Lista para CarPlay",
            texte: "La pantalla del coche está escrita: el mismo mapa, el trazado de la guía, la indicación del momento y su distancia. Se encenderá con la aprobación CarPlay de Apple — bastará con conectar el teléfono.",
        },
        Atout {
            emoji: "⛽",
            titre: "El depósito en el buen sitio",
            texte: "Las estaciones a lo largo de la ruta, ordenadas por precio real (open data oficial), con el ahorro dicho sobre un depósito. La más barata se añade como parada de un gesto — con cinco minutos de alto, lo que dura el repostaje.",
        },
        Atout {
            emoji: "🍃",
            titre: "Las zonas de bajas emisiones, anunciadas",
            texte: "Al elegir la ruta, cada variante dice las ZBE que atraviesa — las españolas como sus primas europeas. La zona se nombra, el distintivo es cosa tuya: sus reglas cambian demasiado para adivinarlas.",
        },
        Atout {
            emoji: "🏍️",
            titre: "El modo moto",
            texte: "El mismo mapa, otra función de coste: rutas pensadas para dos ruedas. Y la opción « rutas de placer » huye de la autopista y busca las curvas — para el paseo, no para llegar pronto.",
        },
        Atout {
            emoji: "🚚",
            titre: "La furgoneta que evita los puentes bajos",
            texte: "Altura, anchura y peso declarados con el permiso de circulación en la mano: los puentes demasiado bajos y las calles prohibidas al gálibo salen de las rutas, incluso sin conexión. La herramienta Pro de las furgonetas.",
        },
        Atout {
            emoji: "📋",
            titre: "El reparto pegado de un bloque",
            texte: "Veinte direcciones copiadas de las notas, una por línea: cada una se vuelve una parada, las no encontradas se muestran en vez de tragarse, y « La más corta » pone el orden. La herramienta Pro del repartidor.",
        },
        Atout {
            emoji: "🔧",
            titre: "El mantenimiento que se recuerda",
            texte: "Cambio de aceite, inspección técnica, neumáticos: recordatorios puestos « dentro de tantos kilómetros », disparados por los kilómetros realmente recorridos del registro — un coche se desgasta rodando, no esperando.",
        },
        Atout {
            emoji: "👋",
            titre: "Avisar de que llegas",
            texte: "A diez minutos del destino, la app propone el mensaje ya listo — minutos restantes, enlace de seguimiento en directo incluido. Quien espera ya no pregunta « ¿dónde estás? ».",
        },
        Atout {
            emoji: "🏠",
            titre: "Desde la pantalla de inicio",
            texte: "Un widget Casa / Trabajo: un toque, y las rutas ya se comparan. No guarda nada — la app resuelve la dirección al despertar.",
        },
        Atout {
            emoji: "✨",
            titre: "Mi año, en una imagen",
            texte: "Kilómetros, horas de carretera, días al volante, el viaje más largo: el registro se cuenta en una tarjeta con los colores de la casa, lista para compartir.",
        },
        Atout {
            emoji: "⭐",
            titre: "MonCap Plus",
            texte: "Catorce días de prueba, todo abierto — después una fórmula: Plus, 3,99 € al mes, para la app entera (sin conexión, GPX); Pro, 9,99 € al mes, para los repartos optimizados, el registro de kilómetros y la facturación. Más allá, la app se bloquea: todo es de pago, es la regla.",
        },
        Atout {
            emoji: "🧾",
            titre: "Los kilómetros que se facturan",
            texte: "Para quienes conducen por trabajo: el registro de trayectos se exporta en CSV, una factura PDF se genera por mes con la tarifa por kilómetro elegida, y el certificado fiscal anual se establece al baremo oficial, con la mayoración eléctrica — incluso sin cobertura.",
        },
        Atout {
            emoji: "🔁",
            titre: "Casa y trabajo siguen a la cuenta",
            texte: "Los favoritos se guardan en la cuenta: un iPhone nuevo los recupera en la primera conexión. Si hay desacuerdo, manda el dispositivo que tienes delante — es el que acabas de tocar.",
        },
        Atout {
            emoji: "📦",
            titre: "Tus datos, en un archivo",
            texte: "Un botón exporta todo lo que la cuenta conoce — trayectos, búsquedas, avisos, preferencias — en un archivo entregado por la hoja de compartir. Sin la contraseña, ni siquiera con hash: un secreto derivado no pertenece a nadie.",
        },
        Atout {
            emoji: "⏰",
            titre: "Salir a tiempo",
            texte: "Das la hora de llegada deseada: la app resta el trayecto estimado y cinco minutos de margen, y pone la notificación « Sal ahora ». La estimación es la del momento, y la hoja lo dice — prometer un recálculo en segundo plano sería mentir.",
        },
        Atout {
            emoji: "🕙",
            titre: "Las franjas de entrega",
            texte: "« Antes de las 10 » se pone en una parada, en el mismo menú que el tiempo de alto. La hora de paso se estima, y el aviso se enciende en naranja en cuanto se pasa. Anclado a la parada misma: sobrevive al reordenamiento. La herramienta Pro del repartidor.",
        },
    ],
    hors_ligne_cartes: &[
        Atout {
            emoji: "🧭",
            titre: "Calculado en el dispositivo",
            texte: "Las rutas las calcula tu iPhone, no un servidor. Perder una salida en el fondo de un valle ya no deja la pantalla congelada en «recalculando»: la ruta se rehace al instante, sin una sola barra de cobertura.",
        },
        Atout {
            emoji: "🗺️",
            titre: "Europa, región por región",
            texte: "De Mónaco (1,1 MB) a Cataluña (320 MB), solo se descarga lo que se atraviesa. Zonas más amplias cubren los trayectos largos — París → Marsella cabe en un solo paquete.",
        },
        Atout {
            emoji: "🔀",
            titre: "Comparar sin cobertura",
            texte: "Las rutas se calculan en el dispositivo, hasta quince como en línea, con sus duraciones, sus giros y sus peajes. Y si te pasas una salida, el recálculo parte de tu posición, sin una sola barra de cobertura.",
        },
        Atout {
            emoji: "📥",
            titre: "Descarga en segundo plano",
            texte: "Un paquete de dos gigabytes no obliga a mantener la pantalla encendida. La transferencia continúa mientras haces otra cosa, e incluso después de cerrar la aplicación.",
        },
        Atout {
            emoji: "💊",
            titre: "Los lugares y los límites, a bordo",
            texte: "El directorio de las 19 categorías originales y los límites de velocidad bajan con la región, sacados del mismo extracto de OpenStreetMap que las carreteras. Buscar una farmacia o leer el límite del momento funciona en modo avión; las categorías añadidas después aún piden red.",
        },
        Atout {
            emoji: "🔎",
            titre: "La búsqueda que encuentra en modo avión",
            texte: "Cuando la búsqueda en línea no devuelve nada, el directorio de las regiones instaladas toma el relevo: los lugares con nombre se buscan por nombre, sin acentos ni mayúsculas, los más cercanos primero. Las direcciones siguen siendo cosa de la red — el directorio solo lleva lugares con nombre.",
        },
    ],
    tarifs: Tarifs {
        titre: "El precio, dicho claro",
        intro: "Sin trampas: catorce días para probarlo todo, luego una fórmula al mes — Plus o Pro, según el uso.",
        plus: Formule {
            nom: "MonCap Plus",
            prix: "3,99 €",
            periode: "al mes, sin permanencia",
            points: &[
                "La app entera: guía, comparador, transportes",
                "212 regiones sin conexión — rutas, lugares, límites de velocidad",
                "Importación de trazas GPX",
                "La gasolinera más barata en tu ruta",
                "La indicación en la muñeca, en el Apple Watch",
            ],
        },
        pro: Formule {
            nom: "MonCap Pro",
            prix: "9,99 €",
            periode: "al mes, para quienes conducen por trabajo",
            points: &[
                "Todo MonCap Plus",
                "Repartos optimizados — pegados de un bloque, una dirección por línea",
                "Franjas de entrega: el aviso salta antes de la hora",
                "Registro de kilómetros (CSV) y certificado fiscal al baremo",
                "Facturación PDF con la tarifa por kilómetro elegida",
                "Gálibo de furgoneta: los puentes bajos se apartan del trayecto",
            ],
        },
        note: "Pasados los catorce días de prueba, la app se bloquea hasta la suscripción — todo es de pago, es la regla de la casa. La suscripción se contrata en la aplicación y se cancela en los ajustes de iOS.",
    },
    preuve: Preuve {
        titre: "Una medición vale más que una promesa",
        texte: "Burdeos → Limoges, {mesure}, calculado sin cobertura alguna sobre el paquete Nouvelle-Aquitaine. Ese trayecto cruza una antigua frontera de recorte: las carreteras se recosen al construir el paquete, no se remiendan en el teléfono.",
        mesure: "222,5 km en 2 h 16",
        note: "Esta comprobación se ejecuta cada semana, sobre el paquete realmente publicado.",
    },
    pile_titre: "Lo que hay debajo",
    pile: &[
        Pile { domaine: "Aplicación iOS", outils: &["Swift", "SwiftUI", "MapKit", "MapLibre", "Core Location", "AVFoundation", "WatchConnectivity"] },
        Pile { domaine: "Servidor", outils: &["Rust", "Axum", "Tokio", "SeaORM", "PostgreSQL", "Redis"] },
        Pile { domaine: "Mapas y rutas", outils: &["Valhalla", "OpenStreetMap", "Overpass", "Osmium", "PMTiles", "IGN"] },
        Pile { domaine: "Datos en directo", outils: &["TomTom", "open-meteo", "NASA FIRMS", "data.economie.gouv.fr"] },
        Pile { domaine: "Web", outils: &["React", "TypeScript", "Bun"] },
        Pile { domaine: "Despliegue", outils: &["Heroku", "GitHub Actions", "Docker"] },
    ],
    franchise_titre: "Lo que no hace",
    franchise: &[
        "Los peajes se estiman según el tramo de autopista, a la tarifa media francesa de clase 1 — no es la tarifa exacta de la concesionaria.",
        "Sin cobertura el cálculo propone las variantes que encuentra: en un trayecto corto son menos que en línea. No hay ni tráfico en directo ni avisos.",
        "Un trayecto sin cobertura debe caber entero en un mismo paquete: para eso sirven las zonas, que se solapan.",
        "La aplicación sigue el idioma del teléfono entre francés, español e inglés; en cualquier otro, se queda en francés.",
        "La app del reloj muestra lo que el iPhone le envía: sin una guía en curso en el teléfono, espera.",
        "La aplicación está en pruebas en TestFlight, todavía no en la App Store.",
    ],
    legendes: Legendes {
        itineraires: "Dibujo de la pantalla de comparación — maquetación y etiquetas reales, valores de ejemplo.",
        guidage: "La banda de maniobra, la velocidad y el límite, el tiempo, el panel de mandos y la barra de llegada — dibujo de pantalla, valores de ejemplo.",
        montre: "Dibujo de la app para Apple Watch durante una guía — maquetación real, valores de ejemplo.",
        lieux: "Las 42 categorías, con los rótulos de la aplicación — mostradas aquí de una sola vez, mientras que en el teléfono la barra se desplaza.",
        regions: "Nombres y pesos reales, leídos en las Releases publicadas; solo el progreso está recreado.",
    },
    reste: "y 12 rutas más",
    maquette: Maquette { installee: "✓ Instalada" },
    signature: "Diseñado y desarrollado por",
    credits: "Mapas y datos viarios OpenStreetMap (ODbL), cálculo de rutas Valhalla, tráfico TomTom, meteorología open-meteo, precios de carburantes data.economie.gouv.fr, focos NASA FIRMS, fotos aéreas IGN.",
    pied: Pied {
        slogan: "El GPS que compara y luego te guía — con o sin cobertura.",
        decouvrir: "Descubrir",
        ancres: &[
            Ancre { id: "comparateur-titre", nom: "El comparador" },
            Ancre { id: "guidage-titre", nom: "La guía" },
            Ancre { id: "montre-titre", nom: "En la muñeca" },
            Ancre { id: "lieux-titre", nom: "A tu alrededor" },
            Ancre { id: "atouts-titre", nom: "Y además" },
            Ancre { id: "hors-ligne-titre", nom: "Sin conexión" },
            Ancre { id: "tarifs-titre", nom: "El precio" },
            Ancre { id: "preuve-titre", nom: "La prueba" },
            Ancre { id: "pile-titre", nom: "Bajo el capó" },
            Ancre { id: "franchise-titre", nom: "Lo que no hace" },
        ],
        donnees: "Datos y motores",
        legal_titre: "Legal",
        copyright: "Todos los derechos reservados.",
    },
    mentions: PageLegale {
        chemin: "/es/aviso-legal",
        titre: "Aviso legal",
        description: "Editor, alojamiento y licencias de los datos del sitio y de la aplicación MonCap GPS.",
        retour: "← Volver al sitio",
        sections: &[
            SectionLegale {
                titre: "Editor",
                corps: &[
                    "Este sitio y la aplicación MonCap GPS son editados por Maxime Nathan Lestage.",
                    "Contacto: maxlestage@icloud.com.",
                ],
            },
            SectionLegale {
                titre: "Alojamiento",
                corps: &[
                    "El sitio y su servidor están alojados en Heroku (Salesforce, Inc.), 415 Mission Street, San Francisco, CA 94105, Estados Unidos.",
                ],
            },
            SectionLegale {
                titre: "Propiedad intelectual",
                corps: &[
                    "El código, los textos y la marca MonCap GPS pertenecen a su autor. Los mapas y datos viarios provienen de OpenStreetMap, © los contribuidores de OpenStreetMap, bajo licencia ODbL.",
                    "Los demás datos mostrados — tráfico, meteorología, calidad del aire, precios de carburantes, focos de incendio, fotos aéreas — siguen siendo propiedad de sus fuentes respectivas, cada una bajo su propia licencia.",
                ],
            },
        ],
    },
    confidentialite: PageLegale {
        chemin: "/es/privacidad",
        titre: "Privacidad",
        description: "Lo que MonCap GPS recoge, lo que se queda en tu teléfono, y tus derechos.",
        retour: "← Volver al sitio",
        sections: &[
            SectionLegale {
                titre: "Este sitio",
                corps: &[
                    "Este sitio no deposita ninguna cookie, no carga ningún script, no lleva rastreadores ni medición de audiencia. Las únicas peticiones que envía tu navegador sirven para mostrar la página.",
                ],
            },
            SectionLegale {
                titre: "La cuenta",
                corps: &[
                    "La aplicación funciona con una cuenta: tu dirección de correo sirve de identificador, junto con una contraseña (guardada con hash) y tus búsquedas recientes de destino. La dirección no sirve para nada más: ningún correo salvo la recuperación de contraseña que tú pidas, ningún boletín, ninguna reventa. Eliminar la cuenta lo elimina todo, dirección incluida.",
                ],
            },
            SectionLegale {
                titre: "La posición",
                corps: &[
                    "Tu posición se procesa en el dispositivo para la guía. Solo se envía al servidor cuando compartes tu trayecto en directo, mientras dura ese compartir. Los favoritos Casa y Trabajo se quedan en el teléfono.",
                ],
            },
            SectionLegale {
                titre: "Los terceros",
                corps: &[
                    "Algunos datos mostrados se piden directamente a sus fuentes — meteorología, calidad del aire, altitud. Esos servicios reciben entonces las coordenadas de la zona pedida, nunca tu identidad ni tu cuenta.",
                ],
            },
            SectionLegale {
                titre: "Publicidad",
                corps: &[
                    "Ninguna publicidad, ninguna reventa de datos, ninguna medición de audiencia. La aplicación no vive de tus datos.",
                ],
            },
            SectionLegale {
                titre: "Tus derechos",
                corps: &[
                    "Puedes pedir el acceso, la rectificación o la supresión de lo que está vinculado a tu cuenta escribiendo a maxlestage@icloud.com. La supresión de la cuenta conlleva la de sus datos.",
                ],
            },
        ],
    },
};

// --- English ---------------------------------------------------------

pub const EN: Contenu = Contenu {
    code: "en",
    chemin: "/en",
    nom: "English",
    titre: "MonCap GPS — compare, then drive, with or without signal",
    description: "An iOS GPS that compares up to 15 routes — time, turns, elevation, fuel, tolls — then guides you. 212 downloadable European regions to drive — and cycle, and walk — with no signal. Trams and buses included.",
    h1: &["Compare, then drive", "with or without signal"],
    chapeau: "An iPhone GPS that lays the routes side by side — time, turns, elevation, fuel, tolls — then guides you. Download your region once and it keeps computing on its own: valleys, tunnels, abroad.",
    se_connecter: "Sign in",
    chevrons: Chevrons { descendre: "Next section", remonter: "Back to top" },
    theme: Theme { libelle: "Theme", noms: &["Auto", "Light", "Dark", "Orange"] },
    langue_libelle: "Language",
    chiffres: &[
        Chiffre { valeur: "15", legende: "routes at most" },
        Chiffre { valeur: "42", legende: "place categories" },
        Chiffre { valeur: "212", legende: "downloadable regions" },
        Chiffre { valeur: "49", legende: "European countries" },
        Chiffre { valeur: "3.99 €", legende: "per month after a 14-day trial" },
    ],
    comparateur: SectionApres {
        titre: "The route is offered, not imposed",
        intro: "Up to 15 routes side by side, each labelled the simplest, the fastest or the shortest. For every one: time, distance, number of turns, arrival time, elevation — and what it costs.",
        apres: "Fuel is worked out from your vehicle's consumption and the real price at the stations around you (official open data), not a national average. Tolls are added when the route uses them. A steep climb is flagged before you leave, not discovered halfway. On foot, by bike or by car, and you can ask to avoid tolls or motorways. On a bike, you even say which one — road, city or mountain — and whether to stay clear of climbs.",
    },
    guidage: Section {
        titre: "Then it takes you there",
        intro: "The arrow follows the actual manoeuvre, the distance to the next turn updates, and the exit number appears under the instruction. Coming up to a junction, the lanes to take light up. Your current speed is compared with the road's limit, read from OpenStreetMap: it turns red when you go over. At the bottom, the arrival time lives next to the minutes left, and the map controls fit in a single panel — cleared of everything that's no use at the wheel.",
    },
    montre: Section {
        titre: "And on your wrist",
        intro: "The watch has its own MonCap app — not just a mirror of the iPhone. It installs along with the app, sits as a complication on the watch face, and opens with one tap: the manoeuvre writ large, the distance, what's left and the arrival time, full screen. A tap on the wrist with each new instruction — you feel the turn coming without looking at the screen. The last state sent survives the app closing: raise your wrist and the instruction is already there.",
    },
    lieux: Section {
        titre: "What's around you",
        intro: "A category strip above the map: one tap and the places appear in the visible area. Fuel shows the pump price for the grade you picked.",
    },
    hors_ligne: Section {
        titre: "When the signal drops",
        intro: "212 regions across 49 European countries, downloadable one by one. Several can live on the phone at once, and transferring a pack doesn't mean sitting in front of the screen.",
    },
    atouts_titre: "And also",
    atouts: &[
        Atout {
            emoji: "🚦",
            titre: "Traffic and reports",
            texte: "Live jams, and reports from those driving ahead of you — police, crash, slowdown. Each one is confirmed or denied with a tap, with both counts shown.",
        },
        Atout {
            emoji: "🚗",
            titre: "Together, live",
            texte: "The position of whoever is driving with you, live on the same map. And an arrival time you share with one button: “I'll be there around 18:42”.",
        },
        Atout {
            emoji: "🔥",
            titre: "Wildfire map",
            texte: "Satellite-detected hotspots, worldwide, for the area you are looking at. Useful in summer, on southern roads.",
        },
        Atout {
            emoji: "🚊",
            titre: "Trams, buses and trains",
            texte: "A fourth mode, next to driving: public transit routes, with lines in the network's own colours, transfers, and real times — current delay included. Leave now, at a chosen time, or so as to arrive before a set hour.",
        },
        Atout {
            emoji: "🫁",
            titre: "Air quality",
            texte: "A dedicated map: the European index painted continuously over the region, from turquoise to purple, with the reading for where you are visible at a glance.",
        },
        Atout {
            emoji: "🔊",
            titre: "Voice guidance",
            texte: "Announcements at the right moment, the map turning with your direction of travel, a recalculation the moment you stray. An interrupted trip resumes where it stopped.",
        },
        Atout {
            emoji: "📍",
            titre: "Stops along the way",
            texte: "Drop a stop with one finger on the map — call in at Paul's before the sea. One button reorders the stops into the shortest run, and each can carry its own stop time — the arrival time counts it, instead of lying.",
        },
        Atout {
            emoji: "🌦️",
            titre: "What the road will meet",
            texte: "Before you leave, each route says what it crosses: a wildfire within two kilometres of the line, rain at the hour you'll pass through — named by town —, and the air you'll breathe on arrival.",
        },
        Atout {
            emoji: "🥾",
            titre: "A GPX track to follow",
            texte: "A hike or a bike route exported from another app imports in one move: the track becomes the route, instructions included, and can be followed to the end in airplane mode.",
        },
        Atout {
            emoji: "🔗",
            titre: "A trip that travels as a link",
            texte: "The trip goes out as a link — destination, stops, mode. Whoever has the app opens it on the same trip, ready to go; whoever doesn't sees the map and gets there with Apple or Google Maps.",
        },
        Atout {
            emoji: "🗣️",
            titre: "“Hey Siri, take me home”",
            texte: "Guidance starts by voice, to your saved home or work — even offline, since their positions are already on the device. And in Shortcuts, a spoken address becomes a route.",
        },
        Atout {
            emoji: "🔒",
            titre: "Guidance without unlocking",
            texte: "The arrow, the instruction, the distance and the arrival time live on the lock screen and in the Dynamic Island. A glance is enough; the phone stays in your pocket.",
        },
        Atout {
            emoji: "🚘",
            titre: "Ready for CarPlay",
            texte: "The car screen is written: the same map, the route being driven, the current instruction and its distance. It lights up as soon as Apple grants the CarPlay approval — plugging in the phone will be enough.",
        },
        Atout {
            emoji: "⛽",
            titre: "The tank filled in the right place",
            texte: "Stations along the route, ranked by real price (official open data), with the saving stated on a full tank. The cheapest joins the trip as a stop in one gesture — five minutes of dwell included, the time it takes to fill up.",
        },
        Atout {
            emoji: "🍃",
            titre: "Low-emission zones, announced",
            texte: "When choosing the route, each variant names the LEZs it crosses — Greater Paris and its European cousins alike. The zone is named, the sticker stays your business: its rules change too often to be guessed.",
        },
        Atout {
            emoji: "🏍️",
            titre: "Motorbike mode",
            texte: "The same map, a different cost function: routes thought out for two wheels. And the “fun roads” option avoids the motorway and hunts for bends — for the ride, not for arriving early.",
        },
        Atout {
            emoji: "🚚",
            titre: "The van that avoids low bridges",
            texte: "Height, width and weight declared registration card in hand: bridges too low and streets closed to your dimensions drop out of the routes, even offline. The Pro tool for vans.",
        },
        Atout {
            emoji: "📋",
            titre: "The delivery run pasted in one block",
            texte: "Twenty addresses copied from your notes, one per line: each becomes a stop, the ones not found are shown instead of swallowed, and “Shortest order” sorts them out. The Pro tool for couriers.",
        },
        Atout {
            emoji: "🔧",
            titre: "Maintenance that reminds itself",
            texte: "Oil change, vehicle inspection, tyres: reminders set “in so many kilometres”, triggered by the kilometres actually driven in the log — a car wears out by driving, not by waiting.",
        },
        Atout {
            emoji: "👋",
            titre: "Say you're arriving",
            texte: "Ten minutes from the destination, the app offers the message ready to send — minutes left, live-tracking link included. Whoever is waiting no longer asks “where are you?”.",
        },
        Atout {
            emoji: "🏠",
            titre: "From the home screen",
            texte: "A Home / Work widget: one tap, and the routes are already being compared. It stores nothing — the app resolves the address on waking.",
        },
        Atout {
            emoji: "✨",
            titre: "My year, in one image",
            texte: "Kilometres, hours on the road, days behind the wheel, longest trip: the log tells its story on a card in the house colours, ready to share.",
        },
        Atout {
            emoji: "⭐",
            titre: "MonCap Plus",
            texte: "Fourteen days of trial, everything open — then a plan: Plus, €3.99 a month, for the whole app (offline, GPX); Pro, €9.99 a month, for optimized multi-stop runs, the mileage log and invoicing. Beyond that the app locks: everything is paid — that's the rule.",
        },
        Atout {
            emoji: "🧾",
            titre: "Kilometres that get invoiced",
            texte: "For people who drive for a living: the trip log exports as CSV, a PDF invoice is generated per month at the per-kilometre rate you choose, and the yearly tax mileage statement is drawn up at the official scale, electric uplift included — even with no signal.",
        },
        Atout {
            emoji: "🔁",
            titre: "Home and work follow the account",
            texte: "The favourites live in the account: a new iPhone gets them back on first sign-in. When they disagree, the device in your hands wins — it's the one you just changed.",
        },
        Atout {
            emoji: "📦",
            titre: "Your data, in one file",
            texte: "One button exports everything the account knows — trips, searches, reports, preferences — as one file handed over by the share sheet. Without the password, not even hashed: a derived secret belongs to no one.",
        },
        Atout {
            emoji: "⏰",
            titre: "Leave on time",
            texte: "Give the arrival time you want: the app subtracts the estimated trip and a five-minute margin, then sets the “Leave now” notification. The estimate is the one from that moment, and the sheet says so — promising a background recalculation would be lying.",
        },
        Atout {
            emoji: "🕙",
            titre: "Delivery time windows",
            texte: "“Before 10 am” goes on a stop, in the same menu as the dwell time. The passing time is estimated, and the chip turns orange the moment it slips. Anchored to the stop itself: it survives reordering. The Pro tool for couriers.",
        },
    ],
    hors_ligne_cartes: &[
        Atout {
            emoji: "🧭",
            titre: "Computed on the device",
            texte: "Routes are computed by your iPhone, not by a server. Missing an exit deep in a valley no longer leaves the screen stuck on “recalculating”: the route is redrawn at once, without a single bar of signal.",
        },
        Atout {
            emoji: "🗺️",
            titre: "Europe, region by region",
            texte: "From Monaco (1.1 MB) to Catalonia (320 MB), you only download what you drive through. Wider zones cover long trips — Paris → Marseille fits in a single pack.",
        },
        Atout {
            emoji: "🔀",
            titre: "Compare without signal",
            texte: "Routes are computed on the device, up to fifteen just as online, with their times, their turns and their tolls. And if you miss an exit, the recomputation starts from where you are — without a single bar of signal.",
        },
        Atout {
            emoji: "📥",
            titre: "Background download",
            texte: "A two-gigabyte pack doesn't require keeping the screen on. The transfer carries on while you do something else, and even after you leave the app.",
        },
        Atout {
            emoji: "💊",
            titre: "Places and limits, on board",
            texte: "The directory of the 19 original categories and the speed limits come down with the region, drawn from the same OpenStreetMap extract as the roads. Finding a pharmacy or reading the current limit works in airplane mode; the categories added since still need the network.",
        },
        Atout {
            emoji: "🔎",
            titre: "Search that finds in airplane mode",
            texte: "When the online search returns nothing, the directory of installed regions takes over: named places are searched by name, accents and case ignored, nearest first. Addresses remain the network's business — the directory only carries named places.",
        },
    ],
    tarifs: Tarifs {
        titre: "The price, said plainly",
        intro: "No tricks: fourteen days to try everything, then one monthly plan — Plus or Pro, depending on how you drive.",
        plus: Formule {
            nom: "MonCap Plus",
            prix: "3.99 €",
            periode: "per month, cancel anytime",
            points: &[
                "The whole app: guidance, comparison, transit",
                "212 offline regions — routes, places, speed limits",
                "GPX track import",
                "The cheapest fuel station along your route",
                "The instruction on your wrist, on Apple Watch",
            ],
        },
        pro: Formule {
            nom: "MonCap Pro",
            prix: "9.99 €",
            periode: "per month, for people who drive for a living",
            points: &[
                "Everything in MonCap Plus",
                "Optimized multi-stop runs — pasted in one block, one address per line",
                "Delivery time windows: the chip warns before it's late",
                "Mileage log (CSV) and tax mileage statement",
                "PDF invoicing at the per-kilometre rate you choose",
                "Van dimensions: low bridges stay out of the route",
            ],
        },
        note: "After the fourteen-day trial the app locks until you subscribe — everything is paid, that's the house rule. The subscription is taken out in the app and cancelled in iOS settings.",
    },
    preuve: Preuve {
        titre: "A measurement beats a promise",
        texte: "Bordeaux → Limoges, {mesure}, computed with no signal at all on the Nouvelle-Aquitaine pack. That trip crosses an old splitting boundary: the roads are stitched back together at build time, not patched on the phone.",
        mesure: "222.5 km in 2 h 16",
        note: "This check runs every week, against the pack actually published.",
    },
    pile_titre: "What's underneath",
    pile: &[
        Pile { domaine: "iOS app", outils: &["Swift", "SwiftUI", "MapKit", "MapLibre", "Core Location", "AVFoundation", "WatchConnectivity"] },
        Pile { domaine: "Server", outils: &["Rust", "Axum", "Tokio", "SeaORM", "PostgreSQL", "Redis"] },
        Pile { domaine: "Maps and routing", outils: &["Valhalla", "OpenStreetMap", "Overpass", "Osmium", "PMTiles", "IGN"] },
        Pile { domaine: "Live data", outils: &["TomTom", "open-meteo", "NASA FIRMS", "data.economie.gouv.fr"] },
        Pile { domaine: "Web", outils: &["React", "TypeScript", "Bun"] },
        Pile { domaine: "Deployment", outils: &["Heroku", "GitHub Actions", "Docker"] },
    ],
    franchise_titre: "What it doesn't do",
    franchise: &[
        "Tolls are estimated from the motorway share, at the average French class-1 rate — not the operator's exact tariff.",
        "Offline, the computation offers the variants it finds: on a short trip there are fewer than online. Neither live traffic nor reports are available.",
        "An offline trip must fit entirely within one pack: that is what the overlapping zones are for.",
        "The app follows the phone's language among French, Spanish and English; anywhere else it stays in French.",
        "The watch app shows what the iPhone sends it: with no guidance running on the phone, it waits.",
        "The app is in testing on TestFlight, not yet on the App Store.",
    ],
    legendes: Legendes {
        itineraires: "A drawing of the comparison screen — real layout and labels, example values.",
        guidage: "The manoeuvre banner, speed and limit, the weather, the control panel and the arrival bar — screen drawing, example values.",
        montre: "A drawing of the Apple Watch app during guidance — real layout, example values.",
        lieux: "All 42 categories, with the app's own labels — shown here in one go, whereas the strip scrolls on the phone.",
        regions: "Real names and sizes, read from the published Releases; only the progress is staged.",
    },
    reste: "and 12 more routes",
    maquette: Maquette { installee: "✓ Installed" },
    signature: "Designed and built by",
    credits: "Maps and road data OpenStreetMap (ODbL), routing Valhalla, traffic TomTom, weather open-meteo, fuel prices data.economie.gouv.fr, hotspots NASA FIRMS, aerial imagery IGN.",
    pied: Pied {
        slogan: "The GPS that compares, then guides you — with or without signal.",
        decouvrir: "Discover",
        ancres: &[
            Ancre { id: "comparateur-titre", nom: "The comparator" },
            Ancre { id: "guidage-titre", nom: "Guidance" },
            Ancre { id: "montre-titre", nom: "On your wrist" },
            Ancre { id: "lieux-titre", nom: "Around you" },
            Ancre { id: "atouts-titre", nom: "And also" },
            Ancre { id: "hors-ligne-titre", nom: "Offline" },
            Ancre { id: "tarifs-titre", nom: "Pricing" },
            Ancre { id: "preuve-titre", nom: "The proof" },
            Ancre { id: "pile-titre", nom: "Under the hood" },
            Ancre { id: "franchise-titre", nom: "What it doesn't do" },
        ],
        donnees: "Data and engines",
        legal_titre: "Legal",
        copyright: "All rights reserved.",
    },
    mentions: PageLegale {
        chemin: "/en/legal",
        titre: "Legal notice",
        description: "Publisher, hosting and data licences for the MonCap GPS site and app.",
        retour: "← Back to the site",
        sections: &[
            SectionLegale {
                titre: "Publisher",
                corps: &[
                    "This site and the MonCap GPS app are published by Maxime Nathan Lestage.",
                    "Contact: maxlestage@icloud.com.",
                ],
            },
            SectionLegale {
                titre: "Hosting",
                corps: &[
                    "The site and its server are hosted by Heroku (Salesforce, Inc.), 415 Mission Street, San Francisco, CA 94105, United States.",
                ],
            },
            SectionLegale {
                titre: "Intellectual property",
                corps: &[
                    "The code, texts and the MonCap GPS name belong to their author. Maps and road data come from OpenStreetMap, © OpenStreetMap contributors, under the ODbL licence.",
                    "The other data shown — traffic, weather, air quality, fuel prices, fire hotspots, aerial imagery — remain the property of their respective sources, each under its own licence.",
                ],
            },
        ],
    },
    confidentialite: PageLegale {
        chemin: "/en/privacy",
        titre: "Privacy",
        description: "What MonCap GPS collects, what stays on your phone, and your rights.",
        retour: "← Back to the site",
        sections: &[
            SectionLegale {
                titre: "This site",
                corps: &[
                    "This site sets no cookies, loads no scripts, and carries no trackers or analytics. The only requests your browser sends are the ones needed to display the page.",
                ],
            },
            SectionLegale {
                titre: "The account",
                corps: &[
                    "The app works with an account: your email address is your login, along with a password (stored hashed) and your recent destination searches. The address is used for nothing else: no mail apart from the password reset you ask for, no newsletter, no resale. Deleting the account deletes everything, address included.",
                ],
            },
            SectionLegale {
                titre: "Your position",
                corps: &[
                    "Your position is processed on the device for guidance. It is only sent to the server while you share your trip live, for the duration of that share. The Home and Work favourites stay on the phone.",
                ],
            },
            SectionLegale {
                titre: "Third parties",
                corps: &[
                    "Some of the data shown is requested directly from its sources — weather, air quality, elevation. Those services then receive the coordinates of the requested area, never your identity or your account.",
                ],
            },
            SectionLegale {
                titre: "Advertising",
                corps: &[
                    "No advertising, no resale of data, no analytics. The app does not live off your data.",
                ],
            },
            SectionLegale {
                titre: "Your rights",
                corps: &[
                    "You can request access to, correction of, or deletion of what is attached to your account by writing to maxlestage@icloud.com. Deleting the account deletes its data.",
                ],
            },
        ],
    },
};
