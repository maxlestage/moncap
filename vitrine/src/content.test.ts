import { expect, test } from "bun:test";
import {
  ITINERAIRES_MAX,
  LIEUX,
  MAQUETTE_ITINERAIRES,
  MAQUETTE_REGIONS,
  OUTILS,
} from "./content";
import { LANGUES } from "./langues";

/**
 * Ces tests ne défendent pas une formulation mais une **matière** : ce que la
 * page doit continuer de dire, et ce qu'elle ne doit pas se mettre à dire.
 * Reformuler reste libre ; supprimer, se contredire ou vieillir ne l'est pas.
 */

/** Replie une chaîne pour comparer à travers les langues. */
function sansAccents(s: string): string {
  return s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}

// --- Ce que la page dit de factuel ---------------------------------------

/**
 * La pile est affichée sans numéro de version, à dessein : une version sur
 * une page vieillit toute seule et personne ne revient la corriger. La règle
 * est facile à réintroduire sans y penser — « React 19 » se tape tout seul —
 * d'où ce test plutôt qu'un commentaire.
 */
test("aucune version dans les technologies", () => {
  const avecChiffre = OUTILS.flat().filter((o) => /\d/.test(o));
  expect(avecChiffre).toEqual([]);
});

test("aucun doublon parmi les outils ni les lieux", () => {
  const outils = OUTILS.flat();
  expect(new Set(outils).size).toBe(outils.length);

  const noms = LIEUX.map((l) => l.nom);
  expect(new Set(noms).size).toBe(noms.length);
  const emojis = LIEUX.map((l) => l.emoji);
  expect(new Set(emojis).size).toBe(emojis.length);
});

/** Une maquette d'un seul itinéraire ne montrerait pas de comparaison. */
test("la maquette compare plusieurs itinéraires", () => {
  expect(MAQUETTE_ITINERAIRES.length).toBeGreaterThanOrEqual(2);
  const etiquettes = MAQUETTE_ITINERAIRES.map((r) => r.etiquette);
  expect(new Set(etiquettes).size).toBe(etiquettes.length);
  const teintes = MAQUETTE_ITINERAIRES.map((r) => r.teinte);
  expect(new Set(teintes).size).toBe(teintes.length);
  // Elle ne peut pas montrer plus d'itinéraires que l'app n'en propose.
  expect(MAQUETTE_ITINERAIRES.length).toBeLessThanOrEqual(ITINERAIRES_MAX);
});

/** Les poids affichés viennent des Releases : une unité doit y figurer. */
test("les régions de la maquette portent un poids lisible", () => {
  for (const r of MAQUETTE_REGIONS) {
    expect(r.poids).toMatch(/^\d+(,\d+)? (Mo|Go)$/);
  }
  // Au moins un téléchargement en cours, sinon la jauge ne se voit jamais.
  expect(MAQUETTE_REGIONS.some((r) => r.avancement !== undefined)).toBe(true);
});

// --- Les trois langues ---------------------------------------------------

test("les trois langues sont présentes, à des adresses distinctes", () => {
  expect(LANGUES.map((l) => l.code)).toEqual(["fr", "es", "en"]);
  const chemins = LANGUES.map((l) => l.chemin);
  expect(new Set(chemins).size).toBe(chemins.length);
  // Le français occupe la racine : c'est la langue du projet.
  expect(LANGUES[0]?.chemin).toBe("/");
});

/**
 * Une traduction bâclée se repère à ce qu'elle laisse tomber. Le type impose
 * la présence des champs, pas leur contenu : une chaîne vide compilerait.
 */
test("aucun texte vide dans aucune langue", () => {
  for (const l of LANGUES) {
    const textes = [
      l.titre,
      l.description,
      ...l.h1,
      l.chapeau,
      l.seConnecter,
      l.voirProjet,
      l.comparateur.titre,
      l.comparateur.intro,
      l.comparateur.apres,
      l.guidage.titre,
      l.guidage.intro,
      l.lieux.titre,
      l.lieux.intro,
      l.horsLigne.titre,
      l.horsLigne.intro,
      l.preuve.titre,
      l.preuve.texte,
      l.preuve.note,
      l.franchiseTitre,
      l.signature,
      l.credits,
      ...l.franchise,
      ...l.theme.noms,
      ...Object.values(l.legendes),
    ];
    for (const t of textes) {
      expect(t.trim().length, `${l.code} : texte vide`).toBeGreaterThan(0);
    }
  }
});

test("chaque langue garde la même matière", () => {
  for (const l of LANGUES) {
    // Le gabarit de la preuve doit ménager la place de la mesure.
    expect(l.preuve.texte, `${l.code}`).toContain("{mesure}");
    expect(l.preuve.mesure).toMatch(/222[,.]5 km/);

    // Le hors ligne garde ses trois piliers : calcul local, granularité du
    // téléchargement, transfert en arrière-plan. C'est ce que la réécriture
    // précédente avait emporté sans que rien ne proteste.
    expect(l.horsLigneCartes.length).toBeGreaterThanOrEqual(3);
    // Sans pli ni accents : « Mónaco » en espagnol est le même argument.
    const hl = sansAccents(l.horsLigneCartes.map((a) => a.texte).join(" "));
    expect(hl).toContain("iphone");
    expect(hl).toContain("monaco");
    // Le plus petit paquet et le plus gros : c'est la granularité qui parle.
    // Le séparateur décimal suit la langue — « 1,1 Mo » mais « 1.1 MB ».
    expect(hl, `${l.code}`).toMatch(/1[.,]1/);
    expect(hl).toContain("320");

    // Le bandeau garde l'étendue européenne et annonce le vrai plafond.
    const valeurs = l.chiffres.map((n) => n.valeur);
    expect(valeurs).toContain(String(ITINERAIRES_MAX));
    expect(valeurs).toContain("206");
    expect(valeurs).toContain("49");

    // Le plafond doit se lire comme un plafond. « 15 itinéraires » seul
    // laisserait croire qu'il y en a toujours quinze : la légende doit dire
    // que c'est un maximum, et l'intro du comparateur reprendre le nombre.
    const plafond = l.chiffres.find((n) => n.valeur === String(ITINERAIRES_MAX));
    expect(plafond?.legende, `${l.code}`).toMatch(
      /maximum|máximo|at most|jusqu|hasta|up to/i,
    );
    expect(l.comparateur.intro, `${l.code}`).toContain(
      String(ITINERAIRES_MAX),
    );

    // Le chiffre des catégories doit suivre la liste, pas la contredire.
    const cat = l.chiffres.find((n) => /cat[ée]g/i.test(n.legende));
    expect(cat?.valeur, `${l.code}`).toBe(String(LIEUX.length));

    // Autant de domaines que de groupes d'outils, sinon une colonne se vide.
    expect(l.pile.length).toBe(OUTILS.length);
    for (const p of l.pile) expect(p.outils.length).toBeGreaterThan(0);

    // Quatre thèmes proposés, quatre libellés.
    expect(l.theme.noms.length).toBe(4);
  }
});

/**
 * Hors français, la page doit dire que les écrans dépeints sont en français.
 * Sans ce rappel, les maquettes laissent croire à une application traduite.
 */
test("les langues étrangères signalent que l'app est en français", () => {
  for (const l of LANGUES) {
    if (l.code === "fr") continue;
    expect(l.legendes.enFrancais, `${l.code}`).toBeTruthy();
  }
});
