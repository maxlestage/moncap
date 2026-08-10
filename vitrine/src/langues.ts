/**
 * Les trois versions de la page : français, espagnol, anglais.
 *
 * Le type `Contenu` est la garantie : oublier une phrase dans une langue ne
 * produit pas une page à trou, mais une erreur de compilation. C'est la
 * raison d'être de ce fichier unique plutôt que de trois fichiers jumeaux
 * qui divergeraient en silence.
 *
 * Ce qui n'est **pas** traduit : les libellés visibles dans les maquettes.
 * Ils dépeignent une application dont l'interface est en français, et
 * inventer une version espagnole ou anglaise de ces écrans reviendrait à
 * montrer un produit qui n'existe pas. Chaque légende le dit.
 */
import type { Atout, Chiffre, Pile } from "./content";
import { ITINERAIRES_MAX, OUTILS } from "./content";

export type Langue = "fr" | "es" | "en";

export type Section = {
  readonly titre: string;
  readonly intro: string;
};

export type Contenu = {
  readonly code: Langue;
  /** Adresse de la page. Le français occupe la racine. */
  readonly chemin: string;
  /** Nom de la langue, écrit dans cette langue. */
  readonly nom: string;
  readonly titre: string;
  readonly description: string;
  /** Les deux lignes du titre principal ; la seconde est mise en valeur. */
  readonly h1: readonly [string, string];
  readonly chapeau: string;
  readonly seConnecter: string;
  readonly voirProjet: string;
  readonly theme: { readonly libelle: string; readonly noms: readonly string[] };
  readonly langueLibelle: string;
  readonly chiffres: readonly Chiffre[];
  readonly comparateur: Section & { readonly apres: string };
  readonly guidage: Section;
  readonly lieux: Section;
  readonly horsLigne: Section;
  readonly atoutsTitre: string;
  readonly atouts: readonly Atout[];
  readonly horsLigneCartes: readonly Atout[];
  readonly preuve: {
    readonly titre: string;
    readonly texte: string;
    readonly mesure: string;
    readonly note: string;
  };
  readonly pileTitre: string;
  readonly pile: readonly Pile[];
  readonly franchiseTitre: string;
  readonly franchise: readonly string[];
  readonly legendes: {
    readonly itineraires: string;
    readonly guidage: string;
    readonly lieux: string;
    readonly regions: string;

  };
  readonly reste: string;
  /**
   * Le seul texte de maquette qui ne vienne pas du catalogue de l'app :
   * « Installée » n'existe que sur ce dessin.
   */
  readonly maquette: {
    readonly installee: string;
  };
  readonly signature: string;
  readonly credits: string;
  readonly codeSource: string;
};

/** Assemble la pile en associant les libellés de domaine aux outils. */
function pile(domaines: readonly string[]): readonly Pile[] {
  return domaines.map((domaine, i) => ({
    domaine,
    outils: OUTILS[i] ?? [],
  }));
}

const FR: Contenu = {
  code: "fr",
  chemin: "/",
  nom: "Français",
  titre: "MonCap GPS — comparez, puis roulez, avec ou sans réseau",
  description:
    "Un GPS iOS qui compare jusqu'à 15 itinéraires — durée, virages, dénivelé, carburant, péages — puis vous guide. 206 régions d'Europe téléchargeables pour rouler sans réseau.",
  h1: ["Comparez, puis roulez", "avec ou sans réseau"],
  chapeau:
    "Un GPS pour iPhone qui met les itinéraires côte à côte — durée, virages, dénivelé, carburant, péages — puis vous guide. Téléchargez votre région une fois, et il continue de calculer tout seul : vallées, tunnels, étranger.",
  seConnecter: "Se connecter",
  voirProjet: "Voir le projet",
  theme: {
    libelle: "Thème",
    noms: ["Auto", "Clair", "Sombre", "Marine"],
  },
  langueLibelle: "Langue",
  chiffres: [
    { valeur: String(ITINERAIRES_MAX), legende: "itinéraires au maximum" },
    { valeur: "19", legende: "catégories de lieux" },
    { valeur: "206", legende: "régions téléchargeables" },
    { valeur: "49", legende: "pays d'Europe" },
    { valeur: "0 €", legende: "aucun abonnement" },
  ],
  comparateur: {
    titre: "Le trajet vous est présenté, pas imposé",
    intro:
      "Jusqu'à 15 itinéraires côte à côte, chacun étiqueté le plus simple, le plus rapide ou le plus court. Pour chacun : la durée, la distance, le nombre de virages, l'heure d'arrivée, le dénivelé — et ce que ça coûte.",
    apres:
      "Le carburant est calculé avec la consommation de votre véhicule et le prix réel des stations autour de vous (open data officiel), pas une moyenne nationale. Les péages s'ajoutent quand l'itinéraire en emprunte. Une forte côte est signalée avant le départ, pas découverte au milieu. À pied, à vélo ou en voiture, et l'on peut demander d'éviter péages ou autoroutes.",
  },
  guidage: {
    titre: "Puis il vous emmène",
    intro:
      "La flèche suit la manœuvre réelle, la distance au prochain virage se met à jour, et le numéro de sortie s'affiche sous la consigne. La vitesse du moment est comparée à la limite de la route, lue sur OpenStreetMap : elle passe au rouge quand vous la dépassez.",
  },
  lieux: {
    titre: "Ce qu'il y a autour",
    intro:
      "Un bandeau de catégories au-dessus de la carte : une touche, et les lieux apparaissent dans la zone visible. L'essence affiche le prix à la pompe pour le carburant que vous avez choisi.",
  },
  horsLigne: {
    titre: "Quand le réseau tombe",
    intro:
      "206 régions dans 49 pays d'Europe, téléchargeables une par une. Plusieurs peuvent cohabiter sur le téléphone, et le transfert d'un paquet ne demande pas de rester devant l'écran.",
  },
  atoutsTitre: "Et aussi",
  atouts: [
    {
      emoji: "🚦",
      titre: "Le trafic et les signalements",
      texte:
        "Les bouchons en temps réel, et les signalements de ceux qui roulent devant vous — police, accident, ralentissement. Chacun se confirme ou s'infirme d'un geste, avec le décompte des deux.",
    },
    {
      emoji: "🚗",
      titre: "À plusieurs, en direct",
      texte:
        "La position de ceux qui font la route avec vous, en temps réel sur la même carte. Et l'heure d'arrivée qui se partage en un bouton : « J'arrive vers 18:42 ».",
    },
    {
      emoji: "🔥",
      titre: "Carte des feux",
      texte:
        "Les foyers détectés par satellite, sur photo aérienne au vingt centimètres. Utile l'été, sur les routes du Sud.",
    },
    {
      emoji: "🔊",
      titre: "Guidage vocal",
      texte:
        "Les annonces au bon moment, la carte qui pivote dans le sens de la marche, le recalcul dès qu'on s'écarte. Une navigation interrompue se reprend là où elle s'était arrêtée.",
    },
  ],
  horsLigneCartes: [
    {
      emoji: "🧭",
      titre: "Calculé sur l'appareil",
      texte:
        "Les itinéraires sont calculés par votre iPhone, pas par un serveur. Rater une sortie au fond d'une vallée ne laisse plus l'écran figé sur « recalcul » : la route se refait immédiatement, sans une barre de réseau.",
    },
    {
      emoji: "🗺️",
      titre: "L'Europe, région par région",
      texte:
        "De Monaco (1,1 Mo) à la Catalogne (320 Mo), on ne télécharge que ce qu'on traverse. Des zones plus larges couvrent les longs trajets — Paris → Marseille tient dans un seul paquet.",
    },
    {
      emoji: "📥",
      titre: "Téléchargement en arrière-plan",
      texte:
        "Un paquet de deux gigaoctets ne demande pas de garder l'écran allumé. Le transfert continue pendant que vous faites autre chose, et même après avoir quitté l'application.",
    },
  ],
  preuve: {
    titre: "Une mesure vaut mieux qu'une promesse",
    texte:
      "Bordeaux → Limoges, {mesure}, calculé sans aucun réseau sur le paquet Nouvelle-Aquitaine. Ce trajet traverse une ancienne frontière de découpage : les routes y sont recousues à la construction, pas rafistolées sur le téléphone.",
    mesure: "222,5 km en 2 h 16",
    note: "Cette vérification tourne chaque semaine, sur le paquet réellement publié.",
  },
  pileTitre: "Ce qu'il y a dessous",
  pile: pile([
    "Application iOS",
    "Serveur",
    "Cartes et itinéraires",
    "Données en direct",
    "Web",
    "Déploiement",
  ]),
  franchiseTitre: "Ce qu'elle ne fait pas",
  franchise: [
    "Les péages sont estimés d'après la part d'autoroute, au tarif moyen français classe 1 — pas le tarif exact du concessionnaire.",
    "Hors ligne, un seul itinéraire est proposé au lieu de quinze, et ni le trafic ni les signalements ne sont disponibles.",
    "Un trajet hors réseau doit tenir entier dans un même paquet : c'est à cela que servent les zones, qui se chevauchent.",
    "L'application suit la langue du téléphone parmi le français, l'espagnol et l'anglais ; ailleurs, elle reste en français.",
    "L'application est en test sur TestFlight, pas encore sur l'App Store.",
  ],
  legendes: {
    itineraires:
      "Dessin de l'écran de comparaison — mise en page et étiquettes réelles, valeurs d'exemple.",
    guidage:
      "Le bandeau de manœuvre, la vitesse et la limite du moment, la météo — dessin d'écran, valeurs d'exemple.",
    lieux:
      "Les 19 catégories, avec les libellés de l'application — montrées ici d'un seul tenant, alors que le bandeau défile sur le téléphone.",
    regions:
      "Noms et poids réels, lus sur les Releases publiées ; seul l'avancement est mis en scène.",
  },
  reste: "et 12 autres itinéraires",
  maquette: {
    installee: "✓ Installée",
  },
  signature: "Conçu et développé par",
  credits:
    "Cartes et données routières OpenStreetMap (ODbL), calcul d'itinéraire Valhalla, trafic TomTom, météo open-meteo, prix des carburants data.economie.gouv.fr, foyers NASA FIRMS, photos aériennes IGN.",
  codeSource: "Code source",
};

const ES: Contenu = {
  code: "es",
  chemin: "/es",
  nom: "Español",
  titre: "MonCap GPS — compara y conduce, con o sin cobertura",
  description:
    "Un GPS para iOS que compara hasta 15 rutas — duración, giros, desnivel, combustible, peajes — y luego te guía. 206 regiones de Europa descargables para conducir sin cobertura.",
  h1: ["Compara y conduce", "con o sin cobertura"],
  chapeau:
    "Un GPS para iPhone que pone las rutas una al lado de otra — duración, giros, desnivel, combustible, peajes — y luego te guía. Descarga tu región una vez y sigue calculando solo: valles, túneles, extranjero.",
  seConnecter: "Iniciar sesión",
  voirProjet: "Ver el proyecto",
  theme: {
    libelle: "Tema",
    noms: ["Auto", "Claro", "Oscuro", "Marino"],
  },
  langueLibelle: "Idioma",
  chiffres: [
    { valeur: String(ITINERAIRES_MAX), legende: "rutas como máximo" },
    { valeur: "19", legende: "categorías de lugares" },
    { valeur: "206", legende: "regiones descargables" },
    { valeur: "49", legende: "países de Europa" },
    { valeur: "0 €", legende: "sin suscripción" },
  ],
  comparateur: {
    titre: "La ruta se te propone, no se te impone",
    intro:
      "Hasta 15 rutas una al lado de otra, cada una etiquetada como la más sencilla, la más rápida o la más corta. Para cada una: la duración, la distancia, el número de giros, la hora de llegada, el desnivel — y lo que cuesta.",
    apres:
      "El combustible se calcula con el consumo de tu vehículo y el precio real de las gasolineras a tu alrededor (datos públicos oficiales), no con una media nacional. Los peajes se suman cuando la ruta los usa. Una cuesta pronunciada se avisa antes de salir, no se descubre a mitad de camino. A pie, en bici o en coche, y se puede pedir evitar peajes o autopistas.",
  },
  guidage: {
    titre: "Y luego te lleva",
    intro:
      "La flecha sigue la maniobra real, la distancia al siguiente giro se actualiza, y el número de salida aparece bajo la indicación. La velocidad del momento se compara con el límite de la vía, leído en OpenStreetMap: se pone en rojo cuando lo superas.",
  },
  lieux: {
    titre: "Lo que hay alrededor",
    intro:
      "Una barra de categorías sobre el mapa: un toque, y los lugares aparecen en la zona visible. La gasolina muestra el precio en el surtidor para el carburante que hayas elegido.",
  },
  horsLigne: {
    titre: "Cuando se cae la cobertura",
    intro:
      "206 regiones en 49 países de Europa, descargables una a una. Varias pueden convivir en el teléfono, y la transferencia de un paquete no exige quedarse delante de la pantalla.",
  },
  atoutsTitre: "Y además",
  atouts: [
    {
      emoji: "🚦",
      titre: "El tráfico y los avisos",
      texte:
        "Los atascos en tiempo real, y los avisos de quienes circulan por delante — policía, accidente, retención. Cada uno se confirma o se desmiente con un gesto, con el recuento de ambos.",
    },
    {
      emoji: "🚗",
      titre: "En grupo, en directo",
      texte:
        "La posición de quienes hacen el camino contigo, en tiempo real sobre el mismo mapa. Y la hora de llegada que se comparte con un botón: «Llego sobre las 18:42».",
    },
    {
      emoji: "🔥",
      titre: "Mapa de incendios",
      texte:
        "Los focos detectados por satélite, sobre foto aérea de veinte centímetros. Útil en verano, en las carreteras del sur.",
    },
    {
      emoji: "🔊",
      titre: "Guía por voz",
      texte:
        "Los avisos en el momento justo, el mapa que gira en el sentido de la marcha, el recálculo en cuanto te desvías. Una navegación interrumpida se retoma donde se quedó.",
    },
  ],
  horsLigneCartes: [
    {
      emoji: "🧭",
      titre: "Calculado en el dispositivo",
      texte:
        "Las rutas las calcula tu iPhone, no un servidor. Perder una salida en el fondo de un valle ya no deja la pantalla congelada en «recalculando»: la ruta se rehace al instante, sin una sola barra de cobertura.",
    },
    {
      emoji: "🗺️",
      titre: "Europa, región por región",
      texte:
        "De Mónaco (1,1 MB) a Cataluña (320 MB), solo se descarga lo que se atraviesa. Zonas más amplias cubren los trayectos largos — París → Marsella cabe en un solo paquete.",
    },
    {
      emoji: "📥",
      titre: "Descarga en segundo plano",
      texte:
        "Un paquete de dos gigabytes no obliga a mantener la pantalla encendida. La transferencia continúa mientras haces otra cosa, e incluso después de cerrar la aplicación.",
    },
  ],
  preuve: {
    titre: "Una medición vale más que una promesa",
    texte:
      "Burdeos → Limoges, {mesure}, calculado sin cobertura alguna sobre el paquete Nouvelle-Aquitaine. Ese trayecto cruza una antigua frontera de recorte: las carreteras se recosen al construir el paquete, no se remiendan en el teléfono.",
    mesure: "222,5 km en 2 h 16",
    note: "Esta comprobación se ejecuta cada semana, sobre el paquete realmente publicado.",
  },
  pileTitre: "Lo que hay debajo",
  pile: pile([
    "Aplicación iOS",
    "Servidor",
    "Mapas y rutas",
    "Datos en directo",
    "Web",
    "Despliegue",
  ]),
  franchiseTitre: "Lo que no hace",
  franchise: [
    "Los peajes se estiman según el tramo de autopista, a la tarifa media francesa de clase 1 — no es la tarifa exacta de la concesionaria.",
    "Sin cobertura se propone una sola ruta en lugar de quince, y no hay ni tráfico ni avisos.",
    "Un trayecto sin cobertura debe caber entero en un mismo paquete: para eso sirven las zonas, que se solapan.",
    "La aplicación sigue el idioma del teléfono entre francés, español e inglés; en cualquier otro, se queda en francés.",
    "La aplicación está en pruebas en TestFlight, todavía no en la App Store.",
  ],
  legendes: {
    itineraires:
      "Dibujo de la pantalla de comparación — maquetación y etiquetas reales, valores de ejemplo.",
    guidage:
      "La banda de maniobra, la velocidad y el límite del momento, el tiempo — dibujo de pantalla, valores de ejemplo.",
    lieux:
      "Las 19 categorías, con los rótulos de la aplicación — mostradas aquí de una sola vez, mientras que en el teléfono la barra se desplaza.",
    regions:
      "Nombres y pesos reales, leídos en las Releases publicadas; solo el progreso está recreado.",
  },
  reste: "y 12 rutas más",
  maquette: {
    installee: "✓ Instalada",
  },
  signature: "Diseñado y desarrollado por",
  credits:
    "Mapas y datos viarios OpenStreetMap (ODbL), cálculo de rutas Valhalla, tráfico TomTom, meteorología open-meteo, precios de carburantes data.economie.gouv.fr, focos NASA FIRMS, fotos aéreas IGN.",
  codeSource: "Código fuente",
};

const EN: Contenu = {
  code: "en",
  chemin: "/en",
  nom: "English",
  titre: "MonCap GPS — compare, then drive, with or without signal",
  description:
    "An iOS GPS that compares up to 15 routes — time, turns, elevation, fuel, tolls — then guides you. 206 downloadable European regions for driving with no signal.",
  h1: ["Compare, then drive", "with or without signal"],
  chapeau:
    "An iPhone GPS that lays the routes side by side — time, turns, elevation, fuel, tolls — then guides you. Download your region once and it keeps computing on its own: valleys, tunnels, abroad.",
  seConnecter: "Sign in",
  voirProjet: "See the project",
  theme: {
    libelle: "Theme",
    noms: ["Auto", "Light", "Dark", "Navy"],
  },
  langueLibelle: "Language",
  chiffres: [
    { valeur: String(ITINERAIRES_MAX), legende: "routes at most" },
    { valeur: "19", legende: "place categories" },
    { valeur: "206", legende: "downloadable regions" },
    { valeur: "49", legende: "European countries" },
    { valeur: "0 €", legende: "no subscription" },
  ],
  comparateur: {
    titre: "The route is offered, not imposed",
    intro:
      "Up to 15 routes side by side, each labelled the simplest, the fastest or the shortest. For every one: time, distance, number of turns, arrival time, elevation — and what it costs.",
    apres:
      "Fuel is worked out from your vehicle's consumption and the real price at the stations around you (official open data), not a national average. Tolls are added when the route uses them. A steep climb is flagged before you leave, not discovered halfway. On foot, by bike or by car, and you can ask to avoid tolls or motorways.",
  },
  guidage: {
    titre: "Then it takes you there",
    intro:
      "The arrow follows the actual manoeuvre, the distance to the next turn updates, and the exit number appears under the instruction. Your current speed is compared with the road's limit, read from OpenStreetMap: it turns red when you go over.",
  },
  lieux: {
    titre: "What's around you",
    intro:
      "A category strip above the map: one tap and the places appear in the visible area. Fuel shows the pump price for the grade you picked.",
  },
  horsLigne: {
    titre: "When the signal drops",
    intro:
      "206 regions across 49 European countries, downloadable one by one. Several can live on the phone at once, and transferring a pack doesn't mean sitting in front of the screen.",
  },
  atoutsTitre: "And also",
  atouts: [
    {
      emoji: "🚦",
      titre: "Traffic and reports",
      texte:
        "Live jams, and reports from those driving ahead of you — police, crash, slowdown. Each one is confirmed or denied with a tap, with both counts shown.",
    },
    {
      emoji: "🚗",
      titre: "Together, live",
      texte:
        "The position of whoever is driving with you, live on the same map. And an arrival time you share with one button: “I'll be there around 18:42”.",
    },
    {
      emoji: "🔥",
      titre: "Wildfire map",
      texte:
        "Satellite-detected hotspots, over twenty-centimetre aerial imagery. Useful in summer, on southern roads.",
    },
    {
      emoji: "🔊",
      titre: "Voice guidance",
      texte:
        "Announcements at the right moment, the map turning with your direction of travel, a recalculation the moment you stray. An interrupted trip resumes where it stopped.",
    },
  ],
  horsLigneCartes: [
    {
      emoji: "🧭",
      titre: "Computed on the device",
      texte:
        "Routes are computed by your iPhone, not by a server. Missing an exit deep in a valley no longer leaves the screen stuck on “recalculating”: the route is redrawn at once, without a single bar of signal.",
    },
    {
      emoji: "🗺️",
      titre: "Europe, region by region",
      texte:
        "From Monaco (1.1 MB) to Catalonia (320 MB), you only download what you drive through. Wider zones cover long trips — Paris → Marseille fits in a single pack.",
    },
    {
      emoji: "📥",
      titre: "Background download",
      texte:
        "A two-gigabyte pack doesn't require keeping the screen on. The transfer carries on while you do something else, and even after you leave the app.",
    },
  ],
  preuve: {
    titre: "A measurement beats a promise",
    texte:
      "Bordeaux → Limoges, {mesure}, computed with no signal at all on the Nouvelle-Aquitaine pack. That trip crosses an old splitting boundary: the roads are stitched back together at build time, not patched on the phone.",
    mesure: "222.5 km in 2 h 16",
    note: "This check runs every week, against the pack actually published.",
  },
  pileTitre: "What's underneath",
  pile: pile([
    "iOS app",
    "Server",
    "Maps and routing",
    "Live data",
    "Web",
    "Deployment",
  ]),
  franchiseTitre: "What it doesn't do",
  franchise: [
    "Tolls are estimated from the motorway share, at the average French class-1 rate — not the operator's exact tariff.",
    "Offline, a single route is offered instead of fifteen, and neither traffic nor reports are available.",
    "An offline trip must fit entirely within one pack: that is what the overlapping zones are for.",
    "The app follows the phone's language among French, Spanish and English; anywhere else it stays in French.",
    "The app is in testing on TestFlight, not yet on the App Store.",
  ],
  legendes: {
    itineraires:
      "A drawing of the comparison screen — real layout and labels, example values.",
    guidage:
      "The manoeuvre banner, current speed and limit, the weather — screen drawing, example values.",
    lieux:
      "All 19 categories, with the app's own labels — shown here in one go, whereas the strip scrolls on the phone.",
    regions:
      "Real names and sizes, read from the published Releases; only the progress is staged.",
  },
  reste: "and 12 more routes",
  maquette: {
    installee: "✓ Installed",
  },
  signature: "Designed and built by",
  credits:
    "Maps and road data OpenStreetMap (ODbL), routing Valhalla, traffic TomTom, weather open-meteo, fuel prices data.economie.gouv.fr, hotspots NASA FIRMS, aerial imagery IGN.",
  codeSource: "Source code",
};

export const LANGUES: readonly Contenu[] = [FR, ES, EN];
