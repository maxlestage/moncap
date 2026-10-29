# MonCap GPS

Application GPS minimale : un serveur **Rust** sur [**Paquets**](https://github.com/maxlestage/Paquets),
une app **Swift / SwiftUI** (iOS), et deux fronts web en **Rust** — **active** + WebAssembly,
le front de la famille Paquets : la vitrine et l'application web.

## Architecture

```
moncap/
├── Cargo.toml      Le serveur, sur Paquets (crate à la racine, pour le buildpack Heroku)
├── src/
│   ├── main.rs         Point d'entrée
│   ├── app.rs          L'application : bouclier de sécurité, CORS, routes, tâches de fond
│   ├── connexion.rs    Comptes, sessions, mots de passe (voir « Connexion »)
│   ├── base.rs         La base SQLite : tables (modèles memoire-orm), migrations
│   ├── cache.rs        Le cache SQLite : tuiles, feux, air, régions hors ligne
│   ├── erreur.rs       Les erreurs, en texte brut comme l'app iOS les attend
│   ├── temps_reel.rs   Le WebSocket
│   ├── alertes.rs, positions.rs, trajets.rs, compte.rs   L'API REST
│   ├── externes/       Feux, air, transports, trafic, paquets hors ligne
│   ├── tuiles.rs, pmtiles.rs   Le relais de tuiles et le lecteur PMTiles
│   ├── partage.rs      Partage live, itinéraires partagés, confidentialité
│   └── statique.rs     Les deux fronts
├── Procfile        web: ./target/release/moncap-gps
├── app.json        buildpack Rust (bouton « Deploy ») — aucun addon
├── web/            Front web, en Rust : active + WebAssembly (voir web/README.md)
├── vitrine/        Page de présentation, en Rust : active + WebAssembly (vitrine/README.md)
├── mouvement/      Les animations des deux fronts (rideau, cascades, molette inertielle…)
├── fabrique/       Les outils de construction des deux fronts (empreintes, wasm-bindgen, wasm-opt)
└── frontend/       App iOS SwiftUI
    └── MonCapGPS/
```

## Serveur (Rust + Paquets)

Paquets réunit le serveur HTTP (**vitesse**), le bouclier de sécurité
(**securite** : pare-feu applicatif, limitation de débit, en-têtes, blocage
des robots d'attaque), l'ORM (**memoire-orm**, SQLite) et le front
(**active**). Les routes sont volontairement aussi simples que possible :

| Méthode | Route         | Rôle                                              |
|---------|---------------|---------------------------------------------------|
| GET     | `/health`     | Vérifie que le serveur et la base répondent (`ok`) |
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

### Connexion

| Méthode | Route | Rôle |
|---------|-------|------|
| POST | `/auth/signup` | Crée un compte `{username, password}` → `{token, username, expires_at}` |
| POST | `/auth/login` | Ouvre une session → `{token, username, expires_at}` |
| POST | `/auth/logout` | Ferme la session courante (204) |
| POST | `/auth/logout-all` | Ferme toutes les sessions du compte (204) |
| GET | `/auth/me` | La session est-elle valable ? → `{username, created_at}` |
| GET | `/auth/sessions` | Les appareils connectés `[{id, device, created_at, last_seen_at, current}]` |
| DELETE | `/auth/sessions/:id` | Déconnecte un appareil (204) |
| POST | `/auth/password` | Change le mot de passe `{current_password, new_password}` → nouvelle session |
| POST | `/auth/forgot` | Envoie un lien de réinitialisation par courriel (204 ; 503 sans SMTP) |
| GET | `/reinitialisation?jeton=` | La page du lien reçu ; son formulaire poste sur `/auth/reset` |

Ce qui la rend fiable :

- **Des sessions côté serveur.** Le jeton remis est opaque (`mc_` + 256 bits
  aléatoires) ; le serveur n'en garde que l'empreinte SHA-256. Une session
  vit **180 jours après son dernier usage** : on n'est plus déconnecté au
  bout d'un mois, ni quand un secret change.
- **Une vraie déconnexion.** Se déconnecter supprime la session. Changer ou
  réinitialiser son mot de passe, « déconnecter partout » ou supprimer son
  compte coupe **toutes** les sessions — un jeton volé ne survit à rien de
  tout cela. Le WebSocket revérifie sa session toutes les minutes.
- **Un identifiant insensible à la casse et aux espaces** : la majuscule
  que le clavier ajoute en début de champ ne fait plus échouer la connexion,
  et deux comptes ne peuvent plus différer par la seule casse.
- **Des refus qui disent pourquoi**, en texte (l'app iOS affiche le corps
  tel quel) : `identifiant ou mot de passe incorrect`, champs trop courts ou
  trop longs, `trop de tentatives…, réessaie dans N min` avec `Retry-After`.
- **Un frein aux devinettes par compte** (10 échecs en 15 min → 15 min
  d'attente), stocké en base (il survit aux redémarrages), il ne bloque plus tout
  un réseau mobile derrière une même adresse IP.
- **Mot de passe oublié** : réponse identique et immédiate que le compte
  existe ou non (le courriel part en arrière-plan), trois liens par heure
  au plus, un nouveau lien annule les précédents, et un lien ne sert qu'une
  fois même sous deux envois simultanés.
- **Argon2id hors de la boucle d'événements** : un hachage ne gèle plus les
  autres requêtes.
- **Plus de jetons JWT** : ceux de l'ancien serveur désignaient des comptes
  d'une base qui n'existe plus, et la nouvelle numérote à partir de 1 — un
  vieux jeton aurait ouvert le compte de quelqu'un d'autre. Un jeton inconnu
  donne 401, et l'app rouvre une session d'elle-même.

Toutes les routes `positions*`, `stats`, `trips`, `searches`, `alerts`,
`account`, `prefs` exigent `Authorization: Bearer <jeton>` et sont **isolées
par utilisateur** ; le WebSocket et l'export GPX acceptent le jeton en query
(`?token=`). Les erreurs sont du **texte brut** ; un jeton absent, révoqué
ou expiré donne `401 authentification requise`.

### Temps réel (WebSocket)

`/ws` diffuse à tous les clients connectés :

- `hello` — l'identifiant de la connexion (pour écarter son propre écho).
- `positions_changed` — émis à chaque ajout/modif/suppression/import → les
  clients rechargent leur liste automatiquement.
- `live` / `live_gone` — positions GPS en direct des utilisateurs qui
  partagent leur localisation (marqueurs mobiles), expirées côté client.
- `alert` / `alerts` / `alert_gone` — signalements (police, accident,
  bouchon, danger), diffusés à tous et renvoyés en instantané à la connexion
  (gardés ~30 min).

Le client envoie `{"kind":"live", lat, lon, label}` ou
`{"kind":"alert", category, lat, lon, label}`. Le serveur tourne en une
seule instance (sa base est un fichier) : plus de Redis ni de pont entre
instances, le canal de diffusion est partagé par tous ses fils.

### Données et cache : SQLite, sans serveur de base

Plus de Postgres ni de Redis : tout tient dans deux fichiers SQLite, ouverts
par Paquets dans le processus même — pas de connexion réseau à payer à
chaque requête, rien à installer ni à facturer.

| Fichier | Variable | Contenu |
| --- | --- | --- |
| `data/moncap.db` | `DATABASE_URL` (`sqlite://…`) | comptes, sessions, positions, trajets, destinations, signalements |
| `data/cache.db` | `CACHE_URL` | tuiles, feux, qualité de l'air, liste des régions — jetable |

- **Le mode WAL** laisse lire pendant qu'on écrit ; le schéma
  (`src/base.rs`) se crée au premier démarrage et les migrations tiennent
  leur registre dans `_memoire_migrations`. Les données d'un compte partent
  avec lui (`ON DELETE CASCADE`).
- **Le cache est un fichier à part** : ses écritures (une tuile par requête)
  ne prennent pas le verrou de la base des comptes, et on peut l'effacer à
  tout moment. Il garde 30 jours au plus et `CACHE_MAX_MO` (512 par défaut) ;
  le ménage passe chaque heure, suivi de `PRAGMA optimize` sur les deux bases.
- **Toutes les destinations sont gardées** (`/searches`), une ligne par lieu :
  y retourner la remonte en tête et compte la visite (`fois`). Plus de
  plafond à douze. `GET /searches?limit=N` n'en rend que les N premières.
- **Les tâches de fond** (purge des signalements, ménage) tournent sur un fil
  que Paquets garde pour elles, à côté des fils du serveur.

> ⚠️ **Le disque d'un dyno Heroku est éphémère** : il repart de zéro à chaque
> déploiement et au redémarrage quotidien. Sur Heroku, la base SQLite oublie
> donc comptes, trajets et destinations au moins une fois par jour. L'app
> s'en remet seule (le compte de l'appareil se recrée, et **les destinations
> restent sur le téléphone**, dans sa propre base SQLite), mais pour garder
> les données du serveur, il faut un disque qui dure : un volume (Fly.io,
> Railway, Render, un VPS, Docker) monté sur `data/`, ou `DATABASE_URL`
> pointé dessus, par exemple `sqlite:///data/moncap.db`.

### Sécurité

Le bouclier de Paquets passe devant chaque requête : identifiant de requête,
hygiène (taille d'URL, d'en-têtes…), limite de corps, **limitation de débit**
par IP (300 req/min, 30 req/min sur `/auth`, 6000 req/min sur `/tiles`, IP
lue sur la dernière entrée de `X-Forwarded-For` derrière Heroku — celle que
le client ne peut pas falsifier), blocage des scanners, **pare-feu
applicatif** (injection SQL, XSS, RCE…), en-têtes de sécurité (HSTS,
`nosniff`, `X-Frame-Options`). Ses refus sont, eux aussi, du texte brut.

### Lancer le backend

```bash
# Rien à installer : la base est un fichier, créé au premier démarrage.
cargo run
# moncap-gps écoute sur http://0.0.0.0:3000, données dans data/moncap.db

# Les tests : chaque essai de bout en bout a sa propre base SQLite en mémoire.
cargo test
```

Le niveau de logs (structurés via `tracing`) se règle avec `RUST_LOG`
(défaut : `info`). Les calculs de distances (`/stats`,
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
relayer des requêtes arbitraires), garde les tuiles servies dans le cache
SQLite (celles du relais comme celles de l'archive PMTiles) et
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

Tout cela vit dans un dépôt à part, **public** et dédié :
[`maxlestage/MonCapMaps`](https://github.com/maxlestage/MonCapMaps) — les
Releases (une par région) et les workflows qui les construisent. Ce dépôt-ci
peut ainsi rester privé : l'app lit la liste et télécharge les fichiers sur
MonCapMaps, sans jeton et sans passer par le serveur.

- **Workflow** `valhalla-tiles.yml` (sur MonCapMaps), manuel. L'entrée
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
- **Vérification** `valhalla-epreuve.yml` (sur MonCapMaps), chaque lundi :
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

### Le serveur, en repli

L'app lit d'abord MonCapMaps directement ; le serveur ne sert plus que de
repli pour la liste, quand GitHub refuse (soixante appels par heure sans
jeton). Historique : quand les paquets vivaient sur ce dépôt **privé**,
l'application les demandait directement à `api.github.com`, sans jeton :
GitHub répondait 404, la liste des régions restait vide, aucune région
n'était proposée — et l'écran affichait « il faut du réseau une fois » alors
que le réseau allait très bien. Le message accusait le réseau parce que
`loadRegions()` ne distinguait pas un refus d'une panne.

Un jeton ne peut pas voyager dans l'application : n'importe qui l'extrait du
binaire, et il ouvrirait le dépôt entier. C'est donc **le backend** qui le
détient, et qui sert d'intermédiaire :

| Adresse | Ce qu'elle fait |
| --- | --- |
| `GET /offline/regions` | la liste des régions publiées, en cache une heure |
| `GET /offline/regions/:tag/archive` | redirige vers l'archive de tuiles |
| `GET /offline/regions/:tag/config` | redirige vers la configuration |

Les deux dernières **redirigent** plutôt qu'elles ne relaient : GitHub répond
à une demande authentifiée par une adresse signée, valable quelques minutes
et qui ne demande aucune authentification. Le téléphone la suit tout seul, et
le dyno ne voit pas passer un octet du transfert — deux gigaoctets par région
n'ont rien à faire là.

Ce qu'il faut régler, une fois, dans **Heroku → Settings → Config Vars** :

| Variable | Valeur |
| --- | --- |
| `GITHUB_TOKEN` | un jeton avec la portée `repo` en lecture sur ce dépôt |
| `MONCAP_DEPOT` | facultatif, `maxlestage/MonCapMaps` par défaut |

Sans jeton, les deux routes répondent **503 avec le motif écrit en toutes
lettres** (`GITHUB_TOKEN absent`), que l'application affiche. C'est délibéré :
la panne d'origine a coûté une soirée de recherche du mauvais côté.

Le préfixe d'étiquette `tuiles-eu-` n'est pas décoratif : il **borne** ce que
l'intermédiaire accepte de servir. Sans lui, la route deviendrait une porte
ouverte sur toutes les pièces jointes d'un dépôt privé. Un test le vérifie,
`..` et `/` compris.

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
le `Procfile`. Aucun addon : la base et le cache sont des fichiers SQLite
(voir « Données et cache » — et l'avertissement sur le disque éphémère).
Une ancienne variable `DATABASE_URL` Postgres est ignorée, avec un
avertissement dans les journaux. Pas de Docker, pas de GitHub Actions, pas
de stack `container`.

Fichiers utilisés :

- `Cargo.toml` / `src/` — la crate à la racine (détectée par le buildpack)
- `Procfile` — `web: ./target/release/moncap-gps`
- `app.json` — déclare le buildpack Rust

### Ce que sert le dyno

| Adresse | Contenu |
| --- | --- |
| `/`, `/es`, `/en` | la vitrine, en français, espagnol et anglais |
| `/vitrine/…` | ses fichiers, nommés d'après leur contenu |
| `/app` | le front web, écran de connexion compris (`web/dist/index.html`) |
| `/web/…` | ses fichiers, nommés d'après leur contenu |
| `/auth/…`, `/positions`… | l'API |
| toute autre adresse en GET | le front web (il route lui-même) |

Le buildpack ne compile **que** la crate du serveur : aucun WebAssembly
n'est fabriqué sur le dyno. Les deux fronts sont donc versionnés construits
(`web/dist`, `vitrine/dist`) — sans quoi le serveur n'aurait rien à servir.

C'est le CI qui s'en charge : `.github/workflows/front.yml` vérifie et
reconstruit les deux fronts, et verse leurs `dist/` dès que leurs sources —
ou celles de `mouvement/` et `fabrique/`, qu'ils partagent — changent sur
`master`. La poussée qui en résulte déclenche à son tour le déploiement
Heroku. Rien à lancer depuis un navigateur, ce qui est le seul outil
disponible ici.

### Les fronts, et leur mouvement

Les deux fronts sont écrits en **Rust**, avec **active** — le front de la
famille Paquets, qui a remplacé Yew — et compilés en WebAssembly. La carte
du front web est écrite en Rust (voir `web/README.md`).

Leurs animations suivent [zamocorp.com](https://zamocorp.com) et sont
jouées par la crate `mouvement`, la même pour les deux. Ce que le site de
référence obtient avec GSAP, ScrollTrigger et Lenis y est repris en Rust,
et seulement ces gestes-là :

| Geste | Où |
| --- | --- |
| Rideau de chargement qui se lève | les deux, pendant l'arrivée du WebAssembly |
| Titres qui montent mot à mot derrière leur masque | les deux |
| Cascades décalées à l'entrée dans l'écran | les deux |
| Dévoilement par `clip-path` | maquettes, carte, carte de connexion |
| Nombres qui comptent jusqu'à leur valeur | chiffres, prix, mesure, tableau de bord |
| Molette inertielle (à la Lenis), progression liée au défilement | les deux ; la carte garde sa molette pour zoomer |
| Boutons magnétiques | les deux, à la souris seulement |
| Volet plein écran entre la connexion et le tableau | front web |
| Route qui se dessine, point GPS qui la parcourt, grain | écran de connexion |
| Bandeau défilant incliné par l'élan | les deux ; un second, en sens inverse, sur la vitrine |
| Grands titres lettre à lettre | les deux |
| Curseur qui suit, avec étiquette sur certains liens | les deux, à la souris seulement |
| Cartes qui s'inclinent vers la souris, lueur qui suit le pointeur | vignettes, cartes, tarifs, panneaux, connexion |
| Ondes au point touché | boutons du front web |
| Volet de sortie avant un lien interne | les deux |
| Décor qui suit la souris (parallaxe) | carte vivante de la vitrine, épingles de la connexion, signature du pied |
| Libellés qui roulent au survol, barre de lecture | les deux |
| Carte qui arrive en vol | front web |

Le moteur ne démarre pas quand le système demande moins de mouvement, et
tout état caché vit sous la classe qu'il pose : sans lui, la page est
immobile, jamais vide. Ses règles — amortissement indépendant de la
fréquence d'écran, cascades plafonnées, compteurs qui finissent sur le
texte exact — sont des fonctions pures, testées sans navigateur.

Le front web vise la **même origine** par défaut : servi par le dyno, il
parle à l'API sans configuration.

### Option 1 — Le plus simple : bouton « Deploy to Heroku »

[![Deploy](https://www.herokucdn.com/deploy/button.svg)](https://heroku.com/deploy?template=https://github.com/maxlestage/moncap)

Tape le bouton (depuis ton navigateur/téléphone). Heroku lit `app.json`,
ajoute le buildpack Rust, compile et démarre l'app. À la fin,
ouvre `https://<ton-app>.herokuapp.com/health` → doit afficher `ok`.

### Option 2 — Depuis le tableau de bord Heroku (connexion GitHub)

1. **New ▸ Create new app**, donne-lui un nom.
2. Onglet **Settings ▸ Buildpacks ▸ Add buildpack** → colle cette URL :
   `https://github.com/emk/heroku-buildpack-rust.git` → **Save changes**.
3. Onglet **Deploy ▸ Deployment method ▸ GitHub** → connecte le dépôt
   `maxlestage/moncap` → **Deploy Branch** (`master`).
4. Ouvre `https://<ton-app>.herokuapp.com/health` → doit afficher `ok`.

> ℹ️ Le buildpack Rust **doit** être ajouté à l'étape 2, sinon Heroku affiche
> « No default language could be detected for this app » (il ne sait pas
> compiler du Rust tout seul).

### Tester l'image en local (Docker, optionnel)

```bash
docker compose up --build
# ou : docker build -t moncap-gps . && docker run -p 3000:3000 -v moncap:/app/data moncap-gps
# le volume garde data/moncap.db (et le cache) d'un redémarrage à l'autre
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
| `vitrine/src/appstrings.rs` | les mêmes libellés pour les maquettes du site |

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

### `Package.resolved` est versionné, et il le faut

Xcode Cloud construit avec la **résolution automatique des dépendances
désactivée**. Sans fichier de résolution versionné, l'archive échoue avant de
compiler la moindre ligne :

```
Could not resolve package dependencies: a resolved file is required when
automatic dependency resolution is disabled and should be placed at
…/project.xcworkspace/xcshareddata/swiftpm/Package.resolved
```

Le fichier épingle **sept** paquets, alors que le projet n'en déclare que deux.
Les cinq autres sont des dépendances de `valhalla-mobile`, et elles doivent y
figurer aussi : un graphe partiel échoue exactement comme un graphe absent.

| Paquet | Version | Déclaré par |
| --- | --- | --- |
| `maplibre-gl-native-distribution` | 7.0.0 | le projet |
| `valhalla-mobile` | 0.6.4 | le projet |
| `valhalla-openapi-models-swift` | 0.6.0 | valhalla-mobile |
| `light-swift-untar` | 1.0.4 | valhalla-mobile |
| `swift-docc-plugin` | 1.5.0 | valhalla-mobile |
| `swift-docc-symbolkit` | 1.0.0 | swift-docc-plugin |
| `anycodable` | 0.6.7 | valhalla-openapi-models-swift |

Deux détails qui font échouer la résolution s'ils sont approximés. L'`identity`
est le dernier segment de l'URL en minuscules, sans `.git`. Le `location` doit
reprendre l'URL **telle que l'écrit le manifeste qui déclare la dépendance** —
`swift-docc-symbolkit` vient de `swiftlang`, pas d'`apple`, bien que le paquet
qui l'appelle soit d'Apple.

Après toute mise à jour d'un paquet dans Xcode, ce fichier change : il doit
être commité avec le reste, sans quoi la construction suivante repart sur les
anciennes versions ou échoue.

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
