# MonCap GPS — front web (React + Bun)

Interface web qui consomme la même API que l'app iOS. **React 19** bundlé
par **Bun** (serveur fullstack natif, sans Vite/webpack), carte **Leaflet**
(OpenStreetMap, sans clé d'API).

## Lancer en développement

```bash
cd web
bun install
bun run dev        # serveur + HMR (http://localhost:3000 par défaut)
```

> Le backend tourne aussi sur le port 3000. Si tu lances les deux en local,
> démarre le backend sur 3000 et le front sur un autre port :
> `bun --hot ./index.html --port 5173`.

Dans l'interface, renseigne l'**URL de l'API** en haut (ex.
`https://ton-app.herokuapp.com`, ou `http://localhost:3000`) puis
« Connecter ». Elle est mémorisée dans le navigateur.

## Construire pour la production

```bash
bun run build      # génère dist/ (HTML + JS + CSS minifiés)
```

`dist/` est un site statique : déploie-le sur n'importe quel hébergement
statique (Netlify, Vercel, GitHub Pages, Cloudflare Pages…), ou sers-le
localement :

```bash
bun static-server.ts 8080 dist
```

## Fonctionnalités

- Carte avec marqueurs et tracé de l'itinéraire (clic sur la carte = ajout)
- Liste des positions : renommer / supprimer
- Calcul de l'itinéraire complet (distance + durée selon la vitesse)
- Statistiques (nombre, longueur, centre)
- Import / export **GPX**
- **Temps réel (WebSocket)** : synchro des positions entre tous les écrans,
  partage de sa position en direct, signalements communautaires (police,
  accident, bouchon, danger) — pastille verte = connecté.
- **Avatars** : choisis ton combi (vert/orange/bleu/menthe) ; il représente
  ta voiture en direct sur la carte des autres.

## Fichiers

- `index.html` — point d'entrée (charge Leaflet + le bundle)
- `src/index.tsx` — montage React
- `src/App.tsx` — UI principale
- `src/MapView.tsx` — carte Leaflet
- `src/api.ts` — client HTTP (URL d'API configurable, persistée)
- `src/types.ts` — types partagés avec le backend
