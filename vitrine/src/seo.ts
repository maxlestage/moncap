/**
 * Le référencement, en un module : l'adresse canonique, le plan du site, les
 * consignes aux robots et les données structurées.
 *
 * Un module partagé plutôt que du texte dans build.tsx : les tests importent
 * les mêmes fonctions que la construction — ce que Google lira est ce que les
 * tests ont vu.
 */
import { AUTEUR } from "./content";
import type { Contenu } from "./langues";
import { LANGUES } from "./langues";

/** L'adresse publique canonique du site. */
export const PUBLIC = "https://moncap-c41a5aaf07e8.herokuapp.com";

/** Locale Open Graph par langue (le format `fr_FR` est celui du protocole). */
export const OG_LOCALES: Record<string, string> = {
  fr: "fr_FR",
  es: "es_ES",
  en: "en_US",
};

/**
 * Les pages publiques à référencer : les trois pages principales (alternates
 * mutuelles entre langues) et les six pages légales.
 */
export function pagesPubliques(): { chemin: string; principale: boolean }[] {
  return LANGUES.flatMap((l) => [
    { chemin: l.chemin, principale: true },
    { chemin: l.mentions.chemin, principale: false },
    { chemin: l.confidentialite.chemin, principale: false },
  ]);
}

/**
 * Le plan du site. Les pages principales portent leurs alternates hreflang
 * (adresses **absolues** — un hreflang relatif est invalide pour les
 * moteurs) ; les légales, distinctes d'une langue à l'autre, vont nues.
 */
export function sitemap(): string {
  const alternates = [
    ...LANGUES.map(
      (l) =>
        `    <xhtml:link rel="alternate" hreflang="${l.code}" href="${PUBLIC}${l.chemin}"/>`,
    ),
    `    <xhtml:link rel="alternate" hreflang="x-default" href="${PUBLIC}/"/>`,
  ].join("\n");
  const urls = pagesPubliques()
    .map(
      (p) =>
        `  <url>\n    <loc>${PUBLIC}${p.chemin}</loc>\n${
          p.principale ? `${alternates}\n` : ""
        }  </url>`,
    )
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">\n${urls}\n</urlset>\n`;
}

/**
 * Les consignes aux robots : tout est ouvert sauf ce qui n'est pas une page
 * — l'application, les liens de partage et de suivi (contenu privé des
 * utilisateurs), la réinitialisation, l'API.
 */
export function robots(): string {
  return [
    "User-agent: *",
    "Disallow: /app",
    "Disallow: /t",
    "Disallow: /live",
    "Disallow: /reinitialisation",
    "Disallow: /auth/",
    "Disallow: /tiles/",
    "Disallow: /offline/",
    "",
    `Sitemap: ${PUBLIC}/sitemap.xml`,
    "",
  ].join("\n");
}

/**
 * Les données structurées schema.org : une application iOS, son auteur, ses
 * deux prix — Plus à 3,99 €/mois et Pro à 9,99 €/mois. Pas d'offre à 0 € :
 * l'essai de quatorze jours n'est pas une formule, et un prix gratuit ici
 * serait un mensonge en résultat enrichi.
 */
export function jsonLd(c: Contenu): string {
  return JSON.stringify({
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: "MonCap GPS",
    operatingSystem: "iOS",
    applicationCategory: "TravelApplication",
    description: c.description,
    inLanguage: c.code,
    author: { "@type": "Person", name: AUTEUR },
    offers: [
      {
        "@type": "Offer",
        name: "MonCap Plus",
        price: "3.99",
        priceCurrency: "EUR",
      },
      {
        "@type": "Offer",
        name: "MonCap Pro",
        price: "9.99",
        priceCurrency: "EUR",
      },
    ],
  });
}
