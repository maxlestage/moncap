# MonCap GPS

Application GPS minimale : un backend **Rust + Axum** et un front **Swift / SwiftUI** (iOS).

## Architecture

```
moncap/
├── backend/        API Rust + Axum
│   └── src/main.rs
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
| DELETE  | `/positions/:id` | Supprime une position (204, ou 404 si absente) |
| GET     | `/positions/nearest?lat=&lon=` | Position enregistrée la plus proche d'un point |
| GET     | `/positions.gpx` | Exporte les positions au format GPX (waypoints) |
| POST    | `/route`      | Distance (km) + cap (°) entre deux points         |
| POST    | `/route/multi` | Distance totale + durée estimée d'un itinéraire (`{points:[...], speed_kmh?}`) |

Les positions sont persistées dans **Postgres** via **SeaORM** (table
`positions`, créée automatiquement au démarrage). Le calcul de trajet
utilise la formule de Haversine pour la distance et le cap initial.

### Lancer le backend

```bash
# 1. Démarrer Postgres (via Docker)
docker compose up -d db

# 2. Lancer l'API
cd backend
cp .env.example .env            # ou exporter DATABASE_URL
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/moncap"
cargo run
# moncap-gps écoute sur http://0.0.0.0:3000
```

La connexion est configurable via la variable d'environnement
`DATABASE_URL` (valeur par défaut :
`postgres://postgres:postgres@localhost:5432/moncap`).

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
```

## Déploiement Heroku (conteneur)

L'app tourne en conteneur : le `Dockerfile` (racine) produit un binaire
Rust optimisé qui écoute sur `$PORT` ; l'addon Heroku Postgres fournit
`DATABASE_URL` ; le schéma est créé automatiquement au démarrage.

Fichiers de déploiement :

- `Dockerfile` — build multi-étapes complet et autonome (contexte = racine)
- `Procfile` — `web: moncap-gps`
- `heroku.yml` — build Docker + process `web`
- `app.json` — stack conteneur + addon Postgres
- `.dockerignore` — exclut `target/`, `frontend/`, `.git/`, `.env`

### Sans ordinateur (recommandé) — déploiement automatique via GitHub

1. Sur **Heroku** (navigateur) : crée une app, puis ajoute l'addon
   **Heroku Postgres** (`essential-0`). Récupère ta **clé API** (Account
   settings ▸ API Key).
2. Sur **GitHub** (navigateur), dans *Settings ▸ Secrets and variables ▸
   Actions*, ajoute trois secrets :
   - `HEROKU_API_KEY` — ta clé API Heroku
   - `HEROKU_APP_NAME` — le nom de l'app (ex. `moncap-gps`)
   - `HEROKU_EMAIL` — l'e-mail de ton compte
3. Chaque push sur `master` déclenche le workflow **Deploy**
   (`.github/workflows/deploy.yml`) qui construit le `Dockerfile` et le
   pousse sur Heroku. (Sans ces secrets, le job ne fait rien et reste vert.)

Vérification : ouvre `https://<app>.herokuapp.com/health` → doit afficher `ok`.

### Avec la CLI Heroku (si tu as un ordinateur)

```bash
heroku create moncap-gps --stack container
heroku addons:create heroku-postgresql:essential-0 -a moncap-gps
git push heroku HEAD:main
heroku open -a moncap-gps
```

### Tester l'image en local (Docker)

```bash
docker build -t moncap-gps .
docker run -p 3000:3000 \
  -e DATABASE_URL="postgres://user:pass@host:5432/moncap" \
  moncap-gps
```

La table `positions` est créée automatiquement au démarrage ; aucune
migration manuelle n'est requise.

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
