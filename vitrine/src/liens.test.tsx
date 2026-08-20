import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { App } from "./App";
import { Legale } from "./Legale";
import { DEPOT, SOURCES } from "./content";
import { LANGUES } from "./langues";

/**
 * Les adresses expédiées au visiteur, relues dans le rendu.
 *
 * La page a longtemps porté deux liens vers le dépôt — « Voir le projet » et
 * « Code source ». Le dépôt est privé : un visiteur non connecté recevait un
 * 404. Rien ne le signalait, ni la construction, ni une capture, ni un test :
 * une adresse morte se rend exactement comme une adresse valide.
 *
 * Ce test lit donc les `href` du rendu et n'accepte qu'une liste connue. Il
 * vaut pour tout lien futur, pas seulement pour ceux-là. La liste est
 * calculée : les pages internes viennent de `langues.ts` (le serveur doit
 * servir chacune — voir la vitrine dans `src/main.rs`), les externes de
 * `SOURCES`.
 */

const AUTORISEES = new Set<string>([
  "/app",
  DEPOT,
  ...LANGUES.map((l) => l.chemin),
  ...LANGUES.map((l) => l.mentions.chemin),
  ...LANGUES.map((l) => l.confidentialite.chemin),
  // Les ancres du pied visent la page principale de la langue courante.
  ...LANGUES.flatMap((l) => l.pied.ancres.map((a) => `${l.chemin}#${a.id}`)),
  ...SOURCES.map((s) => s.url),
]);

function adresses(html: string): string[] {
  return [...html.matchAll(/href="([^"]*)"/g)].map((m) => m[1]!);
}

/** Toutes les pages rendues, principales et légales, dans chaque langue. */
function rendus(): { code: string; html: string }[] {
  const res = { logo: "/vitrine/logo.webp" };
  return LANGUES.flatMap((c) => [
    { code: c.code, html: renderToStaticMarkup(<App c={c} res={res} />) },
    {
      code: `${c.code} mentions`,
      html: renderToStaticMarkup(<Legale c={c} page={c.mentions} res={res} />),
    },
    {
      code: `${c.code} confidentialité`,
      html: renderToStaticMarkup(
        <Legale c={c} page={c.confidentialite} res={res} />,
      ),
    },
  ]);
}

test("les pages ne pointent que vers des adresses que ce serveur sert", () => {
  for (const { code, html } of rendus()) {
    for (const a of adresses(html)) {
      // Les ressources construites portent une empreinte dans leur nom : on
      // ne peut pas les énumérer, seulement vérifier leur préfixe.
      if (a.startsWith("/vitrine/")) continue;
      expect(AUTORISEES.has(a), `${code} : adresse inattendue « ${a} »`).toBe(
        true,
      );
    }
  }
});

/**
 * Le dépôt est public depuis août 2026 : le lien « Code source » a le droit
 * de revivre — mais vers LE dépôt, exactement. La page a longtemps porté
 * des liens vers un dépôt privé, donc un 404 pour tout visiteur ; ce test
 * garde la mémoire de l'épisode en n'acceptant que l'adresse déclarée dans
 * `DEPOT`.
 */
test("les seuls liens github visent le dépôt public déclaré", () => {
  for (const { code, html } of rendus()) {
    for (const a of adresses(html)) {
      if (a.includes("github.com")) {
        expect(a, code).toBe(DEPOT);
      }
    }
  }
});

/**
 * Une action unique doit rester une action : deux boutons blancs côte à côte
 * dans le bandeau ne diraient plus lequel est le principal.
 */
test("une seule action par bloc, et c'est un bouton", () => {
  const html = renderToStaticMarkup(
    <App c={LANGUES[0]!} res={{ logo: "/vitrine/logo.webp" }} />,
  );
  const boutons = [...html.matchAll(/class="bouton"/g)];
  // Un dans le bandeau, un dans le pied.
  expect(boutons.length).toBe(2);
  // Et ils mènent au même endroit : l'application.
  const versApp = [...html.matchAll(/href="\/app"/g)];
  expect(versApp.length).toBe(2);
});
