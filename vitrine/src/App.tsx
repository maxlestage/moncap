import {
  ATOUTS,
  AUTEUR,
  CHIFFRES,
  CONNEXION,
  DEPOT,
  FRANCHISE,
  LIEUX,
  MAQUETTE_ITINERAIRES,
  MODES,
  TECHNIQUES,
} from "./content";

/**
 * Vitrine d'une page, pensée pour le pouce d'abord.
 *
 * Rien d'interactif : une page qui ne fait que présenter n'a pas besoin
 * d'état, et le lecteur n'a pas à attendre du JavaScript pour lire. React
 * n'est ici que pour la structure et le typage du contenu.
 *
 * Les maquettes sont du HTML et du CSS, pas des images : elles restent nettes
 * à toutes les résolutions, suivent le thème sombre, se lisent au lecteur
 * d'écran, et ne coûtent pas un octet de téléchargement supplémentaire. Ce
 * sont des dessins de l'interface, marqués comme tels.
 */

/** L'écran de comparaison : le cœur de l'app quand le réseau est là. */
function MaquetteComparateur() {
  return (
    <figure className="maquette">
      <div className="ecran">
        <div className="modes" aria-hidden="true">
          {MODES.map((m, i) => (
            <span key={m} className={i === MODES.length - 1 ? "actif" : ""}>
              {m}
            </span>
          ))}
        </div>

        <ul className="routes">
          {MAQUETTE_ITINERAIRES.map((r) => (
            <li key={r.etiquette}>
              <span className={`pastille ${r.teinte}`} aria-hidden="true" />
              <div>
                <p className={`etiquette ${r.teinte}`}>{r.etiquette}</p>
                <p className="chiffres-route">
                  <strong>{r.duree}</strong> · {r.distance} · {r.virages} ·{" "}
                  {r.arrivee}
                </p>
                {r.denivele && <p className="detail">{r.denivele}</p>}
                <p className="detail">{r.cout}</p>
                {r.avertissement && (
                  <p className="alerte">{r.avertissement}</p>
                )}
              </div>
            </li>
          ))}
        </ul>

        <p className="demarrer" aria-hidden="true">
          Démarrer
        </p>
      </div>
      <figcaption>
        Dessin de l'écran de comparaison — mise en page et étiquettes réelles,
        valeurs d'exemple.
      </figcaption>
    </figure>
  );
}

/** Le bandeau de guidage, avec les pastilles vitesse et météo. */
function MaquetteGuidage() {
  return (
    <figure className="maquette">
      <div className="ecran">
        <div className="guidage" aria-hidden="true">
          <span className="fleche">↱</span>
          <div>
            <p className="distance">300 m</p>
            <p className="consigne">Tourner à droite sur la D912</p>
            <p className="precision">Sortie 12 — Saint-Émilion</p>
          </div>
        </div>

        <div className="pastilles" aria-hidden="true">
          <span className="meteo">🌤️ 21° Éclaircies</span>
          <span className="vitesse">
            <strong>78</strong>
            <small>km/h</small>
            <em>90</em>
          </span>
        </div>
      </div>
      <figcaption>
        Le bandeau de manœuvre, la vitesse et la limite du moment, la météo —
        dessin d'écran, valeurs d'exemple.
      </figcaption>
    </figure>
  );
}

/** Le bandeau des catégories de lieux. */
function MaquetteLieux() {
  return (
    <figure className="maquette">
      <div className="ecran">
        <ul className="lieux">
          {LIEUX.map((l, i) => (
            <li key={l.nom} className={i === 0 ? "actif" : ""}>
              <span aria-hidden="true">{l.emoji}</span> {l.nom}
            </li>
          ))}
        </ul>
      </div>
      <figcaption>
        Les 19 catégories, avec les libellés de l'application — montrées ici
        d'un seul tenant, alors que le bandeau défile sur le téléphone.
      </figcaption>
    </figure>
  );
}

export function App() {
  return (
    <>
      <header className="hero">
        {/* Le fond du bandeau va d'un bord à l'autre, mais son texte doit
            s'aligner sur celui du corps : d'où ce conteneur, de la même
            largeur que `main`. */}
        <div className="dedans">
          <p className="marque">MonCap GPS</p>
          <h1>
            Comparez, puis roulez
            <br />
            <em>avec ou sans réseau</em>
          </h1>
          <p className="chapeau">
            Un GPS pour iPhone qui met les itinéraires côte à côte — durée,
            virages, dénivelé, carburant, péages — puis vous guide. Et quand le
            réseau tombe, il continue de calculer tout seul.
          </p>

          <div className="actions">
            <a className="bouton" href={CONNEXION}>
              Se connecter
            </a>
            <a className="lien" href={DEPOT}>
              Voir le projet
            </a>
          </div>

          <dl className="chiffres">
            {CHIFFRES.map((c) => (
              <div key={c.legende}>
                <dt>{c.valeur}</dt>
                <dd>{c.legende}</dd>
              </div>
            ))}
          </dl>
        </div>
      </header>

      <main>
        <section aria-labelledby="comparateur-titre">
          <h2 id="comparateur-titre">Le trajet vous est présenté, pas imposé</h2>
          <p className="intro">
            Plusieurs itinéraires côte à côte, chacun étiqueté <em>le plus
            simple</em>, <em>le plus rapide</em> ou <em>le plus court</em>. Pour
            chacun : la durée, la distance, le nombre de virages, l'heure
            d'arrivée, le dénivelé — et ce que ça coûte.
          </p>
          <MaquetteComparateur />
          <p className="intro">
            Le carburant est calculé avec <strong>la consommation de votre
            véhicule</strong> et le prix réel des stations autour de vous (open
            data officiel), pas une moyenne nationale. Les péages s'ajoutent
            quand l'itinéraire en emprunte. Une forte côte est signalée avant le
            départ, pas découverte au milieu. À pied, à vélo ou en voiture, et
            l'on peut demander d'éviter péages ou autoroutes.
          </p>
        </section>

        <section aria-labelledby="guidage-titre">
          <h2 id="guidage-titre">Puis il vous emmène</h2>
          <p className="intro">
            La flèche suit la manœuvre réelle, la distance au prochain virage se
            met à jour, et le numéro de sortie s'affiche sous la consigne. La
            vitesse du moment est comparée à la limite de la route, lue sur
            OpenStreetMap : elle passe au rouge quand vous la dépassez.
          </p>
          <MaquetteGuidage />
        </section>

        <section aria-labelledby="lieux-titre">
          <h2 id="lieux-titre">Ce qu'il y a autour</h2>
          <p className="intro">
            Un bandeau de catégories au-dessus de la carte : une touche, et les
            lieux apparaissent dans la zone visible. L'essence affiche le prix à
            la pompe pour le carburant que vous avez choisi.
          </p>
          <MaquetteLieux />
        </section>

        <section aria-labelledby="atouts">
          <h2 id="atouts">Et aussi</h2>
          <ul className="cartes">
            {ATOUTS.map((a) => (
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
          <h2 id="preuve-titre">Une mesure vaut mieux qu'une promesse</h2>
          <p>
            Bordeaux → Limoges, <strong>222,5 km en 2 h 16</strong>, calculé
            sans aucun réseau sur le paquet Nouvelle-Aquitaine. Ce trajet
            traverse une ancienne frontière de découpage : les routes y sont
            recousues à la construction, pas rafistolées sur le téléphone.
          </p>
          <p className="note">
            Cette vérification tourne chaque semaine, sur le paquet réellement
            publié.
          </p>
        </section>

        <section aria-labelledby="pile-titre" className="pile">
          <h2 id="pile-titre">Ce qu'il y a dessous</h2>
          <dl>
            {TECHNIQUES.map((p) => (
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
          <h2 id="franchise-titre">Ce qu'elle ne fait pas</h2>
          <ul>
            {FRANCHISE.map((f) => (
              <li key={f}>{f}</li>
            ))}
          </ul>
        </section>
      </main>

      <footer>
        <p className="signature">
          Conçu et développé par <strong>{AUTEUR}</strong>.
        </p>
        <p>
          Cartes et données routières <strong>OpenStreetMap</strong> (ODbL),
          calcul d'itinéraire <strong>Valhalla</strong>, trafic{" "}
          <strong>TomTom</strong>, météo <strong>open-meteo</strong>, prix des
          carburants <strong>data.economie.gouv.fr</strong>, foyers{" "}
          <strong>NASA FIRMS</strong>, photos aériennes <strong>IGN</strong>.
        </p>
        <p>
          <a href={CONNEXION}>Se connecter</a> · <a href={DEPOT}>Code source</a>
        </p>
      </footer>
    </>
  );
}
