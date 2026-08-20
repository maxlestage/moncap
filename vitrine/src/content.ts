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
 * Les libellés qui apparaissent **dans** les maquettes sont ceux de
 * l'application, traduits comme elle : `src/appstrings.ts` est engendré par
 * `frontend/traductions.py` à partir du tableau qui alimente le catalogue
 * iOS. Un écran dessiné ici dit donc exactement ce que l'app affiche, et ne
 * peut pas s'en écarter sans que la génération le montre.
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
  /** Clé d'étiquette, telle que l'application la construit. */
  readonly etiquette: string;
  readonly teinte: "verte" | "bleue" | "orange";
  /** minutes, kilomètres, virages, heure d'arrivée. */
  readonly trajet: readonly [number, number, number, string];
  /** Côte puis descente, en mètres. */
  readonly denivele?: readonly [number, number];
  /** Carburant seul, ou carburant + péages + total. */
  readonly cout: readonly [number] | readonly [number, number, number];
  /** Mètres de côte, quand elle mérite un avertissement. */
  readonly avertissement?: readonly [number];
};

/**
 * Les valeurs d'exemple d'un itinéraire. Elles sont versées dans les formats
 * de l'application (`appstrings.ts`) plutôt que recopiées en phrases toutes
 * faites : la maquette suit ainsi la langue **et** la façon dont l'app écrit
 * ses nombres — `String(format:)` sans locale met un point décimal, y compris
 * en français.
 */
export const MAQUETTE_ITINERAIRES: readonly Itineraire[] = [
  {
    etiquette: "Le plus simple",
    teinte: "verte",
    trajet: [28, 24.3, 6, "18:42"],
    cout: [2.6],
  },
  {
    etiquette: "Le plus rapide",
    teinte: "bleue",
    trajet: [22, 31.8, 11, "18:36"],
    cout: [3.4, 1.9, 5.3],
  },
  {
    etiquette: "Le plus court",
    teinte: "orange",
    trajet: [34, 21.7, 19, "18:48"],
    denivele: [180, 165],
    cout: [2.3],
    avertissement: [180],
  },
];

export const MODES: readonly string[] = ["À pied", "Vélo", "Voiture", "Transports"];

/** Le mode montré actif sur la maquette : les lignes dessinées — carburant,
 * péages — sont celles de la voiture, pas d'un tram. */
export const MODE_ACTIF = "Voiture";

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

/** Contact de l'éditeur, affiché dans les mentions légales. */
export const CONTACT = "maxlestage@icloud.com";

/**
 * Les sources de données et moteurs, avec l'adresse de chacun — le pied les
 * lie, la ligne de crédits les nomme. Pas de lien vers notre dépôt : il est
 * privé, un visiteur recevrait un 404 (voir liens.test.tsx).
 */
export type Source = { readonly nom: string; readonly url: string };

export const SOURCES: readonly Source[] = [
  { nom: "OpenStreetMap", url: "https://www.openstreetmap.org/copyright" },
  { nom: "Valhalla", url: "https://valhalla.github.io/valhalla/" },
  { nom: "Transitous", url: "https://transitous.org" },
  { nom: "Open-Meteo", url: "https://open-meteo.com" },
  { nom: "NASA FIRMS", url: "https://firms.modaps.eosdis.nasa.gov" },
  { nom: "data.economie.gouv.fr", url: "https://data.economie.gouv.fr" },
  { nom: "IGN", url: "https://www.ign.fr" },
  { nom: "TomTom", url: "https://www.tomtom.com" },
];

/**
 * Le front web, servi par le même backend que cette page. Chemin relatif et
 * non absolu : la vitrine tourne aussi bien sur un dyno Heroku qu'en local,
 * et coder le domaine ici la casserait ailleurs.
 */
export const CONNEXION = "/app";
