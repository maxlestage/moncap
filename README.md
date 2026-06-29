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
| POST    | `/route`      | Distance (km) + cap (°) entre deux points         |
| POST    | `/route/multi` | Distance totale d'un itinéraire (`{points:[...]}`) + détail par segment |

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
  -d '{"points":[{"lat":48.8566,"lon":2.3522},{"lat":45.7640,"lon":4.8357},{"lat":43.2965,"lon":5.3698}]}'
# {"total_km":669.1,"legs_km":[391.5,277.6]}

curl "localhost:3000/positions/nearest?lat=47.32&lon=5.04"
# {"position":{"id":2,...,"label":"Lyon"},"distance_km":173.7}
```

## Déploiement Heroku

Le backend se déploie via le **container stack** (Docker) : `heroku.yml`
construit `backend/Dockerfile`, et l'app écoute sur `$PORT`. L'addon
Heroku Postgres fournit automatiquement `DATABASE_URL`.

```bash
# 1. Créer l'app en mode conteneur
heroku create moncap-gps --stack container

# 2. Provisionner Postgres (fournit DATABASE_URL)
heroku addons:create heroku-postgresql:essential-0 -a moncap-gps

# 3. Déployer
git push heroku claude/gps-app-rust-swift-1k3yq3:main

# 4. Vérifier
heroku open -a moncap-gps        # /health renvoie "ok"
heroku logs --tail -a moncap-gps
```

Fichiers utilisés par Heroku :

- `heroku.yml` — build Docker du service `web`
- `backend/Dockerfile` — build multi-étapes (binaire Rust optimisé)
- `app.json` — stack conteneur + addon Postgres (déploiement « Deploy to Heroku »)
- `.dockerignore` — exclut `target/`, `frontend/`, etc.

La table `positions` est créée automatiquement au démarrage ; aucune
migration manuelle n'est requise.

## Front (Swift / SwiftUI)

App iOS qui affiche une carte (MapKit), la position de l'appareil
(CoreLocation), les positions enregistrées et l'itinéraire qui les relie
(polyligne). Boutons pour enregistrer la position courante et calculer la
distance totale de l'itinéraire ; liste avec suppression par glissement.

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
