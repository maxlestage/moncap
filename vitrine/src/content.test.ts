import { expect, test } from "bun:test";
import {
  ATOUTS,
  CHIFFRES,
  FRANCHISE,
  HORS_LIGNE,
  LIEUX,
  MAQUETTE_ITINERAIRES,
  MAQUETTE_REGIONS,
  TECHNIQUES,
} from "./content";

/**
 * En ajoutant les fonctions en ligne, j'avais retiré des choses du hors
 * ligne : le chiffre des pays d'Europe et la carte du téléchargement région
 * par région. Une réécriture emporte ce genre de contenu sans que rien ne
 * proteste — la page reste valide, elle dit simplement moins.
 *
 * Ces assertions ne défendent pas une formulation mais une **matière** : ce
 * que la page doit continuer d'expliquer. Les reformuler est libre ; les
 * faire disparaître ne l'est pas.
 */
test("le hors ligne garde ses trois piliers", () => {
  const texte = HORS_LIGNE.map((a) => `${a.titre} ${a.texte}`)
    .join(" ")
    .toLowerCase();
  // Calcul local, granularité du téléchargement, transfert en arrière-plan.
  expect(texte).toContain("iphone");
  expect(texte).toContain("monaco");
  expect(texte).toContain("arrière-plan");
  expect(HORS_LIGNE.length).toBeGreaterThanOrEqual(3);
});

test("le bandeau garde l'étendue européenne", () => {
  const legendes = CHIFFRES.map((c) => c.legende).join(" ");
  expect(legendes).toContain("pays d'Europe");
  expect(legendes).toContain("régions");
});

/** Les poids affichés viennent des Releases : une unité doit y figurer. */
test("les régions de la maquette portent un poids lisible", () => {
  for (const r of MAQUETTE_REGIONS) {
    expect(r.poids).toMatch(/^\d+(,\d+)? (Mo|Go)$/);
  }
  // Au moins un téléchargement en cours, sinon la jauge ne se voit jamais.
  expect(MAQUETTE_REGIONS.some((r) => r.avancement !== undefined)).toBe(true);
});

/**
 * Le bandeau annonce « 19 catégories » et la maquette les montre toutes :
 * les deux viennent du même endroit, mais rien n'oblige le chiffre à suivre
 * la liste. Ajouter une catégorie sans toucher au compteur donnerait une
 * page qui se contredit d'un écran à l'autre.
 */
test("le chiffre annoncé suit la liste des lieux", () => {
  const annonce = CHIFFRES.find((c) => c.legende.includes("catégories"));
  expect(annonce?.valeur).toBe(String(LIEUX.length));
});

/** Une maquette d'un seul itinéraire ne montrerait pas de comparaison. */
test("la maquette compare plusieurs itinéraires", () => {
  expect(MAQUETTE_ITINERAIRES.length).toBeGreaterThanOrEqual(2);
  const etiquettes = MAQUETTE_ITINERAIRES.map((r) => r.etiquette);
  expect(new Set(etiquettes).size).toBe(etiquettes.length);
  const teintes = MAQUETTE_ITINERAIRES.map((r) => r.teinte);
  expect(new Set(teintes).size).toBe(teintes.length);
});

/** Les emoji et libellés servent de clés de rendu et d'identité visuelle. */
test("aucun doublon parmi les lieux", () => {
  const noms = LIEUX.map((l) => l.nom);
  expect(new Set(noms).size).toBe(noms.length);
  const emojis = LIEUX.map((l) => l.emoji);
  expect(new Set(emojis).size).toBe(emojis.length);
});

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
