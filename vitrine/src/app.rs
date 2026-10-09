//! La vitrine, en composants Yew.
//!
//! Portage fidèle de l'ancien `App.tsx` : même structure, mêmes classes CSS,
//! mêmes attributs d'accessibilité. La feuille de style n'a pas bougé d'une
//! ligne, et c'est voulu — un portage qui change deux choses à la fois ne se
//! vérifie plus.
//!
//! Les maquettes sont du HTML et du CSS, pas des images : elles restent
//! nettes à toutes les résolutions, suivent le thème, se lisent au lecteur
//! d'écran, et ne coûtent pas un octet de téléchargement supplémentaire. Ce
//! sont des dessins de l'interface, marqués comme tels.

use yew::prelude::*;

use crate::appstrings::app;
use crate::contenu::{
    Itineraire, Langue, PageLegale, AUTEUR, CONNEXION, LIEUX, MAQUETTE_ITINERAIRES,
    MAQUETTE_REGIONS, MODES, MODE_ACTIF, SOURCES,
};
use crate::fleche::FlecheNav;
use crate::traduire::{
    app_format,
    Valeur::{Nombre, Texte},
};

/// Ce que la coquille HTML transmet à l'application : les chemins des
/// ressources, que seule la construction connaît (ils portent une empreinte).
#[derive(Clone, PartialEq, Properties)]
pub struct Ressources {
    pub logo: AttrValue,
}

/// Les quatre thèmes, dans l'ordre des libellés fournis par la langue.
const THEMES: [&str; 4] = ["auto", "clair", "sombre", "orange"];

/// Quelle page afficher. Le chemin de l'URL la désigne.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Accueil,
    Mentions,
    Confidentialite,
}

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub langue: Langue,
    pub page: Page,
    pub res: Ressources,
    /// L'année du copyright. Fournie plutôt que lue ici : le composant doit
    /// rendre la même chose au test et dans le navigateur.
    pub annee: i32,
}

#[function_component]
pub fn App(p: &Props) -> Html {
    let c = p.langue.contenu();
    match p.page {
        Page::Accueil => html! { <Accueil ..p.clone() /> },
        Page::Mentions => html! { <Legale ..legale_props(p, &c.mentions) /> },
        Page::Confidentialite => html! { <Legale ..legale_props(p, &c.confidentialite) /> },
    }
}

fn legale_props(p: &Props, page: &'static PageLegale) -> LegaleProps {
    LegaleProps {
        langue: p.langue,
        page,
        res: p.res.clone(),
        annee: p.annee,
    }
}

// --- Les maquettes --------------------------------------------------------

/// La légende d'une maquette.
#[function_component]
fn Legende(props: &LegendeProps) -> Html {
    html! { <figcaption>{ props.texte }</figcaption> }
}

#[derive(Clone, PartialEq, Properties)]
struct LegendeProps {
    texte: &'static str,
}

#[derive(Clone, PartialEq, Properties)]
struct LangueProps {
    langue: Langue,
}

/// Le coût d'un itinéraire : carburant seul, ou carburant, péages et total.
///
/// Le total se recalcule au lieu d'être rangé à côté de ses parts — deux
/// façons d'écrire le même nombre finissent toujours par diverger.
fn ligne_cout(langue: Langue, r: &Itineraire) -> String {
    match r.peages {
        None => app_format(langue, "⛽ ~%.2f € · sans péage", &[Nombre(r.carburant)]),
        Some(p) => app_format(
            langue,
            "⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €",
            &[Nombre(r.carburant), Nombre(p), Nombre(r.total())],
        ),
    }
}

/// L'écran de comparaison : le cœur de l'app quand le réseau est là.
#[function_component]
fn MaquetteComparateur(props: &LangueProps) -> Html {
    let l = props.langue;
    let c = l.contenu();
    html! {
        <figure class="maquette">
            <div class="ecran">
                <div class="modes" aria-hidden="true">
                    { for MODES.iter().map(|m| html! {
                        <span key={*m} class={if *m == MODE_ACTIF { "actif" } else { "" }}>
                            { app(l, m) }
                        </span>
                    }) }
                </div>

                <ul class="routes">
                    { for MAQUETTE_ITINERAIRES.iter().map(|r| html! {
                        <li key={r.etiquette}>
                            <span class={classes!("pastille", r.teinte.classe())}
                                  aria-hidden="true" />
                            <div>
                                <p class={classes!("etiquette", r.teinte.classe())}>
                                    { app(l, r.etiquette) }
                                </p>
                                <p class="chiffres-route">
                                    { app_format(l, "%.0f min · %@ · %d virages · arr. %@", &[
                                        Nombre(r.minutes as f64), Texte(r.distance),
                                        Nombre(r.virages as f64), Texte(r.arrivee),
                                    ]) }
                                </p>
                                { r.denivele.map(|(montee, descente)| html! {
                                    <p class="detail">
                                        { app_format(l,
                                            "↗ %.0f m de côte · ↘ %.0f m de descente",
                                            &[Nombre(montee as f64), Nombre(descente as f64)]) }
                                    </p>
                                }).unwrap_or_default() }
                                <p class="detail">{ ligne_cout(l, r) }</p>
                                { r.avertissement.map(|m| html! {
                                    <p class="alerte">
                                        { app_format(l, "⚠️ Forte côte (%.0f m)",
                                                     &[Nombre(m as f64)]) }
                                    </p>
                                }).unwrap_or_default() }
                            </div>
                        </li>
                    }) }
                </ul>

                // Le comparateur en propose jusqu'à quinze : trois lignes
                // seules laisseraient croire à trois.
                <p class="reste" aria-hidden="true">{ format!("⌄ {}", c.reste) }</p>
                <p class="demarrer" aria-hidden="true">{ app(l, "Démarrer") }</p>
            </div>
            <Legende texte={c.legendes.itineraires} />
        </figure>
    }
}

/// Le bandeau de guidage, avec les pastilles vitesse et météo.
#[function_component]
fn MaquetteGuidage(props: &LangueProps) -> Html {
    let l = props.langue;
    let c = l.contenu();
    html! {
        <figure class="maquette">
            <div class="ecran">
                <div class="guidage" aria-hidden="true">
                    <span class="fleche">{ "↱" }</span>
                    <div>
                        <p class="distance">{ "300 m" }</p>
                        // Le bandeau ne montre pas une phrase mais un mot,
                        // lisible d'un coup d'œil au volant, et sa précision
                        // en dessous.
                        <p class="consigne">
                            { app_format(l, "Sortie %@", &[Texte("12")]) }
                        </p>
                        <p class="precision">{ app(l, "sortie") }</p>
                    </div>
                </div>

                <div class="pastilles" aria-hidden="true">
                    <span class="meteo">
                        { format!("🌤️ 21° {}", app(l, "Éclaircies")) }
                    </span>
                    <span class="vitesse">
                        <strong>{ "78" }</strong>
                        <small>{ "km/h" }</small>
                        <em>{ "90" }</em>
                    </span>
                    // Le panneau de commandes : un seul bloc arrondi. En
                    // guidage il ne montre que la 3D et le recentrage.
                    <span class="panneau">
                        <span>{ "3D" }</span>
                        <span class="loc">{ "➤" }</span>
                    </span>
                </div>

                // La barre d'arrivée : les minutes, l'heure en orange — l'une
                // répond à « c'est encore long ? », l'autre à « j'y suis
                // quand ? » — et le bouton pour quitter.
                <div class="eta-barre" aria-hidden="true">
                    <div>
                        <p class="eta-min">{ "12 min " }<em>{ "18:42" }</em></p>
                        <p class="eta-km">
                            { app_format(l, "%.1f km restants", &[Nombre(8.4)]) }
                        </p>
                    </div>
                    <span class="eta-quitter">{ app(l, "Quitter") }</span>
                </div>
            </div>
            <Legende texte={c.legendes.guidage} />
        </figure>
    }
}

/// L'app Apple Watch : le cadran reçu pendant un guidage.
#[function_component]
fn MaquetteMontre(props: &LangueProps) -> Html {
    let l = props.langue;
    let c = l.contenu();
    html! {
        <figure class="maquette montre">
            <div class="cadran" aria-hidden="true">
                <span class="fleche-poignet">{ "↱" }</span>
                <p class="distance-poignet">{ "400 m" }</p>
                <p class="consigne-poignet">{ app(l, "Droite") }</p>
                <div class="pied-poignet">
                    <span><strong>{ "8.4 km" }</strong><small>{ "restant" }</small></span>
                    <span><strong>{ "18:42" }</strong><small>{ "arrivée" }</small></span>
                </div>
            </div>
            <Legende texte={c.legendes.montre} />
        </figure>
    }
}

/// Le bandeau des catégories de lieux.
#[function_component]
fn MaquetteLieux(props: &LangueProps) -> Html {
    let l = props.langue;
    let c = l.contenu();
    html! {
        <figure class="maquette">
            <div class="ecran">
                <ul class="lieux">
                    { for LIEUX.iter().enumerate().map(|(i, lieu)| html! {
                        <li key={lieu.nom} class={if i == 0 { "actif" } else { "" }}>
                            <span aria-hidden="true">{ lieu.emoji }</span>
                            { format!(" {}", app(l, lieu.nom)) }
                        </li>
                    }) }
                </ul>
            </div>
            <Legende texte={c.legendes.lieux} />
        </figure>
    }
}

/// L'écran des régions installées, avec un téléchargement en cours.
#[function_component]
fn MaquetteRegions(props: &LangueProps) -> Html {
    let c = props.langue.contenu();
    html! {
        <figure class="maquette">
            <div class="ecran">
                <ul class="regions">
                    { for MAQUETTE_REGIONS.iter().map(|r| html! {
                        <li key={r.nom}>
                            <div class="ligne">
                                <span class="nom">{ r.nom }</span>
                                <span class="poids">{ r.poids }</span>
                            </div>
                            { match r.avancement {
                                None => html! {
                                    <p class="pret" aria-hidden="true">
                                        { c.maquette.installee }
                                    </p>
                                },
                                Some(pct) => html! {
                                    <>
                                        <div class="jauge" aria-hidden="true">
                                            <span style={format!("width: {pct}%")} />
                                        </div>
                                        <p class="encours">{ format!("{pct} %") }</p>
                                    </>
                                },
                            } }
                        </li>
                    }) }
                </ul>
            </div>
            <Legende texte={c.legendes.regions} />
        </figure>
    }
}

// --- Les pièces communes --------------------------------------------------

/// Choix du thème : quatre boutons radio que la feuille de style lit avec
/// `:has()`.
///
/// Toujours sans script, bien qu'on ait désormais le WebAssembly sous la
/// main : le CSS le fait déjà, et le faire en Rust ne gagnerait rien tout en
/// cessant de fonctionner pendant le chargement. Le choix ne survit pas à un
/// changement de page, et la page le dit plutôt que de laisser croire le
/// contraire.
#[function_component]
fn Themes(props: &LangueProps) -> Html {
    let c = props.langue.contenu();
    html! {
        <fieldset class="reglages">
            <legend>{ c.theme.libelle }</legend>
            { for THEMES.iter().zip(c.theme.noms.iter()).map(|(t, nom)| html! {
                <span key={*t}>
                    <input type="radio" name="theme" id={format!("th-{t}")}
                           checked={*t == "auto"} />
                    <label for={format!("th-{t}")}>{ *nom }</label>
                </span>
            }) }
        </fieldset>
    }
}

/// Les trois langues, la courante marquée comme telle.
#[function_component]
fn Langues(props: &LangueProps) -> Html {
    let c = props.langue.contenu();
    html! {
        <nav class="langues" aria-label={c.langue_libelle}>
            { for Langue::TOUTES.iter().map(|l| {
                let autre = l.contenu();
                if *l == props.langue {
                    html! { <span key={autre.code} aria-current="true">{ autre.nom }</span> }
                } else {
                    html! {
                        <a key={autre.code} href={autre.chemin}
                           hreflang={autre.code} lang={autre.code}>
                            { autre.nom }
                        </a>
                    }
                }
            }) }
        </nav>
    }
}

#[derive(Clone, PartialEq, Properties)]
pub struct PiedProps {
    pub langue: Langue,
    pub res: Ressources,
    pub annee: i32,
}

/// Le pied complet, partagé entre la page principale et les pages légales.
///
/// Il reprend le bleu du bandeau : la page se ferme comme elle s'ouvre. Il
/// porte aussi le choix de langue, qu'on ne peut pas atteindre autrement
/// après avoir lu jusqu'ici.
///
/// Les ancres visent toujours la page principale (`/#id`, `/es#id`…) : le
/// même pied sert depuis une page légale, où `#id` seul ne mènerait nulle
/// part.
#[function_component]
pub fn Pied(p: &PiedProps) -> Html {
    let c = p.langue.contenu();
    html! {
        <footer>
            <div class="dedans">
                <div class="pied-haut">
                    <p class="marque">
                        <img class="logo" src={p.res.logo.clone()}
                             width="40" height="40" alt="" />
                        { "MonCap GPS" }
                    </p>
                    <Langues langue={p.langue} />
                </div>

                <p class="slogan">{ c.pied.slogan }</p>

                <p class="pied-liens">
                    <a class="bouton" href={CONNEXION}>{ c.se_connecter }</a>
                </p>

                <nav class="pied-colonnes">
                    <div>
                        <h3>{ c.pied.decouvrir }</h3>
                        <ul>
                            { for c.pied.ancres.iter().map(|a| html! {
                                <li key={a.id}>
                                    <a href={format!("{}#{}", c.chemin, a.id)}>{ a.nom }</a>
                                </li>
                            }) }
                        </ul>
                    </div>
                    <div>
                        <h3>{ c.pied.donnees }</h3>
                        <ul>
                            { for SOURCES.iter().map(|s| html! {
                                <li key={s.nom}>
                                    <a href={s.url} rel="external noopener">{ s.nom }</a>
                                </li>
                            }) }
                        </ul>
                    </div>
                    <div>
                        <h3>{ c.pied.legal_titre }</h3>
                        <ul>
                            <li><a href={c.mentions.chemin}>{ c.mentions.titre }</a></li>
                            <li>
                                <a href={c.confidentialite.chemin}>
                                    { c.confidentialite.titre }
                                </a>
                            </li>
                        </ul>
                    </div>
                </nav>

                <p class="signature">
                    { c.signature }{ " " }<strong>{ AUTEUR }</strong>{ "." }
                </p>
                <p class="credits">{ c.credits }</p>
                <p class="copyright">
                    { format!("© {} {}. {}", p.annee, AUTEUR, c.pied.copyright) }
                </p>
            </div>
        </footer>
    }
}

// --- La page d'accueil ----------------------------------------------------

#[function_component]
fn Accueil(p: &Props) -> Html {
    let l = p.langue;
    let c = l.contenu();
    // La mesure est mise en valeur au milieu de la phrase : le gabarit évite
    // de couper le texte en trois morceaux traduits séparément.
    let (avant, apres) = match c.preuve.texte.split_once("{mesure}") {
        Some((a, b)) => (a, b),
        None => (c.preuve.texte, ""),
    };

    html! {
        <>
            <FilRoute />

            <header class="hero" id="haut">
                // Le fond du bandeau va d'un bord à l'autre, mais son texte
                // doit s'aligner sur celui du corps : d'où ce conteneur, de
                // la même largeur que `main`.
                <div class="dedans">
                    <div class="barre">
                        <p class="marque">
                            <img class="logo" src={p.res.logo.clone()}
                                 width="40" height="40" alt="" />
                            { "MonCap GPS" }
                        </p>
                        <Langues langue={l} />
                    </div>

                    <h1>
                        { c.h1[0] }<br />
                        <em>{ c.h1[1] }</em>
                    </h1>
                    <p class="chapeau">{ c.chapeau }</p>

                    <div class="actions">
                        <a class="bouton" href={CONNEXION}>{ c.se_connecter }</a>
                    </div>

                    <dl class="chiffres">
                        { for c.chiffres.iter().map(|n| html! {
                            <div key={n.legende}>
                                <dt>{ n.valeur }</dt>
                                <dd>{ n.legende }</dd>
                            </div>
                        }) }
                    </dl>

                    <Themes langue={l} />
                </div>
            </header>

            <main>
                <div class="dedans">
                    <section aria-labelledby="comparateur-titre">
                        <h2 id="comparateur-titre">{ c.comparateur.titre }</h2>
                        <p class="intro">{ c.comparateur.intro }</p>
                        <MaquetteComparateur langue={l} />
                        <p class="intro">{ c.comparateur.apres }</p>
                    </section>

                    <section aria-labelledby="guidage-titre">
                        <h2 id="guidage-titre">{ c.guidage.titre }</h2>
                        <p class="intro">{ c.guidage.intro }</p>
                        <MaquetteGuidage langue={l} />
                    </section>

                    <section aria-labelledby="montre-titre">
                        <h2 id="montre-titre">{ c.montre.titre }</h2>
                        <p class="intro">{ c.montre.intro }</p>
                        <MaquetteMontre langue={l} />
                    </section>

                    <section aria-labelledby="lieux-titre">
                        <h2 id="lieux-titre">{ c.lieux.titre }</h2>
                        <p class="intro">{ c.lieux.intro }</p>
                        <MaquetteLieux langue={l} />
                    </section>

                    <section aria-labelledby="atouts-titre">
                        <h2 id="atouts-titre">{ c.atouts_titre }</h2>
                        <ul class="cartes">
                            { for c.atouts.iter().map(|a| html! {
                                <li key={a.titre}>
                                    <span class="emoji" aria-hidden="true">{ a.emoji }</span>
                                    <h3>{ a.titre }</h3>
                                    <p>{ a.texte }</p>
                                </li>
                            }) }
                        </ul>
                    </section>

                    <section aria-labelledby="hors-ligne-titre">
                        <h2 id="hors-ligne-titre">{ c.hors_ligne.titre }</h2>
                        <p class="intro">{ c.hors_ligne.intro }</p>
                        <MaquetteRegions langue={l} />
                        <ul class="cartes">
                            { for c.hors_ligne_cartes.iter().map(|a| html! {
                                <li key={a.titre}>
                                    <span class="emoji" aria-hidden="true">{ a.emoji }</span>
                                    <h3>{ a.titre }</h3>
                                    <p>{ a.texte }</p>
                                </li>
                            }) }
                        </ul>
                    </section>

                    // Le prix a sa section, pas une ligne en petit : il compte
                    // autant que les fonctions. Deux cartes côte à côte — Plus
                    // mis en avant, Pro pour ceux qui roulent pour travailler.
                    // L'essai n'a pas de carte : un délai avant paiement n'est
                    // pas une formule, et rien ici ne doit ressembler à du 0 €.
                    <section aria-labelledby="tarifs-titre" class="tarifs">
                        <h2 id="tarifs-titre">{ c.tarifs.titre }</h2>
                        <p class="intro">{ c.tarifs.intro }</p>
                        <div class="formules">
                            { for [&c.tarifs.plus, &c.tarifs.pro].iter().enumerate()
                                .map(|(i, f)| html! {
                                <div key={f.nom}
                                     class={if i == 0 { "formule vedette" } else { "formule" }}>
                                    <h3>{ f.nom }</h3>
                                    <p class="prix">
                                        { f.prix }{ " " }<span>{ f.periode }</span>
                                    </p>
                                    <ul>
                                        { for f.points.iter().map(|pt| html! {
                                            <li key={*pt}>{ *pt }</li>
                                        }) }
                                    </ul>
                                </div>
                            }) }
                        </div>
                        <p class="note">{ c.tarifs.note }</p>
                    </section>

                    <section class="preuve" aria-labelledby="preuve-titre">
                        <h2 id="preuve-titre">{ c.preuve.titre }</h2>
                        <p>
                            { avant }
                            <strong>{ c.preuve.mesure }</strong>
                            { apres }
                        </p>
                        <p class="note">{ c.preuve.note }</p>
                    </section>

                    <section aria-labelledby="pile-titre" class="pile">
                        <h2 id="pile-titre">{ c.pile_titre }</h2>
                        <dl>
                            { for c.pile.iter().map(|pi| html! {
                                <div key={pi.domaine}>
                                    <dt>{ pi.domaine }</dt>
                                    <dd>
                                        <ul>
                                            { for pi.outils.iter().map(|o| html! {
                                                <li key={*o}>{ *o }</li>
                                            }) }
                                        </ul>
                                    </dd>
                                </div>
                            }) }
                        </dl>
                    </section>

                    <section aria-labelledby="franchise-titre" class="franchise">
                        <h2 id="franchise-titre">{ c.franchise_titre }</h2>
                        <ul>
                            { for c.franchise.iter().map(|f| html! {
                                <li key={*f}>{ *f }</li>
                            }) }
                        </ul>
                    </section>
                </div>
            </main>

            <Pied langue={l} res={p.res.clone()} annee={p.annee} />
            <FlecheNav langue={l} />
        </>
    }
}

// --- Les pages légales ----------------------------------------------------

#[derive(Clone, PartialEq, Properties)]
pub struct LegaleProps {
    pub langue: Langue,
    pub page: &'static PageLegale,
    pub res: Ressources,
    pub annee: i32,
}

/// Une page légale — mentions ou confidentialité. Même squelette que la page
/// principale : le bandeau bleu, le corps sur papier, le pied complet. Le
/// texte est nu et sans lien : ce qui s'y dit doit rester vrai sans
/// entretien.
#[function_component]
pub fn Legale(p: &LegaleProps) -> Html {
    let c = p.langue.contenu();
    html! {
        <>
            <header class="hero legale">
                <div class="dedans">
                    <div class="barre">
                        <p class="marque">
                            <img class="logo" src={p.res.logo.clone()}
                                 width="40" height="40" alt="" />
                            { "MonCap GPS" }
                        </p>
                    </div>
                    <h1>{ p.page.titre }</h1>
                    <p class="chapeau">
                        <a class="retour" href={c.chemin}>{ p.page.retour }</a>
                    </p>
                </div>
            </header>

            <main>
                <div class="dedans juridique">
                    { for p.page.sections.iter().map(|s| html! {
                        <section key={s.titre}>
                            <h2>{ s.titre }</h2>
                            { for s.corps.iter().map(|para| html! {
                                <p key={*para}>{ *para }</p>
                            }) }
                        </section>
                    }) }
                </div>
            </main>

            <Pied langue={p.langue} res={p.res.clone()} annee={p.annee} />
        </>
    }
}

// --- Le décor -------------------------------------------------------------

/// Le fil de l'itinéraire : un tracé qui se dessine à mesure qu'on descend.
///
/// Purement décoratif, et traité comme tel. `pathLength="1"` déclare au
/// navigateur que ce chemin mesure une unité : le pointillé se compte alors
/// en fractions du parcours, là où il aurait fallu mesurer la courbe pour
/// l'animer.
#[function_component]
fn FilRoute() -> Html {
    html! {
        <svg class="fil-route" viewBox="0 0 28 1000" preserveAspectRatio="none"
             aria-hidden="true" focusable="false">
            <path pathLength="1"
                  d="M14 0C6 110 22 190 14 300C6 410 24 470 14 560C4 650 22 720 14 830C7 920 18 960 14 1000" />
        </svg>
    }
}
