import {
  AUTEUR,
  CONNEXION,
  LIEUX,
  MAQUETTE_ITINERAIRES,
  MAQUETTE_REGIONS,
  MODES,
} from "./content";
import type { Contenu } from "./langues";
import { LANGUES } from "./langues";
import { app, appFormat } from "./traduire";

/**
 * Vitrine d'une page, pensée pour le pouce d'abord.
 *
 * Rien de scripté : une page qui ne fait que présenter n'a pas besoin d'état,
 * et le lecteur n'a pas à attendre du JavaScript pour lire. Même le choix du
 * thème passe par des boutons radio et `:has()`, sans une ligne de script.
 *
 * Les maquettes sont du HTML et du CSS, pas des images : elles restent nettes
 * à toutes les résolutions, suivent le thème, se lisent au lecteur d'écran,
 * et ne coûtent pas un octet de téléchargement supplémentaire. Ce sont des
 * dessins de l'interface, marqués comme tels.
 */

export type Ressources = { readonly logo: string };

/** Les quatre thèmes, dans l'ordre des libellés fournis par la langue. */
const THEMES = ["auto", "clair", "sombre", "marine"] as const;

/** L'écran de comparaison : le cœur de l'app quand le réseau est là. */
function MaquetteComparateur({ c }: { c: Contenu }) {
  return (
    <figure className="maquette">
      <div className="ecran">
        <div className="modes" aria-hidden="true">
          {MODES.map((m, i) => (
            <span key={m} className={i === MODES.length - 1 ? "actif" : ""}>
              {app(c.code, m)}
            </span>
          ))}
        </div>

        <ul className="routes">
          {MAQUETTE_ITINERAIRES.map((r) => (
            <li key={r.etiquette}>
              <span className={`pastille ${r.teinte}`} aria-hidden="true" />
              <div>
                <p className={`etiquette ${r.teinte}`}>
                  {app(c.code, r.etiquette)}
                </p>
                <p className="chiffres-route">
                  {appFormat(
                    c.code,
                    "%.0f min · %.1f km · %d virages · arr. %@",
                    r.trajet,
                  )}
                </p>
                {r.denivele && (
                  <p className="detail">
                    {appFormat(
                      c.code,
                      "↗ %.0f m de côte · ↘ %.0f m de descente",
                      r.denivele,
                    )}
                  </p>
                )}
                <p className="detail">
                  {appFormat(
                    c.code,
                    r.cout.length === 1
                      ? "⛽ ~%.2f € · sans péage"
                      : "⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €",
                    r.cout,
                  )}
                </p>
                {r.avertissement && (
                  <p className="alerte">
                    {appFormat(c.code, "⚠️ Forte côte (%.0f m)", r.avertissement)}
                  </p>
                )}
              </div>
            </li>
          ))}
        </ul>

        {/* Le comparateur en propose jusqu'à quinze : trois lignes seules
            laisseraient croire à trois. */}
        <p className="reste" aria-hidden="true">
          ⌄ {c.reste}
        </p>
        <p className="demarrer" aria-hidden="true">
          {app(c.code, "Démarrer")}
        </p>
      </div>
      <Legende texte={c.legendes.itineraires} />
    </figure>
  );
}

/** Le bandeau de guidage, avec les pastilles vitesse et météo. */
function MaquetteGuidage({ c }: { c: Contenu }) {
  return (
    <figure className="maquette">
      <div className="ecran">
        <div className="guidage" aria-hidden="true">
          <span className="fleche">↱</span>
          <div>
            <p className="distance">300 m</p>
            {/* Le bandeau ne montre pas une phrase mais un mot, lisible d'un
                coup d'œil au volant, et sa précision en dessous. */}
            <p className="consigne">
              {appFormat(c.code, "Sortie %@", ["12"])}
            </p>
            <p className="precision">{app(c.code, "sortie")}</p>
          </div>
        </div>

        <div className="pastilles" aria-hidden="true">
          <span className="meteo">🌤️ 21° {app(c.code, "Éclaircies")}</span>
          <span className="vitesse">
            <strong>78</strong>
            <small>km/h</small>
            <em>90</em>
          </span>
        </div>
      </div>
      <Legende texte={c.legendes.guidage} />
    </figure>
  );
}

/** Le bandeau des catégories de lieux. */
function MaquetteLieux({ c }: { c: Contenu }) {
  return (
    <figure className="maquette">
      <div className="ecran">
        <ul className="lieux">
          {LIEUX.map((l, i) => (
            <li key={l.nom} className={i === 0 ? "actif" : ""}>
              <span aria-hidden="true">{l.emoji}</span> {app(c.code, l.nom)}
            </li>
          ))}
        </ul>
      </div>
      <Legende texte={c.legendes.lieux} />
    </figure>
  );
}

/** L'écran des régions installées, avec un téléchargement en cours. */
function MaquetteRegions({ c }: { c: Contenu }) {
  return (
    <figure className="maquette">
      <div className="ecran">
        <ul className="regions">
          {MAQUETTE_REGIONS.map((r) => (
            <li key={r.nom}>
              <div className="ligne">
                <span className="nom">{r.nom}</span>
                <span className="poids">{r.poids}</span>
              </div>
              {r.avancement === undefined ? (
                <p className="pret" aria-hidden="true">
                  {c.maquette.installee}
                </p>
              ) : (
                <>
                  <div className="jauge" aria-hidden="true">
                    <span style={{ width: `${r.avancement}%` }} />
                  </div>
                  <p className="encours">{r.avancement} %</p>
                </>
              )}
            </li>
          ))}
        </ul>
      </div>
      <Legende texte={c.legendes.regions} />
    </figure>
  );
}

/** La légende d'une maquette. */
function Legende({ texte }: { texte: string }) {
  return <figcaption>{texte}</figcaption>;
}

/**
 * Choix du thème, sans script : quatre boutons radio que la feuille de style
 * lit avec `:has()`. Le choix ne survit pas à un changement de page — il n'y
 * a rien pour le mémoriser sans JavaScript, et le dire vaut mieux que de
 * laisser croire le contraire.
 */
function Themes({ c }: { c: Contenu }) {
  return (
    <fieldset className="reglages">
      <legend>{c.theme.libelle}</legend>
      {THEMES.map((t, i) => (
        <span key={t}>
          <input
            type="radio"
            name="theme"
            id={`th-${t}`}
            defaultChecked={t === "auto"}
          />
          <label htmlFor={`th-${t}`}>{c.theme.noms[i]}</label>
        </span>
      ))}
    </fieldset>
  );
}

/** Les trois langues, la courante marquée comme telle. */
function Langues({ c }: { c: Contenu }) {
  return (
    <nav className="langues" aria-label={c.langueLibelle}>
      {LANGUES.map((l) =>
        l.code === c.code ? (
          <span key={l.code} aria-current="true">
            {l.nom}
          </span>
        ) : (
          <a key={l.code} href={l.chemin} hrefLang={l.code} lang={l.code}>
            {l.nom}
          </a>
        ),
      )}
    </nav>
  );
}

export function App({ c, res }: { c: Contenu; res: Ressources }) {
  // La mesure est mise en valeur au milieu de la phrase : le gabarit évite
  // de couper le texte en trois morceaux traduits séparément.
  const [avant, apres] = c.preuve.texte.split("{mesure}");

  return (
    <>
      <header className="hero">
        {/* Le fond du bandeau va d'un bord à l'autre, mais son texte doit
            s'aligner sur celui du corps : d'où ce conteneur, de la même
            largeur que `main`. */}
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
            <Langues c={c} />
          </div>

          <h1>
            {c.h1[0]}
            <br />
            <em>{c.h1[1]}</em>
          </h1>
          <p className="chapeau">{c.chapeau}</p>

          <div className="actions">
            <a className="bouton" href={CONNEXION}>
              {c.seConnecter}
            </a>
          </div>

          <dl className="chiffres">
            {c.chiffres.map((n) => (
              <div key={n.legende}>
                <dt>{n.valeur}</dt>
                <dd>{n.legende}</dd>
              </div>
            ))}
          </dl>

          <Themes c={c} />
        </div>
      </header>

      <main>
        <div className="dedans">
          <section aria-labelledby="comparateur-titre">
            <h2 id="comparateur-titre">{c.comparateur.titre}</h2>
            <p className="intro">{c.comparateur.intro}</p>
            <MaquetteComparateur c={c} />
            <p className="intro">{c.comparateur.apres}</p>
          </section>

          <section aria-labelledby="guidage-titre">
            <h2 id="guidage-titre">{c.guidage.titre}</h2>
            <p className="intro">{c.guidage.intro}</p>
            <MaquetteGuidage c={c} />
          </section>

          <section aria-labelledby="lieux-titre">
            <h2 id="lieux-titre">{c.lieux.titre}</h2>
            <p className="intro">{c.lieux.intro}</p>
            <MaquetteLieux c={c} />
          </section>

          <section aria-labelledby="atouts-titre">
            <h2 id="atouts-titre">{c.atoutsTitre}</h2>
            <ul className="cartes">
              {c.atouts.map((a) => (
                <li key={a.titre}>
                  <span className="emoji" aria-hidden="true">
                    {a.emoji}
                  </span>
                  <h3>{a.titre}</h3>
                  <p>{a.texte}</p>
                </li>
              ))}
            </ul>
          </section>

          <section aria-labelledby="hors-ligne-titre">
            <h2 id="hors-ligne-titre">{c.horsLigne.titre}</h2>
            <p className="intro">{c.horsLigne.intro}</p>
            <MaquetteRegions c={c} />
            <ul className="cartes">
              {c.horsLigneCartes.map((a) => (
                <li key={a.titre}>
                  <span className="emoji" aria-hidden="true">
                    {a.emoji}
                  </span>
                  <h3>{a.titre}</h3>
                  <p>{a.texte}</p>
                </li>
              ))}
            </ul>
          </section>

          <section className="preuve" aria-labelledby="preuve-titre">
            <h2 id="preuve-titre">{c.preuve.titre}</h2>
            <p>
              {avant}
              <strong>{c.preuve.mesure}</strong>
              {apres}
            </p>
            <p className="note">{c.preuve.note}</p>
          </section>

          <section aria-labelledby="pile-titre" className="pile">
            <h2 id="pile-titre">{c.pileTitre}</h2>
            <dl>
              {c.pile.map((p) => (
                <div key={p.domaine}>
                  <dt>{p.domaine}</dt>
                  <dd>
                    <ul>
                      {p.outils.map((o) => (
                        <li key={o}>{o}</li>
                      ))}
                    </ul>
                  </dd>
                </div>
              ))}
            </dl>
          </section>

          <section aria-labelledby="franchise-titre" className="franchise">
            <h2 id="franchise-titre">{c.franchiseTitre}</h2>
            <ul>
              {c.franchise.map((f) => (
                <li key={f}>{f}</li>
              ))}
            </ul>
          </section>
        </div>
      </main>

      {/* Le pied reprend le bleu du bandeau : la page se ferme comme elle
          s'ouvre, et le bas de l'écran cesse d'être une zone blanche sans
          rien. Il porte aussi le choix de langue, qu'on ne peut pas
          atteindre autrement après avoir lu jusqu'ici. */}
      <footer>
        <div className="dedans">
          <div className="pied-haut">
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
            <Langues c={c} />
          </div>

          <p className="signature">
            {c.signature} <strong>{AUTEUR}</strong>.
          </p>

          <p className="pied-liens">
            <a className="bouton" href={CONNEXION}>
              {c.seConnecter}
            </a>
          </p>

          <p className="credits">{c.credits}</p>
        </div>
      </footer>
    </>
  );
}
