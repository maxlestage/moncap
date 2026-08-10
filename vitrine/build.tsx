/**
 * Construit la page une fois pour toutes, à la place du navigateur.
 *
 * La vitrine n'a aucune interactivité : rien n'y change après l'affichage.
 * Expédier React pour peindre du texte immobile coûterait quatre cents
 * kilooctets de JavaScript — sur un forfait mobile, c'est la seconde
 * d'attente qu'on cherchait à éviter. Le rendu se fait donc ici, et la page
 * livrée ne contient que du HTML et une feuille de style.
 *
 * React et TypeScript restent la façon d'écrire la page ; ils ne sont
 * simplement pas du voyage.
 */
import { renderToStaticMarkup } from "react-dom/server";
import { App } from "./src/App";

const SORTIE = "dist";

await Bun.$`rm -rf ${SORTIE}`;

// Le nom porte l'empreinte du contenu, et le dossier entier est servi sous
// `/vitrine` — une route fixe n'aurait pas pu suivre un nom qui change.
//
// C'est l'unique façon de corriger un cache déjà empoisonné. Un en-tête ne
// s'applique qu'à une réponse que le navigateur demande : tant qu'il croit
// sa copie de `/styles.css` fraîche, il ne redemande rien et ne voit aucun
// nouvel en-tête. Une adresse neuve, elle, n'a pas d'entrée en cache.
const css = await Bun.build({
  entrypoints: ["./src/styles.css"],
  outdir: SORTIE,
  minify: true,
  naming: "[name]-[hash].[ext]",
});
if (!css.success) {
  console.error(css.logs);
  process.exit(1);
}

const feuille = css.outputs.find((o) => o.path.endsWith(".css"));
if (!feuille) throw new Error("Feuille de style introuvable après construction");
const nomFeuille = feuille.path.split("/").pop();

// Le gabarit sert aux deux chemins : le mode développement l'utilise tel
// quel avec React, la construction y verse le rendu et la feuille.
const gabarit = await Bun.file("index.html").text();
const page = gabarit
  .replace(
    "</head>",
    // Chemin absolu : la page est servie à la racine (`/`), un `./` y
    // marcherait, mais `/vitrine/` dit où le dossier est réellement monté
    // et reste juste si la page venait à être servie ailleurs.
    `  <link rel="stylesheet" href="/vitrine/${nomFeuille}" />\n  </head>`,
  )
  .replace(
    '<div id="root"></div>',
    `<div id="root">${renderToStaticMarkup(<App />)}</div>`,
  )
  .replace(/\n\s*<script type="module"[^>]*><\/script>/, "");

await Bun.write(`${SORTIE}/index.html`, page);

const poids = (await Bun.file(`${SORTIE}/index.html`).arrayBuffer()).byteLength;
console.log(`index.html   ${(poids / 1024).toFixed(1)} Ko`);
console.log(`${nomFeuille}   ${(feuille.size / 1024).toFixed(1)} Ko`);
console.log("aucun JavaScript expédié");
