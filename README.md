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
| POST    | `/route`      | Distance (km) + cap (°) entre deux points         |

Les positions sont stockées en mémoire. Le calcul de trajet utilise la
formule de Haversine pour la distance et le cap initial.

### Lancer le backend

```bash
cd backend
cargo run
# moncap-gps écoute sur http://0.0.0.0:3000
```

### Exemples

```bash
curl localhost:3000/health

curl -X POST localhost:3000/positions \
  -H 'content-type: application/json' \
  -d '{"lat":48.8566,"lon":2.3522,"label":"Paris"}'

curl localhost:3000/positions

curl -X POST localhost:3000/route \
  -H 'content-type: application/json' \
  -d '{"from":{"lat":48.8566,"lon":2.3522},"to":{"lat":45.7640,"lon":4.8357}}'
# {"distance_km":391.5,"bearing_deg":150.5}
```

## Front (Swift / SwiftUI)

App iOS qui affiche une carte (MapKit), la position de l'appareil
(CoreLocation) et les positions enregistrées. Boutons pour enregistrer la
position courante et calculer un trajet vers la première position.

### Mise en place dans Xcode

1. Xcode → **File ▸ New ▸ Project ▸ iOS App** (SwiftUI), nom `MonCapGPS`.
2. Remplacer les fichiers générés par ceux de `frontend/MonCapGPS/`.
3. Dans les réglages de la cible, ajouter la clé
   `NSLocationWhenInUseUsageDescription` (déjà présente dans `Info.plist`).
4. Lancer le backend, puis l'app dans le simulateur : `localhost:3000`
   pointe vers votre Mac.

Fichiers Swift :

- `MonCapGPSApp.swift` — point d'entrée de l'app
- `ContentView.swift` — carte + contrôles
- `LocationManager.swift` — accès GPS via CoreLocation
- `APIClient.swift` — appels HTTP au backend
- `Models.swift` — modèles partagés avec le backend
