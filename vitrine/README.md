# Vitrine

Page de présentation de MonCap GPS. Une seule page, pensée pour le
téléphone d'abord.

```sh
bun install
bun run dev       # aperçu avec rechargement à chaud
bun run verifie   # tsc --noEmit
bun run build     # écrit dist/
```

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
