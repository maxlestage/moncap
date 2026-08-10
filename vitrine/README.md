# Vitrine

Page de présentation de MonCap GPS. Une seule page, pensée pour le
téléphone d'abord.

```sh
bun install
bun run dev       # aperçu avec rechargement à chaud
bun run verifie   # tsc --noEmit puis bun test
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
chaque construction.

Un nom fixe a un prix, et il a été payé une fois : sans consigne, un
navigateur qui a déjà vu `/styles.css` garde sa copie sans redemander.
Safari a ainsi servi une page au HTML neuf et au style périmé — les maquettes
s'affichaient en vrac. `ServeFile` n'émet **pas** d'`ETag`, seulement un
`Last-Modified`, ce qui laisse le navigateur juge de la fraîcheur. Les deux
routes de la vitrine envoient donc `Cache-Control: no-cache` : la copie reste
en cache, mais elle est revalidée avant usage et la réponse est un 304 sans
corps. Un aller-retour, pas un téléchargement.

## React sans envoyer React

La page n'a aucun état : rien n'y bouge après l'affichage. Elle est donc
rendue **à la construction** (`renderToStaticMarkup` dans `build.tsx`), et
`dist/` ne contient que du HTML et une feuille de style.

| | poids livré |
| --- | --- |
| Paquet React classique | 397 Ko de JavaScript |
| Rendu à la construction | **10,9 Ko de HTML + 6,6 Ko de CSS**, zéro JavaScript |

React et TypeScript restent la façon d'écrire la page — typage du contenu,
composition, relecture. Ils ne sont simplement pas du voyage. `src/index.tsx`
n'existe que pour le mode développement.

## Ce qui a été vérifié

Captures à 320, 390 et 1280 px, en clair et en sombre : pas de débordement
horizontal, et le texte du bandeau tombe sur la même verticale que celui du
corps.

Le partage des adresses est tenu par un test d'intégration côté Rust
(`racine_sert_la_vitrine_et_app_le_front`) : `/` rend bien la vitrine et non
le front, la feuille répond, le lien de connexion et la signature sont
présents dans le rendu, et `/app` sert l'application.

Le même test vérifie que les deux routes envoient `Cache-Control: no-cache`
et que la feuille servie contient bien les règles des maquettes. Sans en-tête,
la panne ne se voit ni dans les journaux ni dans un `curl` : seulement chez
le visiteur déjà venu.

`bun test` couvre le contenu lui-même : pas de version dans la pile, pas de
doublon dans les listes qui servent de clés de rendu, pas de section vide.

## Contenu

Tout le texte chiffré est dans `src/content.ts`, séparé de la mise en forme.
Les nombres qui y figurent sont **mesurés** : les tailles viennent des
Releases publiées, les 222,5 km de Bordeaux → Limoges d'un calcul réel sans
réseau (voir `.github/workflows/valhalla-epreuve.yml`). Une vitrine qui
arrondit ses chiffres finit par mentir.

La pile technique (`TECHNIQUES`) est listée **sans numéro de version**. Une
version affichée sur une page vieillit toute seule : elle est fausse dès la
mise à jour suivante et personne ne revient la corriger. Le nom, lui, reste
vrai ; qui veut la version la lit dans `Cargo.toml` ou `package.json`, où
elle est tenue à jour par construction. `src/content.test.ts` fait échouer la
construction si un chiffre réapparaît dans la liste — c'est une règle qu'on
réintroduit sans y penser, « React 19 » se tape tout seul.

## Les maquettes

Les trois écrans montrés (comparateur, guidage, bandeau des lieux) sont du
**HTML et du CSS**, pas des images : nets à toute résolution, ils suivent le
thème sombre, se lisent au lecteur d'écran et ne coûtent pas un octet de plus
à télécharger.

Ce sont des **dessins**, et la légende le dit. La mise en page, les étiquettes
et les lignes affichées reprennent celles de l'application — les valeurs
chiffrées, elles, sont des exemples. Faire passer un exemple pour un relevé
serait le premier mensonge d'une vitrine.

## Style

Pas de bibliothèque d'utilitaires : `src/styles.css` tient en trois cents
lignes, avec des variables CSS pour le thème, `prefers-color-scheme` pour le
mode sombre, `env(safe-area-inset-*)` pour l'encoche et des cibles tactiles
d'au moins 44 px.
