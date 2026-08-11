import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { App } from "./App";
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
 * vaut pour tout lien futur, pas seulement pour ceux-là.
 */

const AUTORISEES = new Set(["/", "/es", "/en", "/app"]);

function adresses(html: string): string[] {
  return [...html.matchAll(/href="([^"]*)"/g)].map((m) => m[1]!);
}

test("la page ne pointe que vers des adresses que ce serveur sert", () => {
  for (const c of LANGUES) {
    const html = renderToStaticMarkup(
      <App c={c} res={{ logo: "/vitrine/logo.webp" }} />,
    );
    for (const a of adresses(html)) {
      // Les ressources construites portent une empreinte dans leur nom : on
      // ne peut pas les énumérer, seulement vérifier leur préfixe.
      if (a.startsWith("/vitrine/")) continue;
      expect(AUTORISEES.has(a), `${c.code} : adresse inattendue « ${a} »`).toBe(
        true,
      );
    }
  }
});

test("aucun lien vers le dépôt privé", () => {
  for (const c of LANGUES) {
    const html = renderToStaticMarkup(
      <App c={c} res={{ logo: "/vitrine/logo.webp" }} />,
    );
    expect(html, c.code).not.toContain("github.com");
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
