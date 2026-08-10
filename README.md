# MonCap GPS

Application GPS minimale : un backend **Rust + Axum** et un front **Swift / SwiftUI** (iOS).

## Architecture

```
moncap/
├── Cargo.toml      API Rust + Axum (crate à la racine, pour le buildpack Heroku)
├── src/main.rs
├── Procfile        web: ./target/release/moncap-gps
├── app.json        buildpack Rust + addon Postgres (bouton « Deploy »)
├── web/            Front web React + Bun (voir web/README.md)
├── vitrine/        Page de présentation, rendue à la construction (vitrine/README.md)
└── frontend/       App iOS SwiftUI
    └── MonCapGPS/
```

## Backend (Rust + Axum)

Les routes sont volontairement aussi simples que possible :

| Méthode | Route         | Rôle                                              |
|---------|---------------|---------------------------------------------------|
| GET     | `/health`     | Vérifie que le serveur répond (`ok`)              |
| GET     | `/positions`  | Liste les positions enregistrées                  |
| POST    | `/positions`  | Ajoute une position (`{lat, lon, label}`)         |
| PUT     | `/positions/:id` | Met à jour une position (404 si absente)       |
| DELETE  | `/positions/:id` | Supprime une position (204, ou 404 si absente) |
| GET     | `/positions/nearest?lat=&lon=` | Position enregistrée la plus proche d'un point |
| POST    | `/positions/import` | Importe des positions depuis un corps GPX |
| GET     | `/positions.gpx` | Exporte les positions au format GPX (waypoints) |
| GET     | `/stats`      | Vue d'ensemble : nombre, longueur, boîte englobante, centroïde |
| POST    | `/route`      | Distance (km) + cap (°) entre deux points         |
| POST    | `/route/multi` | Distance totale + durée estimée d'un itinéraire (`{points:[...], speed_kmh?}`) |
| GET     | `/ws`         | WebSocket temps réel (positions, voitures live, signalements) |

### Temps réel (WebSocket)

`/ws` diffuse à tous les clients connectés :

- `positions_changed` — émis à chaque ajout/modif/suppression/import → les
  clients rechargent leur liste automatiquement.
- `live` / `live_gone` — positions GPS en direct des utilisateurs qui
  partagent leur localisation (marqueurs mobiles), expirées côté client.
- `alert` / `alerts` — signalements (police, accident, bouchon, danger),
  diffusés à tous et renvoyés en instantané à la connexion (gardés ~30 min).

Le client envoie `{"kind":"live", lat, lon, label}` ou
`{"kind":"alert", category, lat, lon, label}`.

Les données sont persistées dans **Postgres** via **SeaORM**. Le schéma est
géré par des **migrations versionnées** (`sea-orm-migration`, dossier
`src/migration/`) appliquées automatiquement au démarrage. Le calcul de
trajet utilise la formule de Haversine pour la distance et le cap initial.

### Lancer le backend

```bash
# 1. Démarrer Postgres (via Docker)
docker compose up -d db

# 2. Lancer l'API (depuis la racine du dépôt)
cp .env.example .env            # ou exporter DATABASE_URL
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/moncap"
cargo run
# moncap-gps écoute sur http://0.0.0.0:3000
```

La connexion est configurable via la variable d'environnement
`DATABASE_URL` (valeur par défaut :
`postgres://postgres:postgres@localhost:5432/moncap`).

**Comptes** : `POST /auth/signup` et `POST /auth/login` renvoient un jeton
**JWT** (Bearer) ; toutes les routes `positions*`/`stats` exigent ce jeton et
sont **isolées par utilisateur**. Définis `JWT_SECRET` en production (Heroku
config var) ; le WebSocket et l'export GPX acceptent le jeton en query
(`?token=`).

**Limitation de débit** par IP (clé sur `X-Forwarded-For` derrière Heroku) :
~300 req/min en général et 20 req/min sur `/auth` (anti brute-force) → `429`
au-delà.

Le niveau de logs (structurés via `tracing`) se règle avec `RUST_LOG`
(défaut : `info,sqlx=warn`). Les calculs de distances (`/stats`,
`/positions/nearest`, `/route/multi`) sont parallélisés avec **rayon**, et
le binaire de production est `strip`é pour rester léger.

### Exemples

```bash
curl localhost:3000/health

curl -X POST localhost:3000/positions \
  -H 'content-type: application/json' \
  -d '{"lat":48.8566,"lon":2.3522,"label":"Paris"}'
# {"id":1,"lat":48.8566,"lon":2.3522,"label":"Paris"}

curl localhost:3000/positions

curl -X DELETE localhost:3000/positions/1   # -> 204 (ou 404 si absente)

curl -X POST localhost:3000/route \
  -H 'content-type: application/json' \
  -d '{"from":{"lat":48.8566,"lon":2.3522},"to":{"lat":45.7640,"lon":4.8357}}'
# {"distance_km":391.5,"bearing_deg":150.5}

curl -X POST localhost:3000/route/multi \
  -H 'content-type: application/json' \
  -d '{"points":[{"lat":48.8566,"lon":2.3522},{"lat":45.7640,"lon":4.8357},{"lat":43.2965,"lon":5.3698}],"speed_kmh":100}'
# {"total_km":669.1,"legs_km":[391.5,277.6],"duration_min":401.5}

curl "localhost:3000/positions/nearest?lat=47.32&lon=5.04"
# {"position":{"id":2,...,"label":"Lyon"},"distance_km":173.7}

curl localhost:3000/positions.gpx
# <?xml ...><gpx ...><wpt lat="48.8566" lon="2.3522"><name>Paris</name></wpt>...</gpx>

curl -X PUT localhost:3000/positions/1 \
  -H 'content-type: application/json' \
  -d '{"lat":48.85,"lon":2.35,"label":"Paris Centre"}'

curl -X POST localhost:3000/positions/import \
  -H 'content-type: application/gpx+xml' \
  --data-binary '<gpx><wpt lat="45.764" lon="4.8357"><name>Lyon</name></wpt></gpx>'

curl localhost:3000/stats
# {"count":2,"total_km":390.9,"bbox":{...},"centroid":{"lat":47.3,"lon":3.6}}
```

## Cartes hors ligne (tuiles)

L'app peut télécharger des zones de carte pour les consulter sans réseau
(écran « Carte hors ligne »). Apple Plans ne permettant ni de télécharger ni
de conserver ses tuiles, cela repose sur une **seconde source, en images**.

Le backend expose un relais : `GET /tiles/:z/:x/:y`, configuré par deux
variables d'environnement.

| Variable | Rôle |
| --- | --- |
| `TILES_PMTILES` | **Archive PMTiles auto-hébergée** (URL). Prioritaire : c'est le mode conforme, où l'on sert ses propres tuiles. |
| `TILES_UPSTREAM` | Serveur amont relayé, format `https://…/{z}/{x}/{y}.png`. Utilisé si aucune archive n'est définie. |
| `TILES_GLYPHS` | Polices des libellés (`https://…/{fontstack}/{range}.pbf`). Non défini → carte sans libellés. |
| `TILES_USER_AGENT` | Identification envoyée à l'amont. La plupart des fournisseurs refusent les requêtes sans `User-Agent` identifiable. |

Le relais borne `z`, `x` et `y` (sinon n'importe qui pourrait lui faire
relayer des requêtes arbitraires), garde 1500 tuiles en cache mémoire et
dispose de sa propre limite de débit — la limite générale de 300 req/min
couperait un téléchargement de zone au bout de 300 tuiles.

### ⚠️ Ne pas pointer sur le serveur public d'OpenStreetMap

Le règlement d'OpenStreetMap **interdit le téléchargement de zones
entières**. Via un relais, toutes les requêtes sortent de surcroît d'une
seule adresse IP : le blocage du serveur est l'issue la plus probable.

### Héberger ses propres tuiles à partir d'OpenStreetMap

C'est en revanche parfaitement autorisé, les données étant sous licence
ODbL (attribution « © OpenStreetMap contributors », déjà affichée dans
l'app). Deux chemins, selon le format voulu.

**Le plus simple : une archive PMTiles.** `planetiler` produit, en une
commande et sans base de données, un **fichier unique** à partir d'un extrait
OpenStreetMap. Le backend sait le lire par plages d'octets (`TILES_PMTILES`),
il suffit donc de l'héberger quelque part qui accepte les requêtes de plage —
tout stockage objet le fait.

```bash
# 1. Extrait régional -> une archive .pmtiles (quelques minutes)
java -Xmx8g -jar planetiler.jar --download --area=aquitaine \
     --output=aquitaine.pmtiles

# 2. Publier le fichier sur un stockage objet, puis :
heroku config:set \
  TILES_PMTILES='https://mon-stockage.example/aquitaine.pmtiles' \
  -a mon-app
```

`planetiler` produit des tuiles **vectorielles**, que l'app rend avec
**MapLibre** (écran « Carte hors ligne »). Le serveur fournit la feuille de
style correspondante sur `GET /tiles/style.json`, et le bouton « Utiliser le
serveur MonCap » des réglages configure l'app en un appui.

Les **libellés** (noms de villes et de rues) sont omis par défaut : ils
exigent des polices matricielles à héberger. Renseigner `TILES_GLYPHS`
(format `https://…/{fontstack}/{range}.pbf`) les active.

**Tuiles images.** Le rendu demande une
chaîne complète — base PostGIS + moteur de rendu — qui ne tient pas dans un
dyno Heroku. On la fait tourner **une fois**, sur une machine de travail, et
l'on publie le résultat sur un hébergement statique :

```bash
# 1. Extrait régional (exemple : Aquitaine)
wget https://download.geofabrik.de/europe/france/aquitaine-latest.osm.pbf

# 2. Rendu en tuiles images — voir switch2osm.org pour la chaîne complète
#    (osm2pgsql + renderd/mod_tile). Compter plusieurs heures.

# 3. Publier le dossier {z}/{x}/{y}.png sur un hébergement statique,
#    puis pointer le relais dessus :
heroku config:set \
  TILES_UPSTREAM='https://mon-stockage.example/{z}/{x}/{y}.png' \
  -a mon-app
```

Ordres de grandeur : une région française jusqu'au zoom 13 représente
quelques milliers de tuiles (quelques centaines de Mo) ; aller jusqu'au zoom
16 multiplie ce volume par 15 environ.

## Itinéraires hors ligne (tuiles Valhalla)

À ne pas confondre avec ce qui précède : les cartes hors ligne servent à
**voir**, celles-ci à **calculer**. Ce sont deux jeux de données distincts.
`MKDirections` exige une connexion, si bien que rater une sortie hors
couverture laissait l'app sans rien à proposer. **Valhalla**, embarqué,
calcule l'itinéraire sur l'appareil — à condition de disposer de son propre
graphe routier.

Personne ne distribue ces tuiles gratuitement : le seul fournisseur prêt à
l'emploi demande plusieurs centaines de dollars par mois. Un runner GitHub
suffit pourtant, et les Releases les hébergent sans frais.

- **Workflow** `.github/workflows/valhalla-tiles.yml`, manuel. L'entrée
  `regions` accepte des zones Geofabrik (`france,portugal`), `europe`, ou
  `grandes-regions`.
- **Découpage** lu sur l'index Geofabrik, jamais recopié à la main : au-delà
  de 700 Mo d'extrait on descend d'un cran, ce qui tient chaque archive sous
  les 2 Gio d'une pièce jointe de Release. L'Europe fait ainsi 215 zones dans
  49 pays ; en France, les 13 grandes régions d'aujourd'hui.
- **L'app ne connaît pas la liste** : elle la lit sur les Releases, ne
  retenant que les étiquettes préfixées `tuiles-eu-`. Le titre de la Release
  porte le nom lisible, sa pièce jointe `.tar` le poids réel. Deux cents
  entrées tenues à la main seraient fausses dès la construction suivante.
- **Vérification** `.github/workflows/valhalla-epreuve.yml`, chaque lundi :
  Bordeaux → Limoges sur le paquet publié, avec Toulouse en contrôle négatif.

### Deux impasses, mesurées — ne pas les retenter

L'envie naturelle est de télécharger plusieurs régions et de les réunir sur
le téléphone. Les deux façons de le faire ont été essayées et échouent :

| Méthode | Résultat mesuré |
| --- | --- |
| Prolonger l'archive (`tar -A`) | 254 membres présents, **120 chargés** — la première archive seule |
| Dossier commun (`tile_dir`) | Le service démarre, **aucune arête trouvée**, même dans une seule région |

Et surtout, le format n'était pas le vrai problème : **chaque extrait coupe
ses routes au bord de sa région**, si bien qu'aucune réunion après coup ne
les recollera. La couture serait restée trouée même si la fusion avait
fonctionné mécaniquement.

La seule voie qui raccorde vraiment est de bâtir **un seul graphe à partir
de plusieurs extraits**, à la construction. Mesure : Aquitaine +
Midi-Pyrénées ensemble donnent un paquet de 572 Mo qui route Bordeaux →
Toulouse en 244,9 km, là où les deux paquets séparés — 558 Mo au total — ne
traversaient pas.

Deux détails qui coûtent cher si on les ignore :

- les extraits Geofabrik se chevauchent aux frontières ; passés bruts et en
  nombre à `valhalla_build_tiles`, les objets en double le font tomber
  (`vector::_M_range_check`). **`osmium merge` d'abord**, toujours ;
- une pièce jointe de Release plafonne à **2 Gio**, et l'envoi échoue sur un
  `422` qui n'explique rien. La construction refuse désormais d'elle-même une
  archive trop lourde, en nommant le découpage à faire.

### Ce que l'app en fait

- **Plusieurs régions installées** en même temps ; au calcul, chaque moteur
  est interrogé à son tour jusqu'à ce que l'un réponde.
- **Téléchargement en arrière-plan** (`URLSessionConfiguration.background`),
  qui survit au passage dans une autre application comme à la fermeture.
- **Interrupteur hors ligne** dans les réglages : le calcul local n'est pas
  imposé, il est choisi.

### Limite connue

Un trajet ne se calcule hors réseau que s'il tient **entier dans un même
paquet** : interroger deux paquets à la suite ne recoud pas les routes à
leur frontière, pour la raison mesurée plus haut. Bordeaux → Toulouse ne
passe donc pas avec Nouvelle-Aquitaine et Occitanie côte à côte — il passe
avec la **Zone Sud**, bâtie d'un seul graphe. C'est à cela que servent les
six zones qui se chevauchent.

## Déploiement Heroku (buildpack — 100 % navigateur, sans CLI)

L'app se déploie via un **buildpack Rust** : la crate est à la racine,
Heroku la compile (`cargo build --release`) et lance le binaire défini dans
le `Procfile`. L'addon **Heroku Postgres** fournit `DATABASE_URL` ; la table
`positions` est créée automatiquement au démarrage. Pas de Docker, pas de
GitHub Actions, pas de stack `container`.

Fichiers utilisés :

- `Cargo.toml` / `src/` — la crate à la racine (détectée par le buildpack)
- `Procfile` — `web: ./target/release/moncap-gps`
- `app.json` — déclare le buildpack Rust + l'addon Postgres

### Ce que sert le dyno

| Adresse | Contenu |
| --- | --- |
| `/`, `/es`, `/en` | la vitrine, en français, espagnol et anglais |
| `/vitrine/…` | ses fichiers, nommés d'après leur contenu |
| `/app` | le front web, écran de connexion compris (`web/dist`) |
| le reste | l'API |

Le buildpack ne compile **que** la crate Rust : aucun Bun ne tourne sur le
dyno. Les deux fronts sont donc versionnés construits (`web/dist`,
`vitrine/dist`) — sans quoi le serveur n'aurait rien à servir.

Pour la vitrine, c'est le CI qui s'en charge : `.github/workflows/vitrine.yml`
la reconstruit et verse `dist/` dès que `vitrine/src` change sur `master`. La
poussée qui en résulte déclenche à son tour le déploiement Heroku. Rien à
lancer depuis un navigateur, ce qui est le seul outil disponible ici.

Le front web vise la **même origine** par défaut : servi par le dyno, il
parle à l'API sans configuration.

### Option 1 — Le plus simple : bouton « Deploy to Heroku »

[![Deploy](https://www.herokucdn.com/deploy/button.svg)](https://heroku.com/deploy?template=https://github.com/maxlestage/moncap)

Tape le bouton (depuis ton navigateur/téléphone). Heroku lit `app.json`,
ajoute le buildpack Rust + Postgres, compile et démarre l'app. À la fin,
ouvre `https://<ton-app>.herokuapp.com/health` → doit afficher `ok`.

### Option 2 — Depuis le tableau de bord Heroku (connexion GitHub)

1. **New ▸ Create new app**, donne-lui un nom.
2. Onglet **Settings ▸ Buildpacks ▸ Add buildpack** → colle cette URL :
   `https://github.com/emk/heroku-buildpack-rust.git` → **Save changes**.
3. Onglet **Resources ▸ Add-ons** → cherche **Heroku Postgres** → ajoute le
   plan `essential-0`.
4. Onglet **Deploy ▸ Deployment method ▸ GitHub** → connecte le dépôt
   `maxlestage/moncap` → **Deploy Branch** (`master`).
5. Ouvre `https://<ton-app>.herokuapp.com/health` → doit afficher `ok`.

> ℹ️ Le buildpack Rust **doit** être ajouté à l'étape 2, sinon Heroku affiche
> « No default language could be detected for this app » (il ne sait pas
> compiler du Rust tout seul).

### Tester l'image en local (Docker, optionnel)

```bash
docker build -t moncap-gps .
docker run -p 3000:3000 \
  -e DATABASE_URL="postgres://user:pass@host:5432/moncap" \
  moncap-gps
```

## Front (Swift / SwiftUI)

App iOS qui affiche une carte (MapKit), la position de l'appareil
(CoreLocation), les positions enregistrées et l'itinéraire qui les relie
(polyligne). Fonctions : enregistrer sa position, **navigation turn-by-turn
avec annonces vocales** (depuis ta position vers la destination, carte qui
suit le cap, **recalcul automatique si tu sors de la route**), export GPX,
liste avec suppression.

### Langues

L'app suit la langue du téléphone parmi le **français, l'espagnol et
l'anglais** ; ailleurs, elle reste en français. Le français est la langue
source : chaque clé du catalogue **est** la chaîne française écrite dans le
code Swift, si bien qu'une traduction manquante affiche du français plutôt
qu'un trou.

`frontend/traductions.py` porte le tableau et engendre deux fichiers :

| Fichier | Rôle |
| --- | --- |
| `frontend/MonCapGPS/Localizable.xcstrings` | le catalogue lu par SwiftUI |
| `vitrine/src/appstrings.ts` | les mêmes libellés pour les maquettes du site |

Relancer `python3 frontend/traductions.py` après toute modification. Le
groupe de fichiers du projet Xcode est synchronisé : le catalogue déposé dans
`MonCapGPS/` est intégré sans manipulation.

`python3 frontend/traductions.py --verifie` réunit quatre contrôles, tenus par
un job d'intégration continue. Aucun des défauts qu'ils attrapent ne fait
échouer la compilation — une chaîne non traduite s'affiche simplement en
français — et tous ont été trouvés à la main une première fois.

Deux pièges rencontrés, à connaître avant d'ajouter du texte :

- `Text("littéral")` se traduit tout seul (le littéral sert de clé), mais
  **`Text(uneVariable)` non** : cette surcharge est verbatim. Les libellés
  venus d'énumérations passent donc par `Text(LocalizedStringKey(...))`.
- `String(format:)` ne consulte aucun catalogue : ses formats sont enveloppés
  dans `NSLocalizedString`.

### Comparateur d'itinéraires

**Jusqu'à 15 itinéraires** sont proposés pour un même trajet — c'est un
plafond dur, `options.prefix(15)` dans `ContentView.swift`, appliqué après
dédoublonnage et tri (le plus simple d'abord, puis par durée). Au-delà, la
liste devient plus longue à parcourir que le trajet à choisir.

Chacun porte :

| | |
| --- | --- |
| Étiquette | le plus simple (le moins de manœuvres), le plus rapide, le plus court |
| Trajet | durée, distance, nombre de virages, heure d'arrivée |
| Relief | dénivelé ↗ / ↘, avec avertissement au-delà de 150 m |
| Coût | carburant (consommation du véhicule × prix local réel) + péages estimés |

**Hors ligne, cinq** : Valhalla sait produire des variantes — le paramètre
`alternates` — et les renvoie à côté de l'itinéraire principal, chacune dans
son propre objet `trip`. Le calcul reste local : demander des variantes coûte
du processeur, pas de la donnée. Les péages viennent alors de
`summary.has_toll`, si bien que l'estimation carburant + péages fonctionne
aussi sans réseau. Ce qui manque hors ligne : le dénivelé (il vient d'une API)
et le trafic.

Le **recalcul** en cours de route est traité localement **avant** toute
tentative réseau : attendre l'échec d'une requête ferait patienter le bandeau
« Recalcul… » au pire moment, celui où l'on vient de rater une sortie. Trois
relevés GPS à plus de 55 m de l'itinéraire le déclenchent.

**Temps réel (WebSocket)** :

- 🟢 synchro instantanée des positions entre tous les appareils
- 🚗 partage de sa position GPS en direct (voitures des autres sur la carte)
- 🚨 signalements **police / accident / bouchon / danger** diffusés à tous
- pastille de connexion dans la barre de navigation

`APIClient.baseURL` pointe par défaut sur l'app Heroku
(`https://moncap-c41a5aaf07e8.herokuapp.com`) → l'app marche sans backend
local ; mets `http://localhost:3000` pour développer en local.

### Ouvrir et lancer (projet Xcode prêt à l'emploi)

Un vrai projet Xcode est fourni : **`frontend/MonCapGPS.xcodeproj`**.

1. Ouvre `frontend/MonCapGPS.xcodeproj` dans **Xcode 16+**.
2. Sélectionne ta cible (simulateur iPhone ou ton appareil). Pour un
   appareil réel : onglet **Signing & Capabilities** → choisis ton équipe
   (signature automatique). Le `Bundle Identifier` est `com.maxlestage.moncap`.
3. **Cmd + R** pour lancer.

L'app se connecte directement au backend Heroku (HTTPS + WebSocket `wss`),
l'icône est le logo MonCap, et l'autorisation de localisation est déjà
configurée (via les réglages de build). iOS 17 minimum.

> Le projet utilise un *groupe synchronisé* : tout fichier ajouté dans
> `frontend/MonCapGPS/` est inclus automatiquement.

Fichiers Swift :

- `MonCapGPSApp.swift` — point d'entrée de l'app
- `ContentView.swift` — connexion → carte + contrôles + temps réel
- `LoginView.swift` / `AuthStore.swift` — écran de connexion/inscription + session (jeton)
- `Avatars.swift` + `Assets.xcassets/van*` — avatars « combi » (sélecteur + marqueur live)
- `LocationManager.swift` — accès GPS + vitesse via CoreLocation
- `RouteService.swift` — itinéraire routier MapKit (`MKDirections`)
- `OfflineRouting.swift` — itinéraire calculé sur l'appareil (Valhalla), et
  liste des régions lue sur les Releases
- `NavigationManager.swift` — navigation turn-by-turn + voix (`AVSpeechSynthesizer`)
- `RealtimeClient.swift` — connexion WebSocket (positions live, voitures, alertes)
- `APIClient.swift` — appels HTTP au backend
- `Models.swift` — modèles partagés avec le backend
- `ShareSheet.swift` — feuille de partage pour l'export GPX
