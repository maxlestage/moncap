//! Le compte : les appareils où il est ouvert, et le mot de passe.
//!
//! Chaque connexion est une session du serveur, avec son appareil. On peut
//! en refermer une — un téléphone perdu, un ordinateur prêté — ou toutes à
//! la fois. Changer de mot de passe referme aussi les autres : le serveur
//! rend une session neuve à cet appareil, et à lui seul.

use std::rc::Rc;

use paquets::active::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::{self, Erreur};
use crate::app::FinDeSession;
use crate::navigateur::{self, brut, lance};
use crate::session::Session;
use crate::types::Appareil;

/// Ce que le panneau reçoit du tableau.
pub struct Proprietes {
    /// Le jeton de la session, suivi : il change avec le mot de passe.
    pub jeton: State<String>,
    pub utilisateur: State<String>,
    /// Referme la session de cet appareil.
    pub fin: Rc<dyn Fn(FinDeSession)>,
    /// Remplace la session par celle que le serveur vient de rendre.
    pub renouvelle: Rc<dyn Fn(Session)>,
}

/// Depuis quand, en mots : « à l'instant », « il y a 5 min »… Les instants
/// sont en secondes Unix ; 0 veut dire inconnu.
pub fn il_y_a(maintenant_s: i64, t: i64) -> String {
    if t <= 0 {
        return "à une date inconnue".into();
    }
    let d = (maintenant_s - t).max(0);
    match d {
        0..=59 => "à l'instant".into(),
        60..=3599 => format!("il y a {} min", d / 60),
        3600..=86_399 => format!("il y a {} h", d / 3600),
        _ => format!("il y a {} j", d / 86_400),
    }
}

/// Le nom à montrer pour un appareil.
pub fn nom_appareil(a: &Appareil) -> String {
    match a.device.trim() {
        "" => "Appareil inconnu".into(),
        d => d.to_string(),
    }
}

/// Ce qu'on vérifie avant d'envoyer un nouveau mot de passe.
pub fn verifie_nouveau(
    actuel: &str,
    nouveau: &str,
    confirmation: &str,
) -> Result<(), &'static str> {
    if actuel.is_empty() {
        return Err("Tape ton mot de passe actuel.");
    }
    if nouveau.chars().count() < 6 {
        return Err("Nouveau mot de passe : 6 caractères minimum.");
    }
    if nouveau != confirmation {
        return Err("Les deux nouveaux mots de passe ne sont pas identiques.");
    }
    if nouveau == actuel {
        return Err("Le nouveau mot de passe est le même que l'actuel.");
    }
    Ok(())
}

/// Ce que montre la liste des appareils.
#[derive(Clone, Debug, PartialEq)]
enum Liste {
    /// Pas encore demandée : le panneau est replié.
    Rien,
    Chargement,
    Erreur(String),
    Pret(Vec<Appareil>),
}

/// Un message du panneau, numéroté : le même deux fois de suite rejoue
/// son entrée.
type Message = Option<(u32, String, bool)>;

#[derive(Clone, Copy)]
struct Etat {
    jeton: State<String>,
    utilisateur: State<String>,
    liste: State<Liste>,
    message: State<Message>,
    occupe: State<bool>,
}

impl Etat {
    fn vivant(&self) -> bool {
        self.liste.is_alive()
    }

    fn annonce(&self, texte: impl Into<String>, erreur: bool) {
        if !self.vivant() {
            return;
        }
        let n = untrack(|| self.message.with(|m| m.as_ref().map_or(0, |(n, _, _)| *n))) + 1;
        self.message.set(Some((n, texte.into(), erreur)));
    }

    fn jeton(&self) -> Option<String> {
        self.vivant().then(|| untrack(|| self.jeton.get()))
    }
}

/// Recharge la liste des appareils.
fn charge(e: Etat, fin: Rc<dyn Fn(FinDeSession)>) {
    let Some(jeton) = e.jeton() else { return };
    e.liste.set(Liste::Chargement);
    lance(async move {
        let r = api::appareils(&jeton).await;
        if !e.vivant() {
            return;
        }
        match r {
            Ok(mut v) => {
                // Cet appareil d'abord, puis les plus récemment actifs.
                v.sort_by_key(|a| (!a.current, -a.last_seen_at));
                e.liste.set(Liste::Pret(v));
            }
            Err(Erreur::NonAutorise) => fin(FinDeSession::Expiree),
            Err(err) => e.liste.set(Liste::Erreur(err.message())),
        }
    });
}

fn ligne(a: &Appareil, e: Etat, fin: Rc<dyn Fn(FinDeSession)>) -> Node {
    let maintenant = (navigateur::maintenant_ms() / 1000.0) as i64;
    let id = a.id;
    let mut nom = strong().text(nom_appareil(a));
    if a.current {
        nom = nom.child(span().class("appareil-ici").text(" · cet appareil"));
    }
    let detail = format!(
        "ouverte {} · active {}",
        il_y_a(maintenant, a.created_at),
        il_y_a(maintenant, a.last_seen_at)
    );
    let mut li_ = li()
        .class_if("courant", {
            let courant = a.current;
            move || courant
        })
        .child(
            span()
                .class("etiquette")
                .child(nom)
                .child(em().text(detail)),
        );
    if !a.current {
        li_ = li_.child(
            span().class("actions").child(
                button()
                    .attr("type", "button")
                    .class("petit danger")
                    .bool_attr("disabled", move || e.occupe.get())
                    .on_click(move |_| {
                        let Some(jeton) = e.jeton() else { return };
                        e.occupe.set(true);
                        let fin = fin.clone();
                        lance(async move {
                            let r = api::revoque(&jeton, id).await;
                            if !e.vivant() {
                                return;
                            }
                            e.occupe.set(false);
                            match r {
                                Ok(()) => {
                                    e.annonce("Appareil déconnecté.", false);
                                    charge(e, fin);
                                }
                                Err(Erreur::NonAutorise) => fin(FinDeSession::Expiree),
                                Err(err) => e.annonce(err.message(), true),
                            }
                        });
                    })
                    .text("Déconnecter"),
            ),
        );
    }
    li_.into()
}

/// Le panneau « Mon compte ».
pub fn compte(p: Proprietes) -> Node {
    let e = Etat {
        jeton: p.jeton,
        utilisateur: p.utilisateur,
        liste: use_state(Liste::Rien),
        message: use_state(None),
        occupe: use_state(false),
    };
    let fin = p.fin;
    let renouvelle = p.renouvelle;
    let actuel = use_state(String::new());
    let nouveau = use_state(String::new());
    let confirmation = use_state(String::new());

    // La liste n'est demandée qu'à l'ouverture du panneau : la plupart des
    // visites ne l'ouvrent pas.
    let a_l_ouverture = {
        let fin = fin.clone();
        move |ev: Event| {
            let ouvert = brut(&ev)
                .and_then(|ev| ev.current_target())
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                .is_some_and(|el| el.has_attribute("open"));
            if ouvert && untrack(|| e.liste.get()) == Liste::Rien {
                charge(e, fin.clone());
            }
        }
    };

    let liste = {
        let fin = fin.clone();
        fragment_dyn(move || {
            let l = e.liste.get();
            untrack(|| match l {
                Liste::Rien => Vec::new(),
                Liste::Chargement => vec![p_()
                    .class("indice")
                    .child(span().class("roue").attr("aria-label", "Chargement…"))
                    .into()],
                Liste::Erreur(m) => vec![p_()
                    .class("porte-message erreur")
                    .attr("role", "alert")
                    .text(m)
                    .into()],
                Liste::Pret(v) => vec![ol()
                    .class("liste appareils")
                    .children(
                        v.iter()
                            .map(|a| ligne(a, e, fin.clone()))
                            .collect::<Vec<_>>(),
                    )
                    .into()],
            })
        })
    };

    let partout = {
        let fin = fin.clone();
        move |_: Event| {
            let confirme = navigateur::fenetre()
                .and_then(|f| {
                    f.confirm_with_message("Déconnecter tous les appareils, celui-ci compris ?")
                        .ok()
                })
                .unwrap_or(false);
            if !confirme {
                return;
            }
            let Some(jeton) = e.jeton() else { return };
            e.occupe.set(true);
            let fin = fin.clone();
            lance(async move {
                let r = api::deconnexion_partout(&jeton).await;
                if !e.vivant() {
                    return;
                }
                e.occupe.set(false);
                match r {
                    Ok(()) => fin(FinDeSession::Partout),
                    Err(Erreur::NonAutorise) => fin(FinDeSession::Expiree),
                    Err(err) => e.annonce(err.message(), true),
                }
            });
        }
    };

    let change = {
        let fin = fin.clone();
        move |ev: Event| {
            ev.prevent_default();
            if untrack(|| e.occupe.get()) {
                return;
            }
            let (a, n, c) = untrack(|| (actuel.get(), nouveau.get(), confirmation.get()));
            if let Err(m) = verifie_nouveau(&a, &n, &c) {
                e.annonce(m, true);
                return;
            }
            let Some(jeton) = e.jeton() else { return };
            let utilisateur = untrack(|| e.utilisateur.get());
            e.occupe.set(true);
            let (fin, renouvelle) = (fin.clone(), renouvelle.clone());
            lance(async move {
                let r = api::change_mot_de_passe(&jeton, &utilisateur, &a, &n).await;
                if !e.vivant() {
                    return;
                }
                e.occupe.set(false);
                match r {
                    Ok(s) => {
                        renouvelle(s);
                        actuel.set(String::new());
                        nouveau.set(String::new());
                        confirmation.set(String::new());
                        e.annonce(
                            "Mot de passe changé. Les autres appareils ont été déconnectés.",
                            false,
                        );
                        if untrack(|| e.liste.get()) != Liste::Rien {
                            charge(e, fin);
                        }
                    }
                    Err(Erreur::NonAutorise) => fin(FinDeSession::Expiree),
                    Err(err) => e.annonce(err.message(), true),
                }
            });
        }
    };

    let champ = |libelle: &'static str, etat: State<String>, autocomplete: &'static str| {
        label().class("champ").child(span().text(libelle)).child(
            input()
                .attr("type", "password")
                .attr("autocomplete", autocomplete)
                .attr("required", "")
                .attr_dyn("value", move || etat.get())
                .on_input(move |ev| etat.set(ev.value())),
        )
    };

    details()
        .class("panneau compte")
        .attr("data-mvt", "monte")
        .on("toggle", a_l_ouverture)
        .child(
            summary().child(h2().text("Mon compte")).child(
                span()
                    .class("compte-qui")
                    .text_dyn(move || e.utilisateur.get()),
            ),
        )
        .child(
            div()
                .class("compte-corps")
                .children_dyn(move || {
                    let m = e.message.get();
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
                .child(h3().text("Appareils connectés"))
                .child(liste)
                .child(
                    div().class("rangee").child(
                        button()
                            .attr("type", "button")
                            .class("secondaire danger")
                            .attr("data-onde", "")
                            .bool_attr("disabled", move || e.occupe.get())
                            .on_click(partout)
                            .child(span().text("Déconnecter partout")),
                    ),
                )
                .child(h3().text("Changer le mot de passe"))
                .child(
                    form()
                        .class("compte-mdp")
                        .attr("novalidate", "")
                        .on("submit", change)
                        .child(champ("Mot de passe actuel", actuel, "current-password"))
                        .child(champ("Nouveau mot de passe", nouveau, "new-password"))
                        .child(champ("Confirmation", confirmation, "new-password"))
                        .child(
                            button()
                                .attr("type", "submit")
                                .attr("data-onde", "")
                                .bool_attr("disabled", move || {
                                    e.occupe.get()
                                        || actuel.with(String::is_empty)
                                        || nouveau.with(String::is_empty)
                                })
                                .child(span().children_dyn(move || {
                                    vec![if e.occupe.get() {
                                        span()
                                            .class("roue")
                                            .attr("aria-label", "Un instant…")
                                            .into()
                                    } else {
                                        "Changer le mot de passe".into()
                                    }]
                                })),
                        ),
                ),
        )
        .into()
}

/// L'élément `<p>`.
fn p_() -> Element {
    el("p")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depuis_quand_en_mots() {
        let t = 1_760_000_000;
        assert_eq!(il_y_a(t, t - 5), "à l'instant");
        assert_eq!(il_y_a(t, t - 300), "il y a 5 min");
        assert_eq!(il_y_a(t, t - 3 * 3600), "il y a 3 h");
        assert_eq!(il_y_a(t, t - 2 * 86_400), "il y a 2 j");
        assert_eq!(il_y_a(t, t + 60), "à l'instant", "une horloge en avance");
        assert_eq!(il_y_a(t, 0), "à une date inconnue");
    }

    #[test]
    fn le_nouveau_mot_de_passe_est_verifie_avant_de_partir() {
        assert!(verifie_nouveau("", "secret1", "secret1").is_err());
        assert!(verifie_nouveau("ancien", "court", "court").is_err());
        assert!(verifie_nouveau("ancien", "secret1", "secret2").is_err());
        assert!(verifie_nouveau("secret1", "secret1", "secret1").is_err());
        assert_eq!(verifie_nouveau("ancien", "secret1", "secret1"), Ok(()));
    }

    #[test]
    fn le_panneau_se_rend_replie() {
        let html = compte(Proprietes {
            jeton: use_state("j".to_string()),
            utilisateur: use_state("toi@exemple.fr".to_string()),
            fin: Rc::new(|_| {}),
            renouvelle: Rc::new(|_| {}),
        })
        .render();
        assert!(
            html.starts_with(r#"<details class="panneau compte""#),
            "{html}"
        );
        assert!(html.contains("toi@exemple.fr"));
        assert!(html.contains("Appareils connectés"));
        assert!(html.contains("Déconnecter partout"));
        assert!(html.contains(r#"autocomplete="current-password""#));
        // Replié, il ne demande rien : pas de liste encore.
        assert!(!html.contains("appareils"), "{html}");
    }
}
