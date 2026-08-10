import { expect, test } from "bun:test";
import { ATOUTS, CHIFFRES, FRANCHISE, TECHNIQUES } from "./content";

/**
 * La pile est affichée sans numéro de version, à dessein : une version sur
 * une page vieillit toute seule et personne ne revient la corriger. La règle
 * est facile à réintroduire sans y penser — « React 19 » se tape tout seul —
 * d'où ce test plutôt qu'un commentaire.
 */
test("aucune version dans les technologies", () => {
  const chiffres = TECHNIQUES.flatMap((p) => p.outils).filter((o) =>
    /\d/.test(o),
  );
  expect(chiffres).toEqual([]);
});

test("aucun domaine ni outil en double", () => {
  const domaines = TECHNIQUES.map((p) => p.domaine);
  expect(new Set(domaines).size).toBe(domaines.length);

  const outils = TECHNIQUES.flatMap((p) => p.outils);
  expect(new Set(outils).size).toBe(outils.length);
});

/**
 * Les listes servent de clés de rendu (`key={a.titre}`, `key={f}`) : deux
 * entrées identiques passeraient la construction et casseraient le rendu en
 * silence.
 */
test("aucun doublon dans les listes rendues", () => {
  const titres = ATOUTS.map((a) => a.titre);
  expect(new Set(titres).size).toBe(titres.length);

  const legendes = CHIFFRES.map((c) => c.legende);
  expect(new Set(legendes).size).toBe(legendes.length);

  expect(new Set(FRANCHISE).size).toBe(FRANCHISE.length);
});

/** Aucune section vide : un titre suivi de rien se voit tout de suite. */
test("aucune section vide", () => {
  for (const p of TECHNIQUES) {
    expect(p.outils.length).toBeGreaterThan(0);
  }
  expect(ATOUTS.length).toBeGreaterThan(0);
  expect(CHIFFRES.length).toBeGreaterThan(0);
  expect(FRANCHISE.length).toBeGreaterThan(0);
});
