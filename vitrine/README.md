# Vitrine

Page de présentation de MonCap GPS, en **français, espagnol et anglais**,
pensée pour le téléphone d'abord.

```sh
bun install
bun run dev       # aperçu avec rechargement à chaud
bun run verifie   # tsc --noEmit puis bun test
bun run build     # écrit dist/
```

## Où elle est servie

| Adresse | Contenu |
| --- | --- |
| `/` | la page en français |
| `/es` | en espagnol |
| `/en` | en anglais |
| `/vitrine/…` | feuille de style, logo, icône |

Le front web — écran de connexion compris — vit derrière `/app` ; le bouton
« Se connecter » y mène.

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
| Rendu à la construction | **13,6 Ko de HTML + 9,1 Ko de CSS + 14,3 Ko de logo**, zéro JavaScript |

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

## Les trois langues

`src/langues.ts` porte les trois versions, dans un seul fichier plutôt que
trois jumeaux qui divergeraient en silence. Le type `Contenu` est la
garantie : oublier une phrase dans une langue ne produit pas une page à
trou, mais **une erreur de compilation**. Un test complète ce que le type
ne peut pas voir — une chaîne vide compile très bien.

Les libellés visibles **dans les maquettes** ne sont pas retapés : ils
viennent de `src/appstrings.ts`, engendré par `frontend/traductions.py` à
partir du tableau qui alimente aussi le catalogue iOS. Un écran dessiné ici
dit donc exactement ce que l'application affiche, dans la même langue, et ne
peut pas s'en écarter sans que la génération le montre.

Les lignes chiffrées sont composées en versant des valeurs d'exemple dans les
**formats de l'application** (`src/traduire.ts`), plutôt qu'en recopiant des
phrases. La maquette suit ainsi la langue et la façon dont l'app écrit ses
nombres : `String(format:)` sans locale met un point décimal, l'app affiche
donc « 24.3 km » y compris en français — ce que la maquette écrivait à tort
« 24,3 » avant d'être engendrée.

Les trois pages se lient mutuellement et se déclarent en `hreflang` : sans
cela, une page est un cul-de-sac pour qui n'atterrit pas dans sa langue.

## Les thèmes

Quatre choix — Auto, Clair, Sombre, Marine — **sans une ligne de script** :
des boutons radio en haut de page, et `:has()` pour les lire dans la feuille
de style. « Auto » ne redéfinit rien et laisse `prefers-color-scheme`
décider.

Le choix ne survit pas à un changement de page : rien ne peut le mémoriser
sans JavaScript. C'est le prix d'une page qui n'en expédie pas, et il vaut
mieux l'écrire que de laisser croire le contraire.

## Les couleurs et le logo

Les couleurs de marque ne sont pas choisies dans la feuille de style, elles
sont **relevées dans l'application** : `--marine` est le `LaunchBackground`
du catalogue d'assets — rgb(0.055, 0.165, 0.369) — et `--bleu` le bas du
dégradé de l'écran de garde, rgb(0.10, 0.34, 0.90). Le bandeau reprend ce
dégradé, dans le même sens.

Le logo est l'icône de l'application, redimensionnée une fois : l'original
fait 1024 px et 907 Ko, ce qui n'a pas sa place sur une page de treize. Les
dérivés de `assets/` ont été produits ainsi, et le dyno n'a rien à traiter :

```sh
python3 - <<'EOF'
from PIL import Image
src = Image.open('../frontend/MonCapGPS/Assets.xcassets/AppIcon.appiconset/AppIcon.png').convert('RGB')
src.resize((256, 256), Image.LANCZOS).save('assets/logo.webp', 'WEBP', quality=88, method=6)
src.resize((32, 32), Image.LANCZOS).convert('P', palette=Image.ADAPTIVE, colors=64).save('assets/favicon.png', 'PNG', optimize=True)
EOF
```

WebP seul, sans repli PNG — 14,3 Ko contre 87 Ko. Le format est disponible
partout depuis 2020, et les thèmes reposent de toute façon sur `:has()`,
plus récent encore : un repli pour un navigateur d'avant 2020 protégerait
une page qu'il ne saurait déjà pas afficher.

## Contenu

Tout le texte chiffré est dans `src/content.ts`, séparé de la mise en forme.
Les nombres qui y figurent sont **mesurés** : les tailles viennent des
Releases publiées, les 222,5 km de Bordeaux → Limoges d'un calcul réel sans
réseau (voir `.github/workflows/valhalla-epreuve.yml`). Une vitrine qui
arrondit ses chiffres finit par mentir.

Le plafond de quinze itinéraires n'est pas une estimation : il vient de la
ligne `options.prefix(15)` de `ContentView.swift`. `ITINERAIRES_MAX` est la
seule source du chiffre affiché, dans les trois langues.

La pile technique (`OUTILS`) est listée **sans numéro de version**. Une
version affichée sur une page vieillit toute seule : elle est fausse dès la
mise à jour suivante et personne ne revient la corriger. Le nom, lui, reste
vrai ; qui veut la version la lit dans `Cargo.toml` ou `package.json`, où
elle est tenue à jour par construction. `src/content.test.ts` fait échouer la
construction si un chiffre réapparaît dans la liste — c'est une règle qu'on
réintroduit sans y penser, « React 19 » se tape tout seul.

`src/styles.test.ts` relit la feuille de style à la recherche de couleurs
mal tapées. J'ai écrit `#2b3burlywood`, `#142banchor` et `#1b3straw` dans la
même journée : un nom de couleur CSS venu se coller à un code hexadécimal.
Rien n'échoue — le navigateur ignore la déclaration et sert la page avec un
trait manquant. Le test ne regarde que les déclarations, jamais les
sélecteurs : `#th-auto` est un identifiant, pas une couleur.

## Les maquettes

Les quatre écrans montrés (comparateur, guidage, bandeau des lieux, régions
hors ligne) sont du
**HTML et du CSS**, pas des images : nets à toute résolution, ils suivent le
thème choisi, se lisent au lecteur d'écran et ne coûtent pas un octet de plus
à télécharger.

Ce sont des **dessins**, et la légende le dit. La mise en page, les étiquettes
et les lignes affichées reprennent celles de l'application — les valeurs
chiffrées, elles, sont des exemples. Faire passer un exemple pour un relevé
serait le premier mensonge d'une vitrine.

## Style

Pas de bibliothèque d'utilitaires : `src/styles.css` tient en quelques
centaines de lignes, avec des variables CSS pour les quatre thèmes,
`prefers-color-scheme` pour le mode automatique, `env(safe-area-inset-*)`
pour l'encoche et des cibles tactiles d'au moins 44 px.
