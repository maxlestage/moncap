//! L'écran de connexion et d'inscription.
//!
//! L'identifiant est l'adresse e-mail — comme dans l'app, et comme le dit
//! la politique de confidentialité : elle ne sert qu'à ça, et à retrouver
//! son mot de passe. La réponse du serveur à « Mot de passe oublié ? » est
//! la même que le compte existe ou non : cet écran ne peut pas servir à
//! deviner qui est inscrit.

use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;

use crate::api;
use crate::session::{self, Session};
use crate::ui::Mots;

/// Validation volontairement simple — `local@domaine.tld`, la même que
/// l'app iOS : le seul vrai contrôle d'une adresse, c'est le courriel
/// qu'elle reçoit.
pub fn email_valide(s: &str) -> bool {
    let Some((local, domaine)) = s.split_once('@') else {
        return false;
    };
    let propre = |t: &str| !t.is_empty() && !t.contains('@') && !t.chars().any(char::is_whitespace);
    propre(local)
        && propre(domaine)
        && domaine
            .char_indices()
            .any(|(i, c)| c == '.' && i > 0 && i < domaine.len() - 1)
}

/// Ce que l'écran affiche en bandeau défilant : les fonctions du front web,
/// et seulement celles qui y sont vraiment.
pub const FONCTIONS: [&str; 6] = [
    "Carte en direct",
    "Positions partagées",
    "Signalements",
    "Itinéraires",
    "Import & export GPX",
    "Avatars",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Connexion,
    Inscription,
}

#[derive(Properties, PartialEq)]
pub struct Props {
    pub on_session: Callback<Session>,
}

#[function_component]
pub fn Porte(p: &Props) -> Html {
    let mode = use_state_eq(|| Mode::Connexion);
    let email = use_state(String::new);
    let mdp = use_state(String::new);
    let base = use_state(session::base_api);
    // Numérotés : la même erreur deux fois de suite rejoue son entrée.
    let message = use_state(|| None::<(u32, String, bool)>);
    let compteur = use_mut_ref(|| 0u32);
    let occupe = use_state_eq(|| false);

    let annonce = {
        let message = message.clone();
        let compteur = compteur.clone();
        move |texte: String, erreur: bool| {
            let mut n = compteur.borrow_mut();
            *n += 1;
            message.set(Some((*n, texte, erreur)));
        }
    };

    let inscription = *mode == Mode::Inscription;

    let soumet = {
        let email = email.clone();
        let mdp = mdp.clone();
        let base = base.clone();
        let occupe = occupe.clone();
        let message = message.clone();
        let annonce = annonce.clone();
        let on_session = p.on_session.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            if *occupe {
                return;
            }
            let adresse = email.trim().to_string();
            if inscription && !email_valide(&adresse) {
                annonce("Adresse e-mail invalide.".into(), true);
                return;
            }
            if inscription && mdp.chars().count() < 6 {
                annonce("Mot de passe : 6 caractères minimum.".into(), true);
                return;
            }
            occupe.set(true);
            message.set(None);
            session::ecris_base_api(&base);
            let mdp = (*mdp).clone();
            let occupe = occupe.clone();
            let annonce = annonce.clone();
            let on_session = on_session.clone();
            spawn_local(async move {
                match api::authentifie(inscription, &adresse, &mdp).await {
                    Ok(s) => {
                        session::ouvre(&s);
                        on_session.emit(s);
                    }
                    Err(m) => {
                        annonce(m, true);
                        occupe.set(false);
                    }
                }
            });
        })
    };

    let oublie = {
        let email = email.clone();
        let base = base.clone();
        let occupe = occupe.clone();
        let annonce = annonce.clone();
        Callback::from(move |_| {
            let adresse = email.trim().to_string();
            if !email_valide(&adresse) {
                annonce(
                    "Adresse e-mail invalide — tape-la au-dessus d'abord.".into(),
                    true,
                );
                return;
            }
            occupe.set(true);
            session::ecris_base_api(&base);
            let occupe = occupe.clone();
            let annonce = annonce.clone();
            spawn_local(async move {
                match api::mot_de_passe_oublie(&adresse).await {
                    Ok(()) => annonce(
                        "Si un compte existe avec cette adresse, un lien vient d'être envoyé. \
                         Regarde ta boîte (et les indésirables)."
                            .into(),
                        false,
                    ),
                    Err(m) => annonce(m, true),
                }
                occupe.set(false);
            });
        })
    };

    let bascule = {
        let mode = mode.clone();
        let message = message.clone();
        Callback::from(move |_| {
            mode.set(if *mode == Mode::Connexion {
                Mode::Inscription
            } else {
                Mode::Connexion
            });
            message.set(None);
        })
    };

    let saisie = |etat: &UseStateHandle<String>| {
        let etat = etat.clone();
        Callback::from(move |e: InputEvent| {
            etat.set(e.target_unchecked_into::<HtmlInputElement>().value());
        })
    };

    let accroche = if inscription {
        "Bienvenue à bord."
    } else {
        "Content de te revoir."
    };

    html! {
        <div class="porte">
            <Decor />

            <main class="porte-dedans">
                <section class="porte-titre">
                    <p class="surtitre" data-mvt="monte">{ "Comparez, puis roulez" }</p>
                    <h1 data-mvt="mots" aria-label="MonCap GPS">
                        <Mots texte="MonCap GPS" />
                    </h1>
                    // Rejoue son entrée quand on change de mode : une phrase
                    // neuve, un élément neuf.
                    <p key={accroche} class="porte-accroche" data-mvt="mots">
                        <Mots texte={accroche} depart={2} />
                    </p>
                </section>

                <div class="porte-cadre" data-mvt="devoile">
                <form class="porte-carte" onsubmit={soumet} novalidate=true>
                    <label class="champ" style="--i:0">
                        <span>{ "E-mail" }</span>
                        <input type="email" placeholder="toi@exemple.fr" autocapitalize="none"
                               autocomplete="email" required=true
                               value={(*email).clone()} oninput={saisie(&email)} />
                    </label>
                    <label class="champ" style="--i:1">
                        <span>{ "Mot de passe" }</span>
                        <input type="password" placeholder="6 caractères minimum" required=true
                               autocomplete={if inscription { "new-password" } else { "current-password" }}
                               value={(*mdp).clone()} oninput={saisie(&mdp)} />
                    </label>

                    if inscription {
                        <p class="porte-note" style="--i:2">
                            { "Ton e-mail sert d'identifiant — il ne sert qu'à ça, et à retrouver ton mot de passe." }
                        </p>
                    }

                    if let Some((n, texte, erreur)) = (*message).clone() {
                        <p key={n} class={if erreur { "porte-message erreur" } else { "porte-message info" }}
                           role={if erreur { "alert" } else { "status" }}>
                            { texte }
                        </p>
                    }

                    <button type="submit" class="principal" data-aimant="" style="--i:3"
                            disabled={*occupe || email.is_empty() || mdp.is_empty()}>
                        <span>
                            if *occupe {
                                <span class="roue" aria-label="Un instant…" />
                            } else if inscription {
                                { "Créer mon compte" }
                            } else {
                                { "Se connecter" }
                            }
                        </span>
                    </button>

                    <div class="porte-liens" style="--i:4">
                        <button type="button" class="lien" onclick={bascule}>
                            { if inscription { "Déjà un compte ? Se connecter" } else { "Pas de compte ? S'inscrire" } }
                        </button>
                        if !inscription {
                            <button type="button" class="lien" onclick={oublie} disabled={*occupe}>
                                { "Mot de passe oublié ?" }
                            </button>
                        }
                    </div>

                    <details class="porte-avance" style="--i:5">
                        <summary>{ "Serveur (avancé)" }</summary>
                        <input placeholder="URL de l'API (vide = même serveur)"
                               aria-label="URL de l'API"
                               value={(*base).clone()} oninput={saisie(&base)} />
                    </details>
                </form>
                </div>

                <p class="porte-pied" data-mvt="monte">
                    <a href="/confidentialite">{ "Confidentialité" }</a>
                    { " · " }
                    <a href="/">{ "Le site" }</a>
                </p>
            </main>

            <Defilant />
        </div>
    }
}

/// Le décor : une route qui se dessine, et un point GPS qui la parcourt.
#[function_component]
fn Decor() -> Html {
    const ROUTE: &str = "M-40 720 C 180 640 260 760 420 650 S 640 420 800 470 \
                         S 1040 640 1180 470 S 1360 190 1500 230";
    let bouge = mouvement::mouvement_permis();
    html! {
        <div class="porte-decor" aria-hidden="true">
            <svg viewBox="0 0 1440 900" preserveAspectRatio="xMidYMid slice" focusable="false">
                <path id="route-decor" class="route-ombre" d={ROUTE} />
                <path class="route-trait" pathLength="1" d={ROUTE} />
                // Le point suit la route — en SVG plutôt qu'en CSS : un
                // `offset-path` se compte en pixels de l'élément, et ne
                // suivrait plus le tracé une fois le dessin mis à l'échelle.
                if bouge {
                    <g class="route-point">
                        <circle r="22" class="halo" />
                        <circle r="7" />
                        <animateMotion dur="14s" repeatCount="indefinite" rotate="auto"
                                       begin="1.2s" calcMode="spline" keyTimes="0;1"
                                       keySplines="0.45 0 0.55 1">
                            <mpath href="#route-decor" />
                        </animateMotion>
                    </g>
                }
            </svg>
            <div class="grain" />
        </div>
    }
}

/// Le bandeau qui défile en bas de l'écran. Doublé, pour boucler sans
/// couture : quand la première moitié sort, la seconde est à sa place.
#[function_component]
fn Defilant() -> Html {
    let suite = || {
        html! {
            <span class="defilant-suite">
                { for FONCTIONS.iter().map(|f| html! {
                    <span key={*f}>{ *f }<i>{ "✦" }</i></span>
                }) }
            </span>
        }
    };
    html! {
        <div class="defilant" aria-hidden="true" data-defile="">
            <div class="defilant-piste">{ suite() }{ suite() }</div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_adresses_comme_l_app() {
        for bonne in ["a@b.fr", "prenom.nom@exemple.co.uk", "x@y..z"] {
            assert!(email_valide(bonne), "{bonne}");
        }
        for mauvaise in ["", "a@b", "a@.fr", "a@b.", "@b.fr", "a b@c.fr", "a@b@c.fr"] {
            assert!(!email_valide(mauvaise), "{mauvaise}");
        }
    }
}
