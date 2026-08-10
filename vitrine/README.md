# Vitrine

Page de présentation de MonCap GPS. Une seule page, pensée pour le
téléphone d'abord.

```sh
bun install
bun run dev       # aperçu avec rechargement à chaud
bun run verifie   # tsc --noEmit
bun run build     # écrit dist/
```

## Où elle est servie

Le backend Rust la sert à la **racine** (`/`), et le front web — écran de
connexion compris — vit derrière `/app` ; le bouton « Se connecter » y mène.

`dist/` est **versionné**, contrairement à l'usage. Le buildpack Heroku ne
compile que la crate Rust : aucun Bun ne tourne sur le dyno, donc le résultat
de la construction doit être dans le dépôt.

Il n'y a rien à lancer à la main pour autant : `.github/workflows/vitrine.yml`
reconstruit et verse `dist/` à chaque changement de `src/`, poussé sur
`master`. Ce dépôt se tient depuis un navigateur — un « n'oublie pas de
reconstruire » y serait un piège, pas une consigne.

Pour la même raison, la feuille de style porte un nom **fixe** (`styles.css`)
et non une empreinte : une route Rust ne peut pas suivre un nom qui change à
chaque construction. La fraîcheur repose sur l'ETag calculé par `ServeFile`.

## React sans envoyer React

La page n'a aucun état : rien n'y bouge après l'affichage. Elle est donc
rendue **à la construction** (`renderToStaticMarkup` dans `build.tsx`), et
`dist/` ne contient que du HTML et une feuille de style.

| | poids livré |
| --- | --- |
| Paquet React classique | 397 Ko de JavaScript |
| Rendu à la construction | **4,6 Ko de HTML + 3,3 Ko de CSS**, zéro JavaScript |

React et TypeScript restent la façon d'écrire la page — typage du contenu,
composition, relecture. Ils ne sont simplement pas du voyage. `src/index.tsx`
n'existe que pour le mode développement.

## Ce qui a été vérifié

Captures à 320, 390 et 1280 px, en clair et en sombre : pas de débordement
horizontal, et le texte du bandeau tombe sur la même verticale que celui du
corps.

Le partage des adresses est tenu par un test d'intégration côté Rust
(`racine_sert_la_vitrine_et_app_le_front`) : `/` rend bien la vitrine et non
le front, la feuille répond, le lien de connexion est présent dans le rendu,
et `/app` sert l'application.

## Contenu

Tout le texte chiffré est dans `src/content.ts`, séparé de la mise en forme.
Les nombres qui y figurent sont **mesurés** : les tailles viennent des
Releases publiées, les 222,5 km de Bordeaux → Limoges d'un calcul réel sans
réseau (voir `.github/workflows/valhalla-epreuve.yml`). Une vitrine qui
arrondit ses chiffres finit par mentir.

## Style

Pas de bibliothèque d'utilitaires : `src/styles.css` tient en trois cents
lignes, avec des variables CSS pour le thème, `prefers-color-scheme` pour le
mode sombre, `env(safe-area-inset-*)` pour l'encoche et des cibles tactiles
d'au moins 44 px.
