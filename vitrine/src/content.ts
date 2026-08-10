/**
 * Ce que la page dit de factuel, indépendamment de la langue.
 *
 * Deux règles tiennent ce fichier :
 *
 * 1. Ce qui est présenté comme une mesure en est une. Les poids des paquets
 *    viennent des Releases publiées, les 222,5 km de Bordeaux → Limoges d'un
 *    calcul réel sans réseau, le plafond de quinze itinéraires de la ligne
 *    `options.prefix(15)` du code de l'application. Une vitrine qui arrondit
 *    ses chiffres finit par mentir.
 * 2. Ce qui illustre l'interface est marqué comme tel. Les valeurs des
 *    maquettes sont plausibles mais inventées : c'est un dessin de l'écran,
 *    pas une capture.
 *
 * Les libellés qui apparaissent **dans** les maquettes restent en français
 * quelle que soit la langue de la page : ils dépeignent une application dont
 * l'interface est en français. Traduire un écran qui n'existe pas dans cette
 * langue serait une invention, et la légende le précise.
 */

export type Pile = {
  readonly domaine: string;
  readonly outils: readonly string[];
};

export type Atout = {
  readonly emoji: string;
  readonly titre: string;
  readonly texte: string;
};

export type Chiffre = {
  readonly valeur: string;
  readonly legende: string;
};

/** Plafond réel du comparateur : `options.prefix(15)` dans ContentView. */
export const ITINERAIRES_MAX = 15;

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
 * Noms et poids **réels**, lus sur les Releases publiées le 10 août 2026.
 * Seul l'avancement du téléchargement est mis en scène.
 */
export const MAQUETTE_REGIONS: readonly Region[] = [
  { nom: "France · Nouvelle-Aquitaine", poids: "499 Mo" },
  { nom: "France · Zone Paris–Méditerranée", poids: "1,7 Go" },
  { nom: "Spain · Cataluña", poids: "320 Mo", avancement: 62 },
  { nom: "Monaco", poids: "1,1 Mo" },
];

// --- Pile technique ------------------------------------------------------

/**
 * Les technologies principales, **sans numéro de version**.
 *
 * Une version affichée sur une page vieillit toute seule : elle est fausse
 * dès la mise à jour suivante, et personne ne pense à revenir la corriger.
 * Le nom, lui, reste vrai. Qui veut la version la lit dans `Cargo.toml` ou
 * `package.json`, où elle est tenue à jour par construction.
 */
export const OUTILS: readonly (readonly string[])[] = [
  ["Swift", "SwiftUI", "MapKit", "MapLibre", "Core Location", "AVFoundation"],
  ["Rust", "Axum", "Tokio", "SeaORM", "PostgreSQL", "Redis"],
  ["Valhalla", "OpenStreetMap", "Overpass", "Osmium", "PMTiles", "IGN"],
  ["TomTom", "open-meteo", "NASA FIRMS", "data.economie.gouv.fr"],
  ["React", "TypeScript", "Bun"],
  ["Heroku", "GitHub Actions", "Docker"],
];

// --- Constantes ----------------------------------------------------------

export const AUTEUR = "Maxime Nathan Lestage";

export const DEPOT = "https://github.com/maxlestage/moncap";

/**
 * Le front web, servi par le même backend que cette page. Chemin relatif et
 * non absolu : la vitrine tourne aussi bien sur un dyno Heroku qu'en local,
 * et coder le domaine ici la casserait ailleurs.
 */
export const CONNEXION = "/app";
