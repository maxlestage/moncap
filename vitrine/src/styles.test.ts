import { expect, test } from "bun:test";

/**
 * Une couleur mal tapée ne fait échouer aucune construction : le navigateur
 * ignore la déclaration en silence et sert la page avec un trait manquant.
 * J'ai écrit `#2b3burlywood`, `#142banchor` et `#1b3straw` dans la même
 * journée — un nom de couleur CSS venu se coller à un code hexadécimal — et
 * seule la relecture les a rattrapés. Ce test les rattrape sans relecture.
 */
const FEUILLE = await Bun.file(new URL("./styles.css", import.meta.url)).text();

test("aucune couleur hexadécimale invalide", () => {
  // Seules les déclarations sont examinées — `propriété: valeur;`. Un `#`
  // dans un sélecteur (`:root:has(#th-auto:checked)`) est un identifiant,
  // pas une couleur, et le confondre ferait échouer le test à tort.
  const declarations = FEUILLE.match(/[-a-z]+\s*:\s*[^;{}]+;/g) ?? [];
  const suspectes = declarations
    .flatMap((d) => [...d.matchAll(/#[0-9a-zA-Z]+/g)].map((m) => m[0]))
    .filter(
      (c) => !/^#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(c),
    );
  expect(suspectes).toEqual([]);
});

/**
 * Les quatre thèmes reposent sur `:has()` : si un sélecteur perd son
 * identifiant, le thème cesse d'exister sans que rien ne le signale — la
 * page reste valide, le bouton ne fait simplement plus rien.
 */
test("les quatre thèmes sont câblés", () => {
  for (const t of ["auto", "clair", "sombre", "marine"]) {
    expect(FEUILLE, t).toContain(`#th-${t}:checked`);
  }
});

/**
 * Chaque thème doit redéfinir les jetons qui portent le texte et le fond.
 * Un thème qui n'en changerait qu'un donnerait un texte illisible plutôt
 * qu'un thème.
 */
test("les thèmes explicites redéfinissent fond et texte", () => {
  for (const t of ["sombre", "marine"]) {
    const bloc = FEUILLE.split(`#th-${t}:checked)`)[1]?.split("}")[0] ?? "";
    for (const jeton of ["--papier", "--texte", "--doux", "--accent"]) {
      expect(bloc, `${t} / ${jeton}`).toContain(jeton);
    }
  }
});
