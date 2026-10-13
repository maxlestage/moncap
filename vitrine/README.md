# Vitrine

Page de présentation de MonCap GPS, en **français, espagnol et anglais**,
pensée pour le téléphone d'abord.

Écrite en **Rust**, avec [Paquets](https://github.com/maxlestage/Paquets) —
plus précisément sa partie front, `active`, prise par la fonctionnalité
`client` de `paquets`. Les vues sont des fonctions qui rendent un `Node` :
compilées en natif, elles écrivent **chaque page en HTML complet** à la
construction ; compilées en WebAssembly, elles réhydratent les deux îlots
interactifs de la page. Le texte des trois langues n'existe qu'une fois.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # celle du Cargo.lock
npm install -g binaryen          # fournit wasm-opt (facultatif)

cargo test                       # 47 tests
cargo fmt --check && cargo clippy --all-targets -- -D warnings
cargo run --release --bin construit   # écrit dist/
```

`construit` enchaîne tout : il compile le WebAssembly, le passe à
`wasm-bindgen` puis `wasm-opt`, empreinte les fichiers, rend les neuf pages,
le plan du site et les consignes aux robots. Ses outils — empreinte,
allègement de la feuille, chaîne WebAssembly — vivent dans `../fabrique`,
partagés avec le front web ; les animations dans `../mouvement`.

`wasm-opt` est un confort : absent, la construction le dit et livre un
WebAssembly plus gros, ce qui ne l'empêche pas de fonctionner.

`wasm-bindgen` est verrouillé à la version exacte du front web (`=0.2.129`
dans `Cargo.toml`) : la CI n'installe qu'un outil `wasm-bindgen-cli` pour
les deux fronts, et refuse deux `Cargo.lock` qui divergent.

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
compile que la crate du serveur : rien ne fabrique le WebAssembly sur le
dyno, donc le résultat de la construction doit être dans le dépôt.

Il n'y a rien à lancer à la main pour autant : le workflow « Front »
(`.github/workflows/front.yml`, partagé avec le front web) vérifie,
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

## Rendue à la construction, réhydratée par îlots

La page a été rendue à la construction (HTML pur, zéro JavaScript), puis
par le navigateur (Yew : le corps partait vide et le WebAssembly le
peignait). Elle est désormais **les deux à la fois**, comme `active` le
propose : `construit` rend chaque page en entier avec les vues de
`src/app.rs`, et le navigateur ne repeint rien. Le WebAssembly fait trois
gestes, dans `src/bin/app.rs` :

- **réhydrater les îlots** (`vitrine::ILOTS`) — les deux seules parties
  interactives : la flèche de défilement (`src/fleche.rs`) et l'année du
  copyright, que le navigateur remplace par la sienne. Le rendu les
  enveloppe d'un `<active-island data-island=… data-props=…>` ; le
  navigateur reconstruit la même vue à partir des mêmes propriétés et
  s'accroche au HTML existant, sans rien recréer ;
- **démarrer le moteur de mouvement** (`../mouvement`) ;
- **lever le rideau**.

| | poids livré (gzip) |
| --- | --- |
| Version précédente, rendue à la construction | **15,8 Ko**, zéro JavaScript |
| Version Yew/WebAssembly | **128 Ko** — 114 de WebAssembly, 6,7 de colle, 2,2 de HTML, 5 de CSS |
| Version animée (moteur `mouvement`) | **148 Ko** — 132 de WebAssembly, 8,1 de colle, 2,4 de HTML, 5,5 de CSS |
| Seconde vague (curseur, inclinaison, lettres, carte vivante…) | **163 Ko** — 144 de WebAssembly, 8,7 de colle, 2,3 de HTML, 7,6 de CSS |
| Paquets (`active`), rendue à la construction et réhydratée | **83 Ko** — 56 de WebAssembly, 7 de colle, 12,6 de HTML, 7,6 de CSS |

Le WebAssembly a fondu (352 → 117 Ko bruts, après `wasm-opt`) parce qu'il
n'embarque plus **aucun texte** ni aucune vue de la page : le contenu est
déjà dans le HTML, et les deux îlots reçoivent ce qu'il leur faut — les
libellés de la flèche, l'année — par leurs propriétés. Il ne reste que le
moteur de mouvement, la flèche et la réhydratation. Le HTML, lui, grossit
d'autant : c'est le texte, qui ne voyage plus deux fois.

Conséquences :

- **Tout est lisible sans script.** Robots qui n'exécutent rien, aperçus
  de liens, navigateurs anciens, scripts coupés : chacun reçoit la page
  entière, maquettes comprises. Le repli `<noscript>` de la version Yew,
  qui n'en donnait que la substance, n'a plus de raison d'être.
- **Le rideau reste.** Il couvre la page pendant que le WebAssembly
  arrive : sans lui, les titres paraîtraient immobiles puis rejoueraient
  leur entrée quand le moteur prend la main. Sans script il ne s'affiche
  pas (`<noscript><style>.rideau{display:none}</style></noscript>`), et la
  page est simplement immobile.
- **Ce que les tests lisent est ce que le visiteur reçoit.** `tests/liens.rs`
  et les tests de `construit` rendent les mêmes fonctions que la
  construction, sans moteur de rendu de test à part.

Le passage de Yew à `active` n'a rien changé au document : rendus et
normalisés (commentaires et enveloppes d'îlots ôtés), les neuf pages ont
le même arbre, nœud pour nœud, que celles de la version Yew — hormis la pile
technique, qui dit désormais « Paquets ».

## Ce que Rust achète ici

Ce n'est pas la vitesse : la page ne calcule rien.

- **Une seule source pour le texte.** `src/contenu.rs` nourrit le rendu
  des pages, leur tête et le plan du site, depuis le même binaire. Avant,
  le contenu était en TypeScript et le serveur en Rust ; le référencement
  recopiait des chaînes d'un monde à l'autre.
- **Le dernier script est passé sous contrôle.** La flèche de défilement
  était cinquante lignes de JavaScript dans une chaîne de caractères,
  qu'aucun outil ne relisait. C'est désormais `src/fleche.rs`, que le
  compilateur vérifie et que six tests couvrent — la règle du sens est une
  fonction pure, testable sans navigateur.
- **Des types qui disent ce qu'ils veulent dire.** `Teinte` est une
  énumération, `avancement` une `Option`, les tuples de `trajet` sont
  devenus des champs nommés. Une quatrième couleur est maintenant une erreur
  de compilation, plus une classe CSS absente découverte à l'écran.

## Le mouvement

Les animations suivent [zamocorp.com](https://zamocorp.com), dont l'analyse
des paquets est consignée dans l'historique (#383) : GSAP et ScrollTrigger
pour les cascades, les masques et la progression liée au défilement, Lenis
pour la molette qui glisse. Les gestes en sont repris, en Rust, par la crate
`../mouvement`, partagée avec le front web :

- **le rideau** de chargement, qui se lève quand la page est montée ;
- **les titres montent mot à mot** derrière leur masque (le « yPercent »
  de GSAP), découpés au rendu par `mots()` — à la construction, donc : le
  DOM n'est jamais retouché après coup, et les tests voient le même HTML ;
- **les cascades** : cartes, tarifs, pile, pied — chaque élément entre à
  son tour, avec un retard plafonné ;
- **les maquettes s'allument** par le bas (`clip-path`) ;
- **les chiffres comptent** jusqu'à leur valeur — les vignettes du bandeau,
  les prix, la mesure Bordeaux → Limoges — et finissent sur le texte
  d'origine, à l'octet près ;
- **la molette glisse**, comme Lenis : un amortissement de 0,1 par image à
  60 Hz, ramené à l'intervalle réel pour que la glisse soit la même à
  120 Hz. Le toucher n'est jamais intercepté ;
- **le fil de l'itinéraire** se dessine au défilement, le bandeau dérive et
  s'efface, le **bandeau défilant** (les promesses du hors-ligne, déjà
  traduites) s'incline avec l'élan ;
- **les boutons sont magnétiques**, à la souris seulement.

**La seconde vague** — « bien plus d'animation sur tout le site » — donne
un geste à chaque section :

- **les grands titres montent lettre à lettre** (`lettres()`), chacune
  derrière le masque de son mot ; le lecteur d'écran reçoit le titre d'un
  bloc (`visuel-cache`), les lettres lui sont cachées ;
- **une carte vivante** sous le bandeau : des rues, trois itinéraires qui se
  tracent puis s'effacent en boucle, des épingles qui pulsent, une voiture
  qui roule, une lueur qui suit la souris ; elle s'éloigne quand on
  descend ;
- **un curseur qui suit** — un point exact, un anneau qui le rattrape —, qui
  grossit sur ce qui se clique et porte une étiquette sur les liens qui en
  ont une (`data-curseur`). Il s'ajoute à la flèche du système, il ne la
  remplace pas ;
- **les vignettes, les cartes et les tarifs s'inclinent** vers la souris,
  une lueur dedans suit le pointeur ; la formule vedette a un cadre qui
  tourne ;
- **les maquettes se redressent** en traversant l'écran, puis leur contenu
  arrive à son tour : itinéraires, pastilles, catégories, jauge qui se
  remplit avec son reflet, bouton « Démarrer » qui appelle ;
- **chaque section a son numéro**, « 01 » suivi d'un trait qui se tire ;
- **la preuve se trace** : Bordeaux → Limoges se dessine au défilement,
  une voiture en tête ;
- **un second bandeau** en lettres creuses, les outils de la pile, défile en
  sens inverse ;
- **les liens roulent** au survol — le libellé monte, son double le
  remplace ;
- **une barre de lecture** en haut de l'écran, **un anneau de progression**
  autour de la flèche ;
- **la signature « MonCap GPS » en très grand** ferme le pied, lettre à
  lettre ;
- **un volet monte** avant de suivre un lien interne — vers les pages
  légales, une autre langue —, et la page d'arrivée lève son rideau de la
  même couleur : on ne voit jamais de blanc. Nouvel onglet, ancre,
  téléchargement et site extérieur gardent le comportement du navigateur.

Le curseur, l'inclinaison et la parallaxe n'existent qu'à la souris : au
doigt, ils suivraient un pouce qui cache déjà ce qu'il touche. Un piège vu
en regardant : la page qui glisse sous une souris immobile ne déclenche
aucun `pointermove`, et le curseur gardait l'étiquette d'un bouton parti
plus haut — le moteur relit ce qui est sous la souris à chaque défilement.

**Pourquoi plus seulement du CSS.** La version précédente liait tout à
`animation-timeline`, sans un octet de script de plus. Mais la page charge
de toute façon un WebAssembly — le moteur n'y coûte que son propre code —
et Firefox n'a toujours pas activé `animation-timeline` par défaut
(encore derrière un drapeau en version 152, juin 2026) : une part des
visiteurs voyait une page immobile. Les révélations passent maintenant par
un `IntersectionObserver`, qui marche partout.

**Ce que ça coûte**, mesuré avec `twiggy` : environ 19 Ko gzippés (113 → 132
Ko de WebAssembly) pour la première vague, 12 de plus pour la seconde
(132 → 144) — 18 Ko bruts pour le moteur, 9 pour le rendu mot à mot,
7 de chaînes. Il en coûtait 15 de plus dans une première version : le
compteur lisait « 3,99 » avec `str::parse::<f64>`, qui embarque les tables
de l'analyseur de flottants de la bibliothèque standard. Il lit
maintenant deux entiers.

**Le contenu reste visible par défaut, toujours.** Tout état masqué vit
sous `html.mvt`, la classe que le moteur pose en démarrant et retire s'il
s'arrête. Le moteur ne démarre pas quand le système demande moins de
mouvement (`prefers-reduced-motion`) ; le bloc « mouvement réduit » de la
feuille coupe en plus toute animation.

Deux pièges trouvés en regardant vraiment la page :

- **Chromium applique le `clip-path` d'un élément au calcul de son
  intersection avec l'écran.** Découpé à son départ, un élément n'offrait
  qu'une bande sous son bord bas : hors champ sur un grand écran, il
  n'était jamais « vu », et restait caché — la carte entière du front web,
  à 1280 px. Le dévoilement découpe donc l'enfant, jamais l'élément observé.
- **Une révélation par `transition` garde son délai pour toujours** : une
  carte survolée se serait soulevée avec une demi-seconde de retard. Ce
  sont des animations jouées une fois, en `backwards`, qui rendent ensuite
  l'élément à ses propres règles.

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

`cargo test` couvre le contenu lui-même : pas de version dans la pile, pas de
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

`src/contenu.rs` porte les trois versions, dans un seul fichier plutôt que
trois jumeaux qui divergeraient en silence. Le type `Contenu` est la
garantie : oublier une phrase dans une langue ne produit pas une page à
trou, mais **une erreur de compilation**. Un test complète ce que le type
ne peut pas voir — une chaîne vide compile très bien.

Les libellés visibles **dans les maquettes** ne sont pas retapés : ils
viennent de `src/appstrings.rs`, engendré par `frontend/traductions.py` à
partir du tableau qui alimente aussi le catalogue iOS. Un écran dessiné ici
dit donc exactement ce que l'application affiche, dans la même langue, et ne
peut pas s'en écarter sans que la génération le montre. Un job d'intégration
continue refuse d'ailleurs un module engendré qui aurait divergé du tableau.

Les lignes chiffrées sont composées en versant des valeurs d'exemple dans les
**formats de l'application** (`src/traduire.rs`), plutôt qu'en recopiant des
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

Tout le texte chiffré est dans `src/contenu.rs`, séparé de la mise en forme.
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
elle est tenue à jour par construction. les tests de `src/contenu.rs` font échouer la
construction si un chiffre réapparaît dans la liste — c'est une règle qu'on
réintroduit sans y penser, « Yew 0.23 » se tape tout seul.

Le contrôle de la feuille de style la relit à la recherche de couleurs
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

## La bande blanche sous la barre d'état

Sur iPhone, une bande claire séparait la barre de Safari du bandeau bleu. Ce
n'était ni l'encoche ni un défaut de mise en page, et trois corrections
plausibles n'y ont rien changé : `background` sur `html`, une barre fixe
haute de `env(safe-area-inset-top)`, un `theme-color` au bleu de marque.

La raison tient en deux faits.

**En navigation normale, `env(safe-area-inset-top)` vaut zéro dans Safari.**
Le contenu ne passe pas sous la barre d'état — c'est le navigateur qui occupe
cette bande. Une barre fixe dimensionnée par cette variable mesure donc zéro
et ne couvre rien. Elle reste utile ailleurs (écran d'accueil, plein écran,
paysage), pas ici.

**Safari teinte sa barre d'après le fond du corps.** Pas d'après `html`, pas
d'après ce qui est peint en haut de la page. `body` portait `--papier` : le
navigateur lisait du blanc et peignait du blanc, au-dessus d'un bandeau bleu
foncé et d'une toile bleu foncé.

Le corps porte donc le bleu de marque, et le papier est descendu sur `main`,
qui va d'un bord à l'autre — `.dedans` tient la largeur du texte, comme dans
le bandeau et le pied. Rien ne change à l'œil ; la barre du navigateur, elle,
prend enfin la teinte.

Remettre `--papier` sur `body` est le geste le plus naturel du monde, et
aucune capture ne le rattraperait : la barre du navigateur ne se photographie
pas. Un test s'en charge.

## Style

Pas de bibliothèque d'utilitaires : `src/styles.css` tient en quelques
centaines de lignes, avec des variables CSS pour les quatre thèmes,
`prefers-color-scheme` pour le mode automatique, `env(safe-area-inset-*)`
pour l'encoche et des cibles tactiles d'au moins 44 px.
