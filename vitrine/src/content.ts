/**
 * Le contenu de la page, séparé de sa mise en forme.
 *
 * Tout ce qui est chiffré ici a été mesuré, pas estimé : les tailles de
 * paquets viennent des Releases publiées, et les 222,5 km de l'épreuve
 * Bordeaux → Limoges d'un calcul réel sans réseau. Une vitrine qui arrondit
 * ses chiffres finit par mentir.
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

export const CHIFFRES: readonly Chiffre[] = [
  { valeur: "206", legende: "régions téléchargeables" },
  { valeur: "49", legende: "pays d'Europe" },
  { valeur: "0 €", legende: "aucun abonnement" },
];

export const ATOUTS: readonly Atout[] = [
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
  {
    emoji: "🔥",
    titre: "Carte des feux",
    texte:
      "Les foyers détectés par satellite, sur photo aérienne au vingt " +
      "centimètres. Utile l'été, sur les routes du Sud.",
  },
  {
    emoji: "🚗",
    titre: "À plusieurs, en direct",
    texte:
      "La position de ceux qui roulent avec vous, en temps réel, et les " +
      "signalements — police, accident, bouchon — partagés entre tous.",
  },
  {
    emoji: "🔊",
    titre: "Guidage vocal",
    texte:
      "Les annonces au bon moment, la carte qui pivote dans le sens de la " +
      "marche, et le recalcul automatique dès qu'on s'écarte.",
  },
];

/** Ce que l'app ne fait pas. Le dire évite une déception après coup. */
export const FRANCHISE: readonly string[] = [
  "Hors ligne, un seul itinéraire est proposé au lieu de plusieurs, et le trafic en direct n'est pas disponible.",
  "Un trajet hors réseau doit tenir entier dans un même paquet : c'est à cela que servent les zones, qui se chevauchent.",
  "L'application est en test sur TestFlight, pas encore sur l'App Store.",
];

export const DEPOT = "https://github.com/maxlestage/moncap";

/**
 * Le front web, servi par le même backend que cette page. Chemin relatif et
 * non absolu : la vitrine tourne aussi bien sur un dyno Heroku qu'en local,
 * et coder le domaine ici la casserait ailleurs.
 */
export const CONNEXION = "/app";
