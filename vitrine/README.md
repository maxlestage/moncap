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

## Les adresses et le cache

| Adresse | Contenu | `Cache-Control` |
| --- | --- | --- |
| `/` | la page | `no-cache` — revalidée à chaque visite |
| `/vitrine/styles-<empreinte>.css` | la feuille | `public, max-age=31536000, immutable` |

Cette répartition vient d'une panne, et mérite d'être comprise avant d'être
touchée.

La feuille a d'abord porté un nom **fixe** (`styles.css`), parce qu'une route
Rust ne peut pas suivre un nom qui change à chaque construction. Elle était
servie sans `Cache-Control` — et `ServeFile` n'émet **pas** d'`ETag`, seulement
un `Last-Modified`. Le navigateur restait donc juge de la fraîcheur : Safari a
gardé sa copie et affiché une page au HTML neuf sur un style périmé, maquettes
en vrac.

Poser `no-cache` sur la feuille ne suffisait pas, et c'est le point à retenir :
**un en-tête ne s'applique qu'à une réponse que le navigateur demande**. Tant
qu'il croit sa copie fraîche, il ne redemande rien et ne voit aucun en-tête
nouveau. Un cache déjà pris ne se débloque que par un **changement
d'adresse**.

D'où l'empreinte dans le nom et le dossier monté sous `/vitrine` : chaque
construction produit une adresse neuve, qu'aucun navigateur n'a en cache. À
cette adresse l'octet ne changera jamais, elle peut donc être gardée un an
sans revalidation. Seule la page garde une adresse fixe — c'est le seul
document qui ne peut pas en changer — et se revalide à chaque visite, sans
quoi le navigateur ne verrait jamais la nouvelle empreinte.

## React sans envoyer React

La page n'a aucun état : rien n'y bouge après l'affichage. Elle est donc
rendue **à la construction** (`renderToStaticMarkup` dans `build.tsx`), et
`dist/` ne contient que du HTML et une feuille de style.

| | poids livré |
| --- | --- |
| Paquet React classique | 397 Ko de JavaScript |
| Rendu à la construction | **12,7 Ko de HTML + 7,2 Ko de CSS**, zéro JavaScript |

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

Le même test **extrait le lien de la feuille du rendu puis le suit** — il ne
devine aucun nom. C'est la seule façon de prouver que l'adresse expédiée au
navigateur mène quelque part : un nom écrit en dur resterait vert alors que
la page ne se stylerait plus. Il vérifie ensuite les deux consignes de cache
et la présence des règles des maquettes dans la feuille servie.

Cette panne-là ne se voyait ni dans les journaux, ni dans un `curl`, ni dans
une capture prise sur un navigateur neuf : uniquement chez un visiteur déjà
venu. Les captures de contrôle passent donc par un petit serveur local qui
reproduit le routage (`/` et `/vitrine/…`), et vérifient qu'un fond est bien
appliqué aux maquettes — une page sans style se photographie aussi bien
qu'une autre.

`bun test` couvre le contenu lui-même : pas de version dans la pile, pas de
doublon dans les listes qui servent de clés de rendu, pas de section vide.

Il défend aussi la **matière** de la page. En ajoutant les fonctions en ligne,
la réécriture avait emporté au passage le chiffre des pays d'Europe et la
carte du téléchargement région par région — une page reste valide en disant
moins, et rien ne proteste. Des assertions vérifient donc que le hors ligne
garde ses trois piliers (calcul local, région par région, arrière-plan) et
que le bandeau garde l'étendue européenne. Elles portent sur ce que la page
doit continuer d'expliquer, pas sur une formulation : reformuler est libre,
supprimer ne l'est pas.

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
