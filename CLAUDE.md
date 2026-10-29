# Consignes pour Claude

## Merger sur master, toujours, sans demander

Consigne du propriétaire du dépôt : chaque changement terminé est mergé sur
`master` automatiquement, sans demander de confirmation.

1. Partir du dernier `origin/master` : la CI « Front » y pousse elle-même un
   commit « Reconstruit les fronts » après chaque merge qui touche un front.
2. Travailler sur une branche, la pousser, ouvrir la pull request.
3. Merger en **squash** dès que les vérifications ci-dessous passent.
4. Après le merge, suivre le workflow « Front » sur `master` ; s'il échoue,
   corriger sur une nouvelle branche et merger de la même façon.

Le merge vient après les vérifications, jamais à leur place : une branche
qui ne passe pas ne part pas sur `master`.

## Vérifier avant de merger

Les fronts — les mêmes commandes que `.github/workflows/front.yml` :

```sh
for crate in fabrique mouvement vitrine web; do
  (cd "$crate" && cargo fmt --check \
    && cargo clippy --all-targets -- -D warnings && cargo test) || break
done
(cd mouvement && cargo clippy --target wasm32-unknown-unknown -- -D warnings)
```

Puis reconstruire ce qui a changé (`cargo run --release --bin construit`
dans `vitrine/` ou `web/`) et committer les `dist/`.

Le serveur, à la racine — sa CI (`ci.yml`) ne se lance qu'à la main :

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

La base est SQLite (`data/moncap.db`, le cache dans `data/cache.db`) : rien
à installer. Les essais de bout en bout (`src/essais.rs` et les modules)
tournent toujours, chacun sur sa propre base SQLite en mémoire.

L'app iOS (`frontend/`, Swift) a ses propres workflows ; elle ne se
construit pas ici.
