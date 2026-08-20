import { expect, test } from "bun:test";
import { LANGUES } from "./langues";
import { jsonLd, OG_LOCALES, PUBLIC, pagesPubliques, robots, sitemap } from "./seo";

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
  // Les alternates des pages principales : une par langue + x-default,
  // toutes absolues — un hreflang relatif est invalide pour les moteurs.
  for (const l of LANGUES) {
    expect(plan).toContain(`hreflang="${l.code}" href="${PUBLIC}${l.chemin}"`);
  }
  expect(plan).toContain('hreflang="x-default"');
  expect(plan).not.toContain('href="/');
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
    // La locale Open Graph existe pour chaque langue servie.
    expect(OG_LOCALES[l.code]).toMatch(/^[a-z]{2}_[A-Z]{2}$/);
  }
});
