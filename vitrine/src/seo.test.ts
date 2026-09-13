import { expect, test } from "bun:test";
import { LANGUES } from "./langues";
import {
  groupes,
  jsonLd,
  jsonLdLegale,
  OG_LOCALES,
  PUBLIC,
  pagesPubliques,
  robots,
  sitemap,
  verificationGoogle,
} from "./seo";

/**
 * Le référencement se teste comme le contenu : ce que Google lira est
 * exactement ce que ces fonctions rendent — la construction les appelle
 * telles quelles.
 */

test("le plan du site liste les neuf pages, en absolu", () => {
  const plan = sitemap();
  const pages = pagesPubliques();
  expect(pages.length).toBe(9);
  for (const p of pages) {
    expect(plan).toContain(`<loc>${PUBLIC}${p.chemin}</loc>`);
  }
  // Chaque groupe traduit — principales ET légales — porte ses alternates :
  // une par langue + x-default (la française), toutes absolues — un
  // hreflang relatif est invalide pour les moteurs.
  for (const groupe of groupes()) {
    for (const p of groupe) {
      expect(plan).toContain(
        `hreflang="${p.code}" href="${PUBLIC}${p.chemin}"`,
      );
    }
    expect(plan).toContain(
      `hreflang="x-default" href="${PUBLIC}${groupe[0]?.chemin}"`,
    );
  }
  expect(plan).not.toContain('href="/');
});

test("les groupes traduits couvrent toutes les pages publiques", () => {
  const parGroupes = groupes().flatMap((g) => g.map((p) => p.chemin));
  const publiques = pagesPubliques().map((p) => p.chemin);
  expect(new Set(parGroupes)).toEqual(new Set(publiques));
  // Trois langues par groupe : un groupe bancal casserait hreflang.
  for (const g of groupes()) {
    expect(g.map((p) => p.code)).toEqual(["fr", "es", "en"]);
  }
});

test("les robots protègent le privé et trouvent le plan", () => {
  const r = robots();
  for (const chemin of ["/app", "/t", "/live", "/reinitialisation", "/auth/"]) {
    expect(r).toContain(`Disallow: ${chemin}`);
  }
  expect(r).toContain(`Sitemap: ${PUBLIC}/sitemap.xml`);
  // Aucune page publique n'est bloquée par un préfixe interdit.
  const interdits = r
    .split("\n")
    .filter((l) => l.startsWith("Disallow: "))
    .map((l) => l.slice("Disallow: ".length));
  for (const p of pagesPubliques()) {
    if (p.chemin === "/") continue;
    for (const i of interdits) {
      expect(p.chemin.startsWith(i), `${p.chemin} bloqué par ${i}`).toBe(false);
    }
  }
});

test("les données structurées disent l'app et ses deux prix", () => {
  for (const l of LANGUES) {
    const d = JSON.parse(jsonLd(l));
    expect(d["@type"]).toBe("SoftwareApplication");
    expect(d.operatingSystem).toBe("iOS");
    expect(d.description).toBe(l.description);
    expect(d.inLanguage).toBe(l.code);
    // Deux offres payantes, aucune à 0 € — rien de gratuit passé l'essai.
    const prix = d.offers.map((o: { price: string }) => o.price);
    expect(prix).toEqual(["3.99", "9.99"]);
    // L'adresse absolue de la page, et ce que l'app sait faire — mot pour
    // mot les cartes de tarifs.
    expect(d.url).toBe(`${PUBLIC}${l.chemin}`);
    expect(d.featureList.length).toBeGreaterThanOrEqual(6);
    // La locale Open Graph existe pour chaque langue servie.
    expect(OG_LOCALES[l.code]).toMatch(/^[a-z]{2}_[A-Z]{2}$/);
  }
});

test("les pages légales portent un fil d'Ariane absolu", () => {
  for (const l of LANGUES) {
    for (const page of [l.mentions, l.confidentialite]) {
      const d = JSON.parse(jsonLdLegale(l, page));
      expect(d["@type"]).toBe("BreadcrumbList");
      expect(d.itemListElement.length).toBe(2);
      expect(d.itemListElement[0].item).toBe(`${PUBLIC}${l.chemin}`);
      expect(d.itemListElement[1].item).toBe(`${PUBLIC}${page.chemin}`);
      expect(d.itemListElement[1].name).toBe(page.titre);
    }
  }
});

test("l'adresse publique est absolue et sans barre de fin", () => {
  // `${PUBLIC}${chemin}` est écrit partout : une barre de fin donnerait
  // « //es », que les moteurs comptent comme une autre page.
  expect(PUBLIC).toMatch(/^https:\/\//);
  expect(PUBLIC.endsWith("/")).toBe(false);
});

test("la balise Google n'apparaît que si le jeton existe", () => {
  const avant = process.env.GOOGLE_SITE_VERIFICATION;
  try {
    // Sans jeton : rien. Une balise vide se lit comme une vérification
    // ratée, c'est pire que pas de balise du tout.
    delete process.env.GOOGLE_SITE_VERIFICATION;
    expect(verificationGoogle()).toEqual([]);
    process.env.GOOGLE_SITE_VERIFICATION = "   ";
    expect(verificationGoogle()).toEqual([]);

    process.env.GOOGLE_SITE_VERIFICATION = "jeton-de-test";
    expect(verificationGoogle()).toEqual([
      '<meta name="google-site-verification" content="jeton-de-test" />',
    ]);
  } finally {
    if (avant === undefined) delete process.env.GOOGLE_SITE_VERIFICATION;
    else process.env.GOOGLE_SITE_VERIFICATION = avant;
  }
});
