/**
 * Le référencement, en un module : l'adresse canonique, le plan du site, les
 * consignes aux robots et les données structurées.
 *
 * Un module partagé plutôt que du texte dans build.tsx : les tests importent
 * les mêmes fonctions que la construction — ce que Google lira est ce que les
 * tests ont vu.
 */
import { AUTEUR } from "./content";
import type { Contenu, Langue, PageLegale } from "./langues";
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
 * Les pages publiques à référencer : les trois pages principales et les six
 * pages légales.
 */
export function pagesPubliques(): { chemin: string; principale: boolean }[] {
  return LANGUES.flatMap((l) => [
    { chemin: l.chemin, principale: true },
    { chemin: l.mentions.chemin, principale: false },
    { chemin: l.confidentialite.chemin, principale: false },
  ]);
}

/**
 * Les trois groupes de pages traduites entre elles : la principale, les
 * mentions, la confidentialité. Chaque page d'un groupe est l'alternate des
 * autres — les adresses diffèrent, la matière est la même, et c'est
 * exactement ce que hreflang décrit. La version française fait x-default :
 * c'est la langue du projet.
 */
export function groupes(): { code: Langue; chemin: string }[][] {
  return [
    LANGUES.map((l) => ({ code: l.code, chemin: l.chemin })),
    LANGUES.map((l) => ({ code: l.code, chemin: l.mentions.chemin })),
    LANGUES.map((l) => ({ code: l.code, chemin: l.confidentialite.chemin })),
  ];
}

/**
 * Le plan du site. Chaque page — principales ET légales — porte les
 * alternates hreflang de son groupe, en adresses **absolues** : un hreflang
 * relatif est invalide pour les moteurs, et une page légale sans alternates
 * laissait Google deviner que /es/privacidad traduit /confidentialite.
 */
export function sitemap(): string {
  const urls = groupes()
    .flatMap((groupe) => {
      const alternates = [
        ...groupe.map(
          (p) =>
            `    <xhtml:link rel="alternate" hreflang="${p.code}" href="${PUBLIC}${p.chemin}"/>`,
        ),
        `    <xhtml:link rel="alternate" hreflang="x-default" href="${PUBLIC}${groupe[0]!.chemin}"/>`,
      ].join("\n");
      return groupe.map(
        (p) => `  <url>\n    <loc>${PUBLIC}${p.chemin}</loc>\n${alternates}\n  </url>`,
      );
    })
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
    url: `${PUBLIC}${c.chemin}`,
    author: { "@type": "Person", name: AUTEUR },
    // Ce que l'app sait faire, mot pour mot les cartes de tarifs : le
    // résultat enrichi raconte la même chose que la page.
    featureList: [...c.tarifs.plus.points, ...c.tarifs.pro.points],
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

/**
 * Le fil d'Ariane d'une page légale : Accueil → la page. Deux niveaux, pas
 * de théâtre — c'est ce que le résultat de recherche affiche sous le titre.
 */
export function jsonLdLegale(c: Contenu, p: PageLegale): string {
  return JSON.stringify({
    "@context": "https://schema.org",
    "@type": "BreadcrumbList",
    itemListElement: [
      {
        "@type": "ListItem",
        position: 1,
        name: "MonCap GPS",
        item: `${PUBLIC}${c.chemin}`,
      },
      {
        "@type": "ListItem",
        position: 2,
        name: p.titre,
        item: `${PUBLIC}${p.chemin}`,
      },
    ],
  });
}
