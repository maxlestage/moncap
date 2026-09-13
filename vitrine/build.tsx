/**
 * Construit les pages une fois pour toutes, à la place du navigateur.
 *
 * La vitrine n'a presque aucune interactivité scriptée : le choix du thème
 * passe par des boutons radio et `:has()`, et seule la flèche de défilement
 * embarque un script inline d'un demi-kilooctet. Expédier React pour peindre du texte immobile coûterait
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
import { Legale } from "./src/Legale";
import { LANGUES } from "./src/langues";
import type { Contenu, PageLegale } from "./src/langues";
import {
  groupes,
  jsonLd,
  jsonLdLegale,
  OG_LOCALES,
  PUBLIC,
  robots,
  sitemap,
  verificationGoogle,
} from "./src/seo";

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
 * Les balises de tête propres à une langue : titre, description, la
 * canonique et les `alternate` en adresses **absolues** (un hreflang relatif
 * est invalide pour les moteurs), Open Graph et Twitter complets, les
 * données structurées schema.org — prix compris —, l'icône et la feuille.
 */
function tete(c: Contenu): string {
  const alternates = [
    ...LANGUES.map(
      (l) =>
        `<link rel="alternate" hreflang="${l.code}" href="${PUBLIC}${l.chemin}" />`,
    ),
    `<link rel="alternate" hreflang="x-default" href="${PUBLIC}/" />`,
  ];
  const autresLocales = LANGUES.filter((l) => l.code !== c.code).map(
    (l) =>
      `<meta property="og:locale:alternate" content="${OG_LOCALES[l.code]}" />`,
  );
  return [
    `<title>${attr(c.titre)}</title>`,
    `<meta name="description" content="${attr(c.description)}" />`,
    `<link rel="canonical" href="${PUBLIC}${c.chemin}" />`,
    `<meta name="robots" content="index,follow,max-image-preview:large" />`,
    ...verificationGoogle(),
    `<meta name="theme-color" content="#0e2a5e" />`,
    `<meta property="og:type" content="website" />`,
    `<meta property="og:site_name" content="MonCap GPS" />`,
    `<meta property="og:url" content="${PUBLIC}${c.chemin}" />`,
    `<meta property="og:title" content="${attr(c.titre)}" />`,
    `<meta property="og:description" content="${attr(c.description)}" />`,
    `<meta property="og:image" content="${PUBLIC}${logo}" />`,
    `<meta property="og:locale" content="${OG_LOCALES[c.code]}" />`,
    ...autresLocales,
    `<meta name="twitter:card" content="summary" />`,
    `<meta name="twitter:title" content="${attr(c.titre)}" />`,
    `<meta name="twitter:description" content="${attr(c.description)}" />`,
    `<meta name="twitter:image" content="${PUBLIC}${logo}" />`,
    `<link rel="icon" type="image/png" href="${favicon}" />`,
    `<link rel="apple-touch-icon" href="${logo}" />`,
    ...alternates,
    `<script type="application/ld+json">${jsonLd(c)}</script>`,
    `<link rel="stylesheet" href="${BASE}/${nomFeuille}" />`,
  ]
    .map((l) => `    ${l}`)
    .join("\n");
}

/**
 * La tête d'une page légale : titre et description propres, la canonique,
 * et les `alternate` de son groupe — /es/privacidad traduit bien
 * /confidentialite, les adresses diffèrent mais la matière est la même,
 * c'est exactement ce que hreflang décrit. Open Graph minimal et fil
 * d'Ariane schema.org complètent, même feuille et même icône.
 */
function teteLegale(c: Contenu, p: PageLegale): string {
  const groupe =
    groupes().find((g) => g.some((page) => page.chemin === p.chemin)) ?? [];
  const alternates = [
    ...groupe.map(
      (page) =>
        `<link rel="alternate" hreflang="${page.code}" href="${PUBLIC}${page.chemin}" />`,
    ),
    `<link rel="alternate" hreflang="x-default" href="${PUBLIC}${groupe[0]?.chemin ?? "/"}" />`,
  ];
  return [
    `<title>${attr(p.titre)} — MonCap GPS</title>`,
    `<meta name="description" content="${attr(p.description)}" />`,
    `<link rel="canonical" href="${PUBLIC}${p.chemin}" />`,
    `<meta name="robots" content="index,follow,max-image-preview:large" />`,
    ...verificationGoogle(),
    `<meta name="theme-color" content="#0e2a5e" />`,
    `<meta property="og:type" content="website" />`,
    `<meta property="og:site_name" content="MonCap GPS" />`,
    `<meta property="og:url" content="${PUBLIC}${p.chemin}" />`,
    `<meta property="og:title" content="${attr(p.titre)}" />`,
    `<meta property="og:description" content="${attr(p.description)}" />`,
    `<meta property="og:locale" content="${OG_LOCALES[c.code]}" />`,
    ...alternates,
    `<script type="application/ld+json">${jsonLdLegale(c, p)}</script>`,
    `<link rel="icon" type="image/png" href="${favicon}" />`,
    `<link rel="stylesheet" href="${BASE}/${nomFeuille}" />`,
  ]
    .map((l) => `    ${l}`)
    .join("\n");
}

/** Verse un rendu dans le gabarit et l'écrit à son adresse. */
async function emet(
  code: string,
  chemin: string,
  enTete: string,
  rendu: string,
): Promise<number> {
  const page = gabarit
    .replace('<html lang="fr">', `<html lang="${code}">`)
    .replace("  </head>", `${enTete}\n  </head>`)
    .replace('<div id="root"></div>', `<div id="root">${rendu}</div>`)
    .replace(/\n\s*<script type="module"[^>]*><\/script>/, "");

  // Le français occupe la racine ; tout le reste, un sous-dossier.
  const fichier =
    chemin === "/" ? `${SORTIE}/index.html` : `${SORTIE}${chemin}/index.html`;
  await Bun.write(fichier, page);
  return page.length;
}

const pages: { chemin: string; octets: number }[] = [];

for (const c of LANGUES) {
  pages.push({
    chemin: c.chemin,
    octets: await emet(
      c.code,
      c.chemin,
      tete(c),
      renderToStaticMarkup(<App c={c} res={{ logo }} />),
    ),
  });
  for (const p of [c.mentions, c.confidentialite]) {
    pages.push({
      chemin: p.chemin,
      octets: await emet(
        c.code,
        p.chemin,
        teteLegale(c, p),
        renderToStaticMarkup(<Legale c={c} page={p} res={{ logo }} />),
      ),
    });
  }
}

// --- Robots et plan du site ----------------------------------------------

await Bun.write(`${SORTIE}/robots.txt`, robots());
await Bun.write(`${SORTIE}/sitemap.xml`, sitemap());

// --- Récapitulatif -------------------------------------------------------

const ko = (n: number) => `${(n / 1024).toFixed(1)} Ko`;
for (const p of pages) console.log(`${p.chemin.padEnd(6)} ${ko(p.octets)}`);
console.log(`${nomFeuille}   ${ko(feuille.size)}`);
console.log(
  `${logo.split("/").pop()}   ${ko((await Bun.file("assets/logo.webp").arrayBuffer()).byteLength)}`,
);
console.log("un seul script : la flèche de défilement (inline, ~0,5 Ko)");
