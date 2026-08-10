import { ATOUTS, AUTEUR, CHIFFRES, CONNEXION, DEPOT, FRANCHISE } from "./content";

/**
 * Vitrine d'une page, pensée pour le pouce d'abord.
 *
 * Rien d'interactif : une page qui ne fait que présenter n'a pas besoin
 * d'état, et le lecteur n'a pas à attendre du JavaScript pour lire. React
 * n'est ici que pour la structure et le typage du contenu.
 */
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
            La navigation qui marche
            <br />
            <em>sans réseau</em>
          </h1>
          <p className="chapeau">
            Un GPS pour iPhone qui calcule ses itinéraires lui-même. Téléchargez
            votre région une fois, roulez partout — vallées, tunnels, étranger.
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
        <section aria-labelledby="atouts">
          <h2 id="atouts">Ce qu'elle fait</h2>
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
          calcul d'itinéraire <strong>Valhalla</strong>, photos aériennes{" "}
          <strong>IGN</strong>.
        </p>
        <p>
          <a href={CONNEXION}>Se connecter</a> · <a href={DEPOT}>Code source</a>
        </p>
      </footer>
    </>
  );
}
