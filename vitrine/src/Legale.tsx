import type { Ressources } from "./App";
import { Pied } from "./App";
import type { Contenu, PageLegale } from "./langues";

/**
 * Une page légale — mentions ou confidentialité. Même squelette que la page
 * principale : le bandeau bleu, le corps sur papier, le pied complet. Le
 * texte vient de `langues.ts`, nu et sans lien : ce qui s'y dit doit rester
 * vrai sans entretien.
 */
export function Legale({
  c,
  page,
  res,
}: {
  c: Contenu;
  page: PageLegale;
  res: Ressources;
}) {
  return (
    <>
      <header className="hero legale">
        <div className="dedans">
          <div className="barre">
            <p className="marque">
              <img
                className="logo"
                src={res.logo}
                width="40"
                height="40"
                alt=""
              />
              MonCap GPS
            </p>
          </div>
          <h1>{page.titre}</h1>
          <p className="chapeau">
            <a className="retour" href={c.chemin}>
              {page.retour}
            </a>
          </p>
        </div>
      </header>

      <main>
        <div className="dedans juridique">
          {page.sections.map((s) => (
            <section key={s.titre}>
              <h2>{s.titre}</h2>
              {s.corps.map((p) => (
                <p key={p}>{p}</p>
              ))}
            </section>
          ))}
        </div>
      </main>

      <Pied c={c} res={res} />
    </>
  );
}
