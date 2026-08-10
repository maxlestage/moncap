/**
 * Le contenu de la page, séparé de sa mise en forme.
 *
 * Deux règles tiennent ce fichier :
 *
 * 1. Ce qui est présenté comme une mesure en est une. Les tailles de paquets
 *    viennent des Releases publiées, les 222,5 km de Bordeaux → Limoges d'un
 *    calcul réel sans réseau. Une vitrine qui arrondit ses chiffres finit par
 *    mentir.
 * 2. Ce qui illustre l'interface est marqué comme tel (`MAQUETTE`). Les
 *    valeurs y sont plausibles mais inventées : c'est un dessin de l'écran,
 *    pas une capture. Le dire évite de faire passer un exemple pour un
 *    relevé.
 */

export type Atout = {
  readonly emoji: string;
  readonly titre: string;
  readonly texte: string;
};

export type Chiffre = {
  readonly valeur: string;
  readonly legende: string;
};

export type Pile = {
  readonly domaine: string;
  readonly outils: readonly string[];
};

export const CHIFFRES: readonly Chiffre[] = [
  { valeur: "19", legende: "catégories de lieux" },
  { valeur: "206", legende: "régions téléchargeables" },
  { valeur: "49", legende: "pays d'Europe" },
  { valeur: "0 €", legende: "aucun abonnement" },
];

// --- Comparateur d'itinéraires ------------------------------------------

export type Itineraire = {
  readonly etiquette: string;
  readonly teinte: "verte" | "bleue" | "orange";
  readonly duree: string;
  readonly distance: string;
  readonly virages: string;
  readonly arrivee: string;
  readonly denivele?: string;
  readonly cout: string;
  readonly avertissement?: string;
};

/**
 * Dessin de l'écran de comparaison. Les valeurs sont inventées — la mise en
 * page, les étiquettes et les lignes affichées reprennent en revanche
 * exactement celles de l'application.
 */
export const MAQUETTE_ITINERAIRES: readonly Itineraire[] = [
  {
    etiquette: "Le plus simple",
    teinte: "verte",
    duree: "28 min",
    distance: "24,3 km",
    virages: "6 virages",
    arrivee: "arr. 18:42",
    cout: "⛽ ~2,60 € · sans péage",
  },
  {
    etiquette: "Le plus rapide",
    teinte: "bleue",
    duree: "22 min",
    distance: "31,8 km",
    virages: "11 virages",
    arrivee: "arr. 18:36",
    cout: "⛽ ~3,40 € · 🛣️ péages ~1,90 € · total ~5,30 €",
  },
  {
    etiquette: "Le plus court",
    teinte: "orange",
    duree: "34 min",
    distance: "21,7 km",
    virages: "19 virages",
    arrivee: "arr. 18:48",
    denivele: "↗ 180 m de côte · ↘ 165 m de descente",
    cout: "⛽ ~2,30 € · sans péage",
    avertissement: "⚠️ Forte côte (180 m)",
  },
];

export const MODES: readonly string[] = ["À pied", "Vélo", "Voiture"];

// --- Bandeau des lieux ---------------------------------------------------

export type Lieu = { readonly emoji: string; readonly nom: string };

/** Les 19 catégories du bandeau, dans l'ordre et avec les libellés de l'app. */
export const LIEUX: readonly Lieu[] = [
  { emoji: "⛽", nom: "Essence" },
  { emoji: "🅿️", nom: "Parking" },
  { emoji: "⚡", nom: "Recharge" },
  { emoji: "🍽️", nom: "Restaurants" },
  { emoji: "🏨", nom: "Hôtels" },
  { emoji: "🛒", nom: "Supermarché" },
  { emoji: "💊", nom: "Pharmacie" },
  { emoji: "🏧", nom: "Distributeur" },
  { emoji: "🚻", nom: "Toilettes" },
  { emoji: "🚰", nom: "Eau potable" },
  { emoji: "♿", nom: "Handicapé" },
  { emoji: "🏊", nom: "Baignade" },
  { emoji: "🏄", nom: "Surf" },
  { emoji: "🛹", nom: "Skatepark" },
  { emoji: "🏕️", nom: "Camping" },
  { emoji: "⛰️", nom: "Point de vue" },
  { emoji: "🍔", nom: "Fast-food" },
  { emoji: "🏛️", nom: "Tourisme" },
  { emoji: "🎉", nom: "Sorties" },
];

// --- Régions hors ligne --------------------------------------------------

export type Region = {
  readonly nom: string;
  readonly poids: string;
  /** Pourcentage téléchargé, absent quand la région est déjà installée. */
  readonly avancement?: number;
};

/**
 * Dessin de l'écran des régions. Les noms et les poids sont **réels** : lus
 * sur les Releases publiées le 10 août 2026. Seul l'avancement du
 * téléchargement est mis en scène.
 */
export const MAQUETTE_REGIONS: readonly Region[] = [
  { nom: "France · Nouvelle-Aquitaine", poids: "499 Mo" },
  { nom: "France · Zone Paris–Méditerranée", poids: "1,7 Go" },
  { nom: "Spain · Cataluña", poids: "320 Mo", avancement: 62 },
  { nom: "Monaco", poids: "1,1 Mo" },
];

// --- Le reste ------------------------------------------------------------

export const ATOUTS: readonly Atout[] = [
  {
    emoji: "🚦",
    titre: "Le trafic et les signalements",
    texte:
      "Les bouchons en temps réel, et les signalements de ceux qui roulent " +
      "devant vous — police, accident, ralentissement. Chacun se confirme " +
      "ou s'infirme d'un geste, avec le décompte des deux.",
  },
  {
    emoji: "🚗",
    titre: "À plusieurs, en direct",
    texte:
      "La position de ceux qui font la route avec vous, en temps réel sur " +
      "la même carte. Et l'heure d'arrivée qui se partage en un bouton : " +
      "« J'arrive vers 18:42 ».",
  },
  {
    emoji: "🔥",
    titre: "Carte des feux",
    texte:
      "Les foyers détectés par satellite, sur photo aérienne au vingt " +
      "centimètres. Utile l'été, sur les routes du Sud.",
  },
  {
    emoji: "🔊",
    titre: "Guidage vocal",
    texte:
      "Les annonces au bon moment, la carte qui pivote dans le sens de la " +
      "marche, le recalcul dès qu'on s'écarte. Une navigation interrompue " +
      "se reprend là où elle s'était arrêtée.",
  },
];

/** Le hors ligne, qui a sa propre section : ses cartes y sont détaillées. */
export const HORS_LIGNE: readonly Atout[] = [
  {
    emoji: "🧭",
    titre: "Calculé sur l'appareil",
    texte:
      "Les itinéraires sont calculés par votre iPhone, pas par un serveur. " +
      "Rater une sortie au fond d'une vallée ne laisse plus l'écran figé sur " +
      "« recalcul » : la route se refait immédiatement, sans une barre de réseau.",
  },
  {
    emoji: "🗺️",
    titre: "L'Europe, région par région",
    texte:
      "De Monaco (1,1 Mo) à la Catalogne (320 Mo), on ne télécharge que ce " +
      "qu'on traverse. Des zones plus larges couvrent les longs trajets — " +
      "Paris → Marseille tient dans un seul paquet.",
  },
  {
    emoji: "📥",
    titre: "Téléchargement en arrière-plan",
    texte:
      "Un paquet de deux gigaoctets ne demande pas de garder l'écran allumé. " +
      "Le transfert continue pendant que vous faites autre chose, et même " +
      "après avoir quitté l'application.",
  },
];

/** Ce que l'app ne fait pas. Le dire évite une déception après coup. */
export const FRANCHISE: readonly string[] = [
  "Les péages sont estimés d'après la part d'autoroute, au tarif moyen français classe 1 — pas le tarif exact du concessionnaire.",
  "Hors ligne, un seul itinéraire est proposé au lieu de plusieurs, et ni le trafic ni les signalements ne sont disponibles.",
  "Un trajet hors réseau doit tenir entier dans un même paquet : c'est à cela que servent les zones, qui se chevauchent.",
  "L'application est en test sur TestFlight, pas encore sur l'App Store.",
];

/**
 * Les technologies principales, sans numéro de version.
 *
 * Une version affichée sur une page vieillit toute seule : elle est fausse
 * dès la mise à jour suivante, et personne ne pense à revenir la corriger.
 * Le nom, lui, reste vrai. Qui veut la version la lit dans `Cargo.toml` ou
 * `package.json`, où elle est tenue à jour par construction.
 *
 * Volontairement les principales, pas toutes : une liste exhaustive de
 * dépendances n'apprend rien à qui lit une vitrine.
 */
export const TECHNIQUES: readonly Pile[] = [
  {
    domaine: "Application iOS",
    outils: ["Swift", "SwiftUI", "MapKit", "MapLibre", "Core Location", "AVFoundation"],
  },
  {
    domaine: "Serveur",
    outils: ["Rust", "Axum", "Tokio", "SeaORM", "PostgreSQL", "Redis"],
  },
  {
    domaine: "Cartes et itinéraires",
    outils: ["Valhalla", "OpenStreetMap", "Overpass", "Osmium", "PMTiles", "IGN"],
  },
  {
    domaine: "Données en direct",
    outils: ["TomTom", "open-meteo", "NASA FIRMS", "data.economie.gouv.fr"],
  },
  {
    domaine: "Web",
    outils: ["React", "TypeScript", "Bun"],
  },
  {
    domaine: "Déploiement",
    outils: ["Heroku", "GitHub Actions", "Docker"],
  },
];

export const AUTEUR = "Maxime Nathan Lestage";

export const DEPOT = "https://github.com/maxlestage/moncap";

/**
 * Le front web, servi par le même backend que cette page. Chemin relatif et
 * non absolu : la vitrine tourne aussi bien sur un dyno Heroku qu'en local,
 * et coder le domaine ici la casserait ailleurs.
 */
export const CONNEXION = "/app";
