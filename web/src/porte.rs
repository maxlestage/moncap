//! L'écran de connexion et d'inscription.
//!
//! L'identifiant est l'adresse e-mail — comme dans l'app, et comme le dit
//! la politique de confidentialité : elle ne sert qu'à ça, et à retrouver
//! son mot de passe. La réponse du serveur à « Mot de passe oublié ? » est
//! la même que le compte existe ou non : cet écran ne peut pas servir à
//! deviner qui est inscrit.
//!
//! Ce que dit le serveur — identifiants refusés, trop de tentatives, mot de
//! passe trop faible — est montré tel quel. Une requête à la fois : le
//! bouton reste éteint tant que la réponse n'est pas revenue, et, après
//! « trop de tentatives », tant que l'attente demandée n'est pas passée.

use std::cell::RefCell;
use std::rc::Rc;

use gloo_timers::callback::Interval;
use paquets::active::prelude::*;

use crate::api;
use crate::navigateur::lance;
use crate::session::{self, Session};
use crate::ui::{lettres, mots, roule};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Connexion,
    Inscription,
}

/// Un message de la porte, numéroté : la même erreur deux fois de suite
/// rejoue son entrée. Le booléen dit si c'est une erreur.
type Message = Option<(u32, String, bool)>;

/// Ce que la porte reçoit.
pub struct Proprietes {
    /// Appelé avec la session ouverte.
    pub on_session: Rc<dyn Fn(Session)>,
    /// Un message à montrer en arrivant — « session expirée », par
    /// exemple —, et s'il s'agit d'une erreur.
    pub avis: Option<(String, bool)>,
}

/// La porte.
pub fn porte(p: Proprietes) -> Node {
    let mode = use_state(Mode::Connexion);
    let email = use_state(String::new());
    let mdp = use_state(String::new());
    let base = use_state(session::base_api());
    let message: State<Message> = use_state(p.avis.map(|(t, e)| (1, t, e)));
    let occupe = use_state(false);
    // Les secondes à attendre avant de réessayer, après un 429.
    let attente = use_state(0u32);
    let decompte: Rc<RefCell<Option<Interval>>> = Rc::default();
    {
        let decompte = decompte.clone();
        on_cleanup(move || drop(decompte.borrow_mut().take()));
    }

    let annonce = move |texte: String, erreur: bool| {
        if !message.is_alive() {
            return;
        }
        let n = untrack(|| message.with(|m| m.as_ref().map_or(0, |(n, _, _)| *n))) + 1;
        message.set(Some((n, texte, erreur)));
    };

    let bloque = {
        let decompte = decompte.clone();
        move |secondes: u32| {
            attente.set(secondes);
            let fini = Interval::new(1000, move || {
                if attente.is_alive() {
                    attente.update(|s| *s = s.saturating_sub(1));
                }
            });
            *decompte.borrow_mut() = Some(fini);
        }
    };

    let inscription = memo(move || mode.get() == Mode::Inscription);

    let soumet = {
        let on_session = p.on_session.clone();
        move |e: Event| {
            e.prevent_default();
            let (occupe_, attente_) = untrack(|| (occupe.get(), attente.get()));
            if occupe_ || attente_ > 0 {
                return;
            }
            let inscription = untrack(|| inscription.get());
            let adresse = api::identifiant(&untrack(|| email.get()));
            let mot_de_passe = untrack(|| mdp.get());
            if inscription && !email_valide(&adresse) {
                annonce("Adresse e-mail invalide.".into(), true);
                return;
            }
            if inscription && mot_de_passe.chars().count() < 6 {
                annonce("Mot de passe : 6 caractères minimum.".into(), true);
                return;
            }
            occupe.set(true);
            message.set(None);
            session::ecris_base_api(&untrack(|| base.get()));
            let on_session = on_session.clone();
            let bloque = bloque.clone();
            lance(async move {
                let r = api::authentifie(inscription, &adresse, &mot_de_passe).await;
                if !occupe.is_alive() {
                    return;
                }
                match r {
                    Ok(s) => on_session(s),
                    Err(echec) => {
                        occupe.set(false);
                        if let Some(s) = echec.attente_s.filter(|s| *s > 0) {
                            bloque(s);
                        }
                        annonce(echec.message, true);
                    }
                }
            });
        }
    };

    let oublie = move |_: Event| {
        let adresse = api::identifiant(&untrack(|| email.get()));
        if !email_valide(&adresse) {
            annonce(
                "Adresse e-mail invalide — tape-la au-dessus d'abord.".into(),
                true,
            );
            return;
        }
        occupe.set(true);
        session::ecris_base_api(&untrack(|| base.get()));
        lance(async move {
            let r = api::mot_de_passe_oublie(&adresse).await;
            if !occupe.is_alive() {
                return;
            }
            match r {
                Ok(()) => annonce(
                    "Si un compte existe avec cette adresse, un lien vient d'être envoyé. \
                     Regarde ta boîte (et les indésirables)."
                        .into(),
                    false,
                ),
                Err(echec) => annonce(echec.message, true),
            }
            occupe.set(false);
        });
    };

    let bascule = move |_: Event| {
        mode.update(|m| {
            *m = if *m == Mode::Connexion {
                Mode::Inscription
            } else {
                Mode::Connexion
            }
        });
        message.set(None);
    };

    let saisie = |etat: State<String>| move |e: Event| etat.set(e.value());

    let formulaire = form()
        .class("porte-carte")
        .attr("novalidate", "")
        .attr("data-incline", "3")
        .on("submit", soumet)
        .child(
            label()
                .class("champ")
                .style("--i:0")
                .child(span().text("E-mail"))
                .child(
                    input()
                        .attr("type", "email")
                        .attr("placeholder", "toi@exemple.fr")
                        .attr("autocapitalize", "none")
                        .attr("autocomplete", "username")
                        .attr("spellcheck", "false")
                        .attr("required", "")
                        .attr_dyn("value", move || email.get())
                        .on_input(saisie(email)),
                ),
        )
        .child(
            label()
                .class("champ")
                .style("--i:1")
                .child(span().text("Mot de passe"))
                .child(
                    input()
                        .attr("type", "password")
                        .attr("placeholder", "6 caractères minimum")
                        .attr("required", "")
                        .attr_dyn("autocomplete", move || {
                            if inscription.get() {
                                "new-password"
                            } else {
                                "current-password"
                            }
                            .to_string()
                        })
                        .attr_dyn("value", move || mdp.get())
                        .on_input(saisie(mdp)),
                ),
        )
        .children_dyn(move || {
            if !inscription.get() {
                return Vec::new();
            }
            vec![p_()
                .class("porte-note")
                .style("--i:2")
                .text("Ton e-mail sert d'identifiant — il ne sert qu'à ça, et à retrouver ton mot de passe.")
                .into()]
        })
        .children_dyn(move || {
            let m = message.get();
            untrack(|| {
                m.map(|(_, texte, erreur)| {
                    p_().class(if erreur {
                        "porte-message erreur"
                    } else {
                        "porte-message info"
                    })
                    .attr("role", if erreur { "alert" } else { "status" })
                    .text(texte)
                    .into()
                })
                .into_iter()
                .collect()
            })
        })
        .child(
            button()
                .attr("type", "submit")
                .class("principal")
                .attr("data-aimant", "")
                .attr("data-onde", "")
                .style("--i:3")
                .bool_attr("disabled", move || {
                    occupe.get()
                        || attente.get() > 0
                        || email.with(|e| e.trim().is_empty())
                        || mdp.with(String::is_empty)
                })
                .child(span().children_dyn(move || {
                    let (o, a, i) = (occupe.get(), attente.get(), inscription.get());
                    vec![if o {
                        span().class("roue").attr("aria-label", "Un instant…").into()
                    } else if a > 0 {
                        format!("Réessaie dans {}", api::duree_attente(a)).into()
                    } else if i {
                        "Créer mon compte".into()
                    } else {
                        "Se connecter".into()
                    }]
                })),
        )
        .child(
            div()
                .class("porte-liens")
                .style("--i:4")
                .child(
                    button()
                        .attr("type", "button")
                        .class("lien")
                        .attr("data-onde", "")
                        .on_click(bascule)
                        .text_dyn(move || {
                            if inscription.get() {
                                "Déjà un compte ? Se connecter"
                            } else {
                                "Pas de compte ? S'inscrire"
                            }
                            .to_string()
                        }),
                )
                .children_dyn(move || {
                    if inscription.get() {
                        return Vec::new();
                    }
                    vec![button()
                        .attr("type", "button")
                        .class("lien")
                        .bool_attr("disabled", move || occupe.get())
                        .on_click(oublie)
                        .text("Mot de passe oublié ?")
                        .into()]
                }),
        )
        .child(
            details()
                .class("porte-avance")
                .style("--i:5")
                .child(summary().text("Serveur (avancé)"))
                .child(
                    input()
                        .attr("placeholder", "URL de l'API (vide = même serveur)")
                        .attr("aria-label", "URL de l'API")
                        .attr_dyn("value", move || base.get())
                        .on_input(saisie(base)),
                ),
        );

    div()
        .class("porte")
        .child(decor())
        .child(
            main()
                .class("porte-dedans")
                .child(
                    section()
                        .class("porte-titre")
                        .child(
                            p_().class("surtitre")
                                .attr("data-mvt", "monte")
                                .text("Comparez, puis roulez"),
                        )
                        .child(
                            h1().attr("data-mvt", "lettres")
                                .attr("aria-label", "MonCap GPS")
                                .child(lettres("MonCap GPS")),
                        )
                        // Rejoue son entrée quand on change de mode : une
                        // phrase neuve, un élément neuf.
                        .children_dyn(move || {
                            let accroche = if inscription.get() {
                                "Bienvenue à bord."
                            } else {
                                "Content de te revoir."
                            };
                            vec![p_()
                                .class("porte-accroche")
                                .attr("data-mvt", "mots")
                                .child(mots(accroche, 2))
                                .into()]
                        }),
                )
                .child(
                    div()
                        .class("porte-cadre")
                        .attr("data-mvt", "devoile")
                        .child(formulaire),
                )
                .child(
                    p_().class("porte-pied")
                        .attr("data-mvt", "monte")
                        .child(a().href("/confidentialite").child(roule("Confidentialité")))
                        .text(" · ")
                        .child(a().href("/").child(roule("Le site"))),
                ),
        )
        .child(defilant())
        .into()
}

/// L'élément `<p>`.
fn p_() -> Element {
    el("p")
}

/// Le décor : une route qui se dessine, et un point GPS qui la parcourt.
fn decor() -> Node {
    const ROUTE: &str = "M-40 720 C 180 640 260 760 420 650 S 640 420 800 470 \
                         S 1040 640 1180 470 S 1360 190 1500 230";
    let mut dessin = svg()
        .attr("viewBox", "0 0 1440 900")
        .attr("preserveAspectRatio", "xMidYMid slice")
        .attr("focusable", "false")
        .child(
            path()
                .id("route-decor")
                .class("route-ombre")
                .attr("d", ROUTE),
        )
        .child(
            path()
                .class("route-trait")
                .attr("pathLength", "1")
                .attr("d", ROUTE),
        );
    // Le point suit la route — en SVG plutôt qu'en CSS : un `offset-path`
    // se compte en pixels de l'élément, et ne suivrait plus le tracé une
    // fois le dessin mis à l'échelle.
    if crate::navigateur::mouvement_permis() {
        dessin = dessin.child(
            g().class("route-point")
                .child(circle().attr("r", "22").class("halo"))
                .child(circle().attr("r", "7"))
                .child(
                    el("animateMotion")
                        .attr("dur", "14s")
                        .attr("repeatCount", "indefinite")
                        .attr("rotate", "auto")
                        .attr("begin", "1.2s")
                        .attr("calcMode", "spline")
                        .attr("keyTimes", "0;1")
                        .attr("keySplines", "0.45 0 0.55 1")
                        .child(el("mpath").attr("href", "#route-decor")),
                ),
        );
    }
    // `data-souris` : les épingles suivent la souris, chacune selon sa
    // profondeur (`--z`).
    div()
        .class("porte-decor")
        .attr("aria-hidden", "true")
        .attr("data-souris", "")
        .child(dessin)
        .child(
            div().class("flottants").children(
                FLOTTANTS
                    .iter()
                    .enumerate()
                    .map(|(i, (emoji, x, y, z))| {
                        span()
                            .class("flottant")
                            .style(format!("--x:{x}%;--y:{y}%;--z:{z};--i:{i}"))
                            .child(span().text(*emoji))
                    })
                    .collect::<Vec<_>>(),
            ),
        )
        .child(div().class("grain"))
        .into()
}

/// Les épingles qui flottent autour de la porte : ce que la communauté
/// signale sur la route. Emoji, position en pour cent de l'écran, et
/// profondeur — les plus proches bougent plus avec la souris.
const FLOTTANTS: [(&str, u8, u8, f32); 5] = [
    ("🚓", 88, 8, 1.6),
    ("⛽", 95, 76, 0.9),
    ("🚧", 6, 64, 1.3),
    ("📍", 47, 7, 0.7),
    ("⚠️", 64, 88, 1.1),
];

/// Le bandeau qui défile en bas de l'écran. Doublé, pour boucler sans
/// couture : quand la première moitié sort, la seconde est à sa place.
fn defilant() -> Node {
    let suite = || {
        span().class("defilant-suite").children(
            FONCTIONS
                .iter()
                .map(|f| span().text(*f).child(i().text("✦")))
                .collect::<Vec<_>>(),
        )
    };
    div()
        .class("defilant")
        .attr("aria-hidden", "true")
        .attr("data-defile", "")
        .child(div().class("defilant-piste").child(suite()).child(suite()))
        .into()
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

    fn rendu(avis: Option<(String, bool)>) -> String {
        porte(Proprietes {
            on_session: Rc::new(|_| {}),
            avis,
        })
        .render()
    }

    #[test]
    fn la_porte_s_ouvre_sur_la_connexion() {
        let html = rendu(None);
        assert!(html.contains("Se connecter"), "{html}");
        assert!(html.contains("Mot de passe oublié ?"));
        assert!(html.contains("Pas de compte ? S&#x27;inscrire"), "{html}");
        assert!(html.contains(r#"autocomplete="current-password""#));
        assert!(html.contains(r#"aria-label="MonCap GPS""#));
        // Champs vides : le bouton attend.
        assert!(
            html.contains(r#"<button class="principal" type="submit""#),
            "{html}"
        );
        assert!(html.contains(r#"disabled="""#));
        assert!(!html.contains("porte-message"));
    }

    #[test]
    fn une_session_expiree_le_dit_en_arrivant() {
        let html = rendu(Some((api::SESSION_EXPIREE.into(), true)));
        assert!(
            html.contains(r#"<p class="porte-message erreur" role="alert">Session expirée : reconnecte-toi.</p>"#),
            "{html}"
        );
    }

    #[test]
    fn le_bandeau_boucle_sans_couture() {
        let html = defilant().render();
        assert_eq!(html.matches("Carte en direct").count(), 2);
    }
}
