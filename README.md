# MonCap GPS

Application GPS minimale : un backend **Rust + Axum** et un front **Swift / SwiftUI** (iOS).

## Architecture

```
moncap/
├── Cargo.toml      API Rust + Axum (crate à la racine, pour le buildpack Heroku)
├── src/main.rs
├── Procfile        web: ./target/release/moncap-gps
├── app.json        buildpack Rust + addon Postgres (bouton « Deploy »)
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

Les positions sont persistées dans **Postgres** via **SeaORM** (table
`positions`, créée automatiquement au démarrage). Le calcul de trajet
utilise la formule de Haversine pour la distance et le cap initial.

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
(polyligne). Boutons pour enregistrer la position courante, calculer la
distance et la durée de l'itinéraire, exporter les positions en GPX
(feuille de partage) ; liste avec suppression par glissement.

### Mise en place dans Xcode

1. Xcode → **File ▸ New ▸ Project ▸ iOS App** (SwiftUI), nom `MonCapGPS`.
2. Remplacer les fichiers générés par ceux de `frontend/MonCapGPS/`.
3. Dans les réglages de la cible, ajouter la clé
   `NSLocationWhenInUseUsageDescription` (déjà présente dans `Info.plist`).
4. Lancer le backend, puis l'app dans le simulateur : `localhost:3000`
   pointe vers votre Mac.
5. En production, remplacer `baseURL` dans `APIClient.swift` par l'URL
   Heroku (ex. `https://moncap-gps.herokuapp.com`).

Fichiers Swift :

- `MonCapGPSApp.swift` — point d'entrée de l'app
- `ContentView.swift` — carte + contrôles
- `LocationManager.swift` — accès GPS via CoreLocation
- `APIClient.swift` — appels HTTP au backend
- `Models.swift` — modèles partagés avec le backend
- `ShareSheet.swift` — feuille de partage pour l'export GPX
