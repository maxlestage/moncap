/**
 * Construit les pages une fois pour toutes, à la place du navigateur.
 *
 * La vitrine n'a aucune interactivité scriptée : rien n'y change après
 * l'affichage, pas même le choix du thème — il passe par des boutons radio
 * et `:has()`. Expédier React pour peindre du texte immobile coûterait
 * quatre cents kilooctets de JavaScript ; sur un forfait mobile, c'est la
 * seconde d'attente qu'on cherchait à éviter. Le rendu se fait donc ici, et
 * les pages livrées ne contiennent que du HTML, une feuille de style et une
 * image.
 *
 * React et TypeScript restent la façon d'écrire la page ; ils ne sont
 * simplement pas du voyage.
 *
 * Chaque fichier servi porte l'empreinte de son contenu dans son nom. C'est
 * la seule façon de corriger un cache déjà pris : un en-tête ne s'applique
 * qu'à une réponse que le navigateur demande, tandis qu'une adresse neuve
 * n'a jamais d'entrée en cache.
 */
import { renderToStaticMarkup } from "react-dom/server";
import { App } from "./src/App";
import { LANGUES } from "./src/langues";
import type { Contenu } from "./src/langues";

const SORTIE = "dist";
/** Préfixe sous lequel le backend monte le dossier construit. */
const BASE = "/vitrine";

await Bun.$`rm -rf ${SORTIE}`;
await Bun.$`mkdir -p ${SORTIE}`;

// --- Feuille de style ----------------------------------------------------

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

// --- Images --------------------------------------------------------------

/**
 * Copie un fichier en insérant l'empreinte de son contenu dans son nom.
 * Les images viennent du catalogue d'assets de l'application, redimensionnées
 * une fois (voir vitrine/README.md) : le dyno ne sait pas traiter d'images.
 */
async function copieEmpreinte(source: string): Promise<string> {
  const octets = await Bun.file(source).arrayBuffer();
  const empreinte = Bun.hash(octets).toString(36).slice(0, 8);
  const point = source.lastIndexOf(".");
  const nom = `${source.slice(source.lastIndexOf("/") + 1, point)}-${empreinte}${source.slice(point)}`;
  await Bun.write(`${SORTIE}/${nom}`, octets);
  return `${BASE}/${nom}`;
}

const logo = await copieEmpreinte("assets/logo.webp");
const favicon = await copieEmpreinte("assets/favicon.png");

// --- Pages ---------------------------------------------------------------

const gabarit = await Bun.file("index.html").text();

/** Échappe ce qui part dans un attribut HTML. */
function attr(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/"/g, "&quot;");
}

/**
 * Les balises de tête propres à une langue : titre, description, icône,
 * feuille, et les `alternate` qui disent aux moteurs que les trois pages
 * sont la même, dans trois langues.
 */
function tete(c: Contenu): string {
  const alternates = [
    ...LANGUES.map(
      (l) => `<link rel="alternate" hreflang="${l.code}" href="${l.chemin}" />`,
    ),
    `<link rel="alternate" hreflang="x-default" href="/" />`,
  ];
  return [
    `<title>${attr(c.titre)}</title>`,
    `<meta name="description" content="${attr(c.description)}" />`,
    `<meta property="og:title" content="${attr(c.titre)}" />`,
    `<meta property="og:description" content="${attr(c.description)}" />`,
    `<meta property="og:image" content="${logo}" />`,
    `<meta property="og:locale" content="${c.code}" />`,
    `<link rel="icon" type="image/png" href="${favicon}" />`,
    `<link rel="apple-touch-icon" href="${logo}" />`,
    ...alternates,
    `<link rel="stylesheet" href="${BASE}/${nomFeuille}" />`,
  ]
    .map((l) => `    ${l}`)
    .join("\n");
}

const pages: { chemin: string; octets: number }[] = [];

for (const c of LANGUES) {
  const page = gabarit
    .replace('<html lang="fr">', `<html lang="${c.code}">`)
    .replace("  </head>", `${tete(c)}\n  </head>`)
    .replace(
      '<div id="root"></div>',
      `<div id="root">${renderToStaticMarkup(<App c={c} res={{ logo }} />)}</div>`,
    )
    .replace(/\n\s*<script type="module"[^>]*><\/script>/, "");

  // Le français occupe la racine ; les autres, un sous-dossier.
  const fichier =
    c.chemin === "/" ? `${SORTIE}/index.html` : `${SORTIE}${c.chemin}/index.html`;
  await Bun.write(fichier, page);
  pages.push({ chemin: c.chemin, octets: page.length });
}

// --- Récapitulatif -------------------------------------------------------

const ko = (n: number) => `${(n / 1024).toFixed(1)} Ko`;
for (const p of pages) console.log(`${p.chemin.padEnd(6)} ${ko(p.octets)}`);
console.log(`${nomFeuille}   ${ko(feuille.size)}`);
console.log(
  `${logo.split("/").pop()}   ${ko((await Bun.file("assets/logo.webp").arrayBuffer()).byteLength)}`,
);
console.log("aucun JavaScript expédié");
