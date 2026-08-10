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
 * La bande blanche sous la barre d'état de Safari venait de là, et la
 * correction est trop discrète pour survivre seule : Safari teinte sa barre
 * d'après le fond du **corps**, pas d'après `html` ni d'après ce qui est
 * peint en haut de la page. Remettre `--papier` sur `body` — le geste le plus
 * naturel du monde — ramènerait la bande blanche sans qu'aucune capture ne
 * le montre, puisque la barre du navigateur ne se photographie pas.
 */
test("le corps porte le bleu de marque, le papier est sur le contenu", () => {
  const corps = FEUILLE.match(/\nbody \{[^}]*\}/)?.[0] ?? "";
  expect(corps).toContain("background: var(--marine)");
  expect(corps).not.toContain("--papier");

  const contenu = FEUILLE.match(/\nmain \{[^}]*\}/)?.[0] ?? "";
  expect(contenu).toContain("background: var(--papier)");
});

/**
 * Le bandeau chiffré doit tenir deux colonnes même sur une page agrandie, où
 * la largeur utile tombe vers 220 px. En une seule colonne, cinq nombres
 * étirent le bandeau sur plus de trois cent cinquante pixels de haut — c'est
 * l'état que montrait une capture prise avec le zoom de Safari, et qu'une
 * capture à 390 px ne révèle jamais.
 */
test("deux chiffres de front tiennent sur une page agrandie", () => {
  const min = FEUILLE.match(/minmax\((\d+(?:\.\d+)?)rem, 1fr\)/)?.[1];
  const gap = FEUILLE.match(/\.chiffres \{[^}]*gap: (\d+(?:\.\d+)?)rem/)?.[1];
  expect(min, "largeur minimale de colonne introuvable").toBeDefined();
  expect(gap, "gouttière introuvable").toBeDefined();

  const px = (rem: string) => Number(rem) * 16;
  expect(2 * px(min!) + px(gap!)).toBeLessThanOrEqual(220);
});

/**
 * Le corps étant bleu marine, toute marge extérieure entre le papier et le
 * pied s'y voit : une bande sombre, plus foncée que le papier au-dessus comme
 * que le dégradé en dessous. Le souffle avant le pied doit donc rester une
 * marge intérieure.
 */
test("rien ne sépare le contenu du pied", () => {
  const pied = FEUILLE.match(/\nfooter \{[^}]*\}/)?.[0] ?? "";
  expect(pied).not.toContain("margin-top");
  expect(pied).not.toContain("margin:");

  // Le souffle existe quand même : il est simplement dans le contenu.
  const contenu = FEUILLE.match(/\nmain > \.dedans \{[^}]*\}/)?.[0] ?? "";
  expect(contenu).toContain("padding-bottom");
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
