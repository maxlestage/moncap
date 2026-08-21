import {
  AUTEUR,
  CONNEXION,
  LIEUX,
  MAQUETTE_ITINERAIRES,
  MAQUETTE_REGIONS,
  MODE_ACTIF,
  MODES,
  SOURCES,
} from "./content";
import type { Contenu } from "./langues";
import { LANGUES } from "./langues";
import { app, appFormat } from "./traduire";

/**
 * Vitrine d'une page, pensée pour le pouce d'abord.
 *
 * Presque rien de scripté : une page qui ne fait que présenter n'a pas
 * besoin d'état, et le lecteur n'a pas à attendre du JavaScript pour lire.
 * Le choix du thème passe par des boutons radio et `:has()` ; la seule
 * exception est la flèche de défilement, un script inline d'un demi-Ko —
 * voir `SCRIPT_FLECHE`.
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
          {MODES.map((m) => (
            <span key={m} className={m === MODE_ACTIF ? "actif" : ""}>
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

/**
 * La seule pincée de script de la page — un choix explicite du patron, pas
 * une dérive : une flèche qui lit le sens du défilement. Vanilla, inline,
 * un demi-kilooctet — React n'est toujours pas du voyage. En descendant,
 * elle mène au prochain titre de section ; en remontant (ou arrivé en bas),
 * elle ramène au bandeau. Sans script, l'ancre remonte, simplement.
 */
const SCRIPT_FLECHE = `
(function () {
  var f = document.getElementById("fleche");
  if (!f) return;
  var titres = [].slice.call(document.querySelectorAll("main h2[id]"));
  var doux = matchMedia("(prefers-reduced-motion: reduce)").matches
    ? "auto" : "smooth";
  var dernier = pageYOffset;
  var monte = false;
  function maj() {
    var y = pageYOffset;
    if (Math.abs(y - dernier) > 4) { monte = y < dernier; dernier = y; }
    var enBas = innerHeight + y >= document.documentElement.scrollHeight - 60;
    var enHaut = y < 60;
    var haut = enBas || (monte && !enHaut);
    f.textContent = haut ? "\u2191" : "\u2304";
    f.setAttribute("aria-label", haut ? f.dataset.remonter : f.dataset.descendre);
    f.dataset.sens = haut ? "haut" : "bas";
  }
  f.addEventListener("click", function (e) {
    e.preventDefault();
    if (f.dataset.sens === "haut") { scrollTo({ top: 0, behavior: doux }); return; }
    var cible = null;
    for (var i = 0; i < titres.length; i++) {
      if (titres[i].getBoundingClientRect().top > 80) { cible = titres[i]; break; }
    }
    if (cible || titres.length) {
      (cible || titres[titres.length - 1]).scrollIntoView({ behavior: doux });
    }
  });
  addEventListener("scroll", maj, { passive: true });
  maj();
})();
`;

export function App({ c, res }: { c: Contenu; res: Ressources }) {
  // La mesure est mise en valeur au milieu de la phrase : le gabarit évite
  // de couper le texte en trois morceaux traduits séparément.
  const [avant, apres] = c.preuve.texte.split("{mesure}");

  return (
    <>
      <header className="hero" id="haut">
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

          {/* Le prix a sa section, pas une ligne en petit : il compte autant
              que les fonctions. Deux cartes côte à côte — Plus mis en avant,
              Pro pour ceux qui roulent pour travailler — le mensuel écrit en
              grand. L'essai n'a pas de carte : un délai avant paiement n'est
              pas une formule, et rien ici ne doit ressembler à du 0 €. */}
          <section aria-labelledby="tarifs-titre" className="tarifs">
            <h2 id="tarifs-titre">{c.tarifs.titre}</h2>
            <p className="intro">{c.tarifs.intro}</p>
            <div className="formules">
              {[c.tarifs.plus, c.tarifs.pro].map((f, i) => (
                <div key={f.nom} className={i === 0 ? "formule vedette" : "formule"}>
                  <h3>{f.nom}</h3>
                  <p className="prix">
                    {f.prix} <span>{f.periode}</span>
                  </p>
                  <ul>
                    {f.points.map((p) => (
                      <li key={p}>{p}</li>
                    ))}
                  </ul>
                </div>
              ))}
            </div>
            <p className="note">{c.tarifs.note}</p>
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

      <Pied c={c} res={res} />

      {/* La flèche unique : elle lit le sens du défilement — on descend,
          elle mène à la section suivante ; on remonte, elle ramène en haut.
          C'est la seule pincée de script de la page, voulue telle quelle ;
          sans lui, l'ancre remonte simplement au bandeau. */}
      <a
        className="fleche-nav"
        id="fleche"
        href="#haut"
        aria-label={c.chevrons.remonter}
        data-descendre={c.chevrons.descendre}
        data-remonter={c.chevrons.remonter}
      >
        ↑
      </a>
      <script dangerouslySetInnerHTML={{ __html: SCRIPT_FLECHE }} />
    </>
  );
}

/**
 * Le pied complet, partagé entre la page principale et les pages légales :
 * la marque et son slogan, les sections de la page, les sources de données
 * — chacune liée —, le légal, la signature et le copyright.
 *
 * Il reprend le bleu du bandeau : la page se ferme comme elle s'ouvre. Il
 * porte aussi le choix de langue, qu'on ne peut pas atteindre autrement
 * après avoir lu jusqu'ici.
 *
 * Les ancres visent toujours la page principale (`/#id`, `/es#id`…) : le
 * même pied sert depuis une page légale, où `#id` seul ne mènerait nulle
 * part.
 */
export function Pied({ c, res }: { c: Contenu; res: Ressources }) {
  return (
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

        <p className="slogan">{c.pied.slogan}</p>

        <p className="pied-liens">
          <a className="bouton" href={CONNEXION}>
            {c.seConnecter}
          </a>
        </p>

        <nav className="pied-colonnes">
          <div>
            <h3>{c.pied.decouvrir}</h3>
            <ul>
              {c.pied.ancres.map((a) => (
                <li key={a.id}>
                  <a href={`${c.chemin}#${a.id}`}>{a.nom}</a>
                </li>
              ))}
            </ul>
          </div>
          <div>
            <h3>{c.pied.donnees}</h3>
            <ul>
              {SOURCES.map((s) => (
                <li key={s.nom}>
                  <a href={s.url} rel="external noopener">
                    {s.nom}
                  </a>
                </li>
              ))}
            </ul>
          </div>
          <div>
            <h3>{c.pied.legalTitre}</h3>
            <ul>
              <li>
                <a href={c.mentions.chemin}>{c.mentions.titre}</a>
              </li>
              <li>
                <a href={c.confidentialite.chemin}>{c.confidentialite.titre}</a>
              </li>
            </ul>
          </div>
        </nav>

        <p className="signature">
          {c.signature} <strong>{AUTEUR}</strong>.
        </p>
        <p className="credits">{c.credits}</p>
        <p className="copyright">
          © {new Date().getFullYear()} {AUTEUR}. {c.pied.copyright}
        </p>
      </div>
    </footer>
  );
}
