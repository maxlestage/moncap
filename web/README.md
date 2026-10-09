# MonCap GPS — front web (Yew + WebAssembly)

L'interface web qui consomme la même API que l'app iOS, écrite en **Rust**
avec [Yew](https://yew.rs) et compilée en **WebAssembly**. Elle remplace
l'ancienne application React + Bun + Leaflet : un seul langage du serveur
au navigateur, et les types échangés avec l'API vérifiés par le compilateur
plutôt que recopiés à la main.

On y entre par `/app`. Les fichiers vivent sous `/web/…`, nommés d'après
leur contenu (voir « Les adresses et le cache »).

## Construire

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # la version de Cargo.lock
npm install -g binaryen                            # fournit wasm-opt (facultatif)

cargo test                                         # 54 tests, sans navigateur
cargo fmt --check && cargo clippy --all-targets -- -D warnings
cargo run --release --bin construit                # écrit dist/
```

`construit` compile le WebAssembly, le passe à `wasm-bindgen` puis
`wasm-opt`, empreinte chaque fichier, et écrit la coquille `dist/index.html`.
Les outils sont ceux de la vitrine, partagés par la crate `../fabrique`.

**Il n'y a rien à lancer à la main.** Comme la vitrine, `dist/` est
versionné — le buildpack Heroku ne compile que la crate du serveur — et
`.github/workflows/front.yml` le reconstruit et le verse à chaque changement
des sources poussé sur `master`.

Pour essayer en local : lancer le serveur à la racine du dépôt
(`cargo run`, voir le README principal), puis ouvrir `http://localhost:3000/app`.
L'URL d'une autre API se règle sur l'écran de connexion, « Serveur (avancé) ».

## Fonctionnalités

Celles de l'ancien front, une par une :

- Carte OpenStreetMap avec les positions numérotées et le tracé qui les
  relie ; toucher la carte pose une position, qu'on nomme dans une bulle.
- Liste des positions : renommer sur place, supprimer.
- Itinéraire complet (distance et durée à la vitesse choisie), chiffres
  (positions, longueur du tracé, conducteurs en direct).
- Import et export **GPX**.
- **Temps réel** : synchronisation des positions entre écrans, partage de sa
  position, voitures des autres en direct, signalements communautaires et
  votes — pastille verte quand le WebSocket est connecté.
- Recherche de destination, avec distance et direction depuis sa position.
- **Avatars** : quatorze véhicules, qui vous représentent sur la carte des
  autres.

### Ce qui a changé en route

Plusieurs défauts de l'ancien front sont corrigés, et chacun a son test :

- **On ne se voit plus en double.** Le serveur renvoie sa position à tout le
  monde, nous compris ; il annonce aussi notre identifiant (`hello`), que
  l'ancien front ignorait. Il est maintenant lu, et notre écho écarté.
- **Un signalement infirmé disparaît tout de suite** (`alert_gone`, lui
  aussi ignoré jusqu'ici), au lieu de rester affiché jusqu'à son expiration.
  Les signalements sont resynchronisés chaque minute : le serveur purge les
  expirés sans prévenir personne.
- **Un conducteur immobile ne disparaît plus.** La position n'était envoyée
  que si l'on avait bougé de huit mètres ; arrêté, on s'effaçait de la
  carte des autres au bout de quinze secondes, et un nouveau venu ne voyait
  jamais ceux qui étaient déjà là. Un battement redit la position toutes
  les cinq secondes.
- **Changer d'avatar se voit tout de suite** chez les autres : l'ancien
  front gardait celui du démarrage du partage.
- **La recherche respecte la politique de Nominatim.** L'ancien front
  interrogeait le service à chaque pause de la frappe : une auto-complétion,
  que la [politique d'usage](https://operations.osmfoundation.org/policies/nominatim/)
  du serveur public interdit en toutes lettres, sous peine de blocage. La
  recherche part désormais sur Entrée, une fois par seconde au plus, et une
  question déjà posée n'est pas reposée.
- **Les tuiles viennent de `tile.openstreetmap.org`**, sans les sous-domaines
  `a.`, `b.`, `c.` que l'équipe d'OpenStreetMap demande d'abandonner, avec
  l'attribution que la licence ODbL exige.
- **Les bulles ne prennent plus de HTML.** Leaflet construisait ses bulles
  à partir de chaînes : chaque libellé devait être échappé à la main. Ce
  sont maintenant des composants, où un libellé est du texte.
- Les distances s'écrivent comme dans l'app iOS : des mètres sous le
  kilomètre (`Distance.courte`).

## La carte

Elle est écrite en Rust (`src/carte/`), à la place de Leaflet. Leaflet 1.9.4
date de 2023, sa version 2 n'est toujours qu'en alpha, et il arrivait par
un objet global chargé depuis un CDN tiers — un script hors de tout
contrôle, sur un domaine que le site ne maîtrise pas.

La géométrie (`projection.rs` : Web Mercator, tuiles, cadrage) est faite de
fonctions pures, testées sans navigateur — la tuile de Bordeaux au zoom 10,
le point qui reste sous le curseur pendant un zoom, le cadrage qui montre
tous les points et pas davantage. Le composant ne fait que la brancher sur
l'écran : glisser, lancer avec inertie, pincer, molette, double toucher,
clavier (flèches, `+`/`−`, Entrée pour poser une position au centre),
cadrage animé, bulles qui se recadrent pour rester visibles.

Le plan est un calque unique, déplacé d'un seul `transform` au glissement,
et arrondi au pixel : translaté d'une fraction, il faisait apparaître un
liseré clair entre les tuiles. Les voitures roulent d'un relevé à l'autre
par une transition CSS, coupée pendant un zoom.

En sombre, les tuiles sont inversées par un filtre : OpenStreetMap n'en
publie qu'en clair.

## Le mouvement

Les animations suivent [zamocorp.com](https://zamocorp.com) — titres qui
montent mot à mot derrière leur masque, cascades, volet plein écran entre la
connexion et le tableau, rideau de chargement, molette inertielle, boutons
magnétiques, compteurs, route qui se dessine, bandeau défilant. Elles sont
jouées par la crate `../mouvement`, partagée avec la vitrine, et décrites
dans le README principal.

## Les adresses et le cache

| Adresse | Contenu | `Cache-Control` |
| --- | --- | --- |
| `/app` | la coquille | `no-cache` — revalidée à chaque visite |
| `/web/<nom>-<empreinte>.<ext>` | feuille, WebAssembly, colle, images | `public, max-age=31536000, immutable` |

La même répartition que la vitrine, pour la même raison : seule la coquille
garde une adresse fixe. Elle lie ses fichiers par des chemins absolus, et
fonctionne donc à n'importe quelle adresse — le serveur la renvoie aussi en
repli pour les liens profonds. Les adresses des images, que seule la
construction connaît, voyagent en JSON sur le point de montage
(`data-ressources`).

## Ce que ça pèse

Mesuré sur la construction, et non supposé :

| | gzip |
| --- | --- |
| Ancien front : React (125 Ko) + feuille (2,5 Ko) + Leaflet depuis unpkg (~46 Ko) | **~174 Ko** |
| Front Yew : WebAssembly (227 Ko) + colle (9,8 Ko) + feuille (7 Ko) + coquille (1 Ko) | **~245 Ko** |

Soit environ 40 % de plus. Le poids est réparti, sans dépendance qui domine
(relevé avec `twiggy` sur une construction non dépouillée) : le plus gros
morceau est le rendu du tableau de bord lui-même, puis la carte et la
lecture JSON des événements. C'est le prix d'une application entière en
WebAssembly ; il se paie une fois, les fichiers étant ensuite gardés un an
par le navigateur. Les avatars, eux, sont passés de PNG à WebP :
214 Ko → 66 Ko.

## Les avatars

Ce sont les images du catalogue de l'app iOS, à l'octet près, converties
une fois en WebP :

```sh
python3 - <<'EOF'
from PIL import Image
import glob, os
for f in glob.glob('../frontend/MonCapGPS/Assets.xcassets/*.imageset/*.png'):
    nom = os.path.splitext(os.path.basename(f))[0]
    if nom in ('Logo',): continue
    Image.open(f).convert('RGBA').save(f'assets/avatars/{nom}.webp', 'WEBP', quality=90, method=6)
EOF
```

Un test vérifie que chaque avatar listé dans `src/catalogue.rs` a son
image : un oubli se verrait chez les autres, sous la forme d'une image
cassée sur la carte.

## Fichiers

- `src/bin/app.rs` — le point d'entrée du WebAssembly
- `src/bin/construit.rs` — fabrique `dist/`
- `src/app.rs` — la racine : la porte ou le tableau, et le volet entre les deux
- `src/porte.rs` — connexion, inscription, mot de passe oublié
- `src/tableau.rs` — le tableau de bord
- `src/carte/` — la carte (`projection.rs` pour la géométrie)
- `src/etat.rs` — l'état du tableau et ses règles (réducteur pur)
- `src/temps_reel.rs` — le WebSocket, avec reconnexion
- `src/api.rs` — le client de l'API
- `src/nominatim.rs` — la recherche d'adresse
- `src/geo.rs` — distances, caps, envoi de la position
- `src/localisation.rs` — la géolocalisation du navigateur
- `src/catalogue.rs` — signalements et avatars
- `src/ui.rs` — mots masqués, compteur
- `src/styles.css` — la feuille de style
