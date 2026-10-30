//! Partager sa position depuis le navigateur, sans compte.
//!
//! On se nomme, on partage, on envoie le lien : le proche suit la position
//! sur la page `/live/<jeton>` du serveur, comme celle que l'app iOS envoie.
//! Rien d'autre ne part au serveur — ni compte, ni historique.
//!
//! Ce que le navigateur retient, il le garde chez lui :
//!   - le nom et le partage en cours dans `localStorage`, par le magasin de
//!     Mémoire (`profil.rs`) : un rechargement reprend le partage là où il
//!     était, et deux onglets voient le même ;
//!   - la trace des positions envoyées dans IndexedDB (`trace.rs`), effacée
//!     à l'arrêt.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use paquets::active::prelude::*;
use paquets::orm::web::LocalSubscription;

use crate::api::{self, Erreur};
use crate::localisation::{self, Surveillance};
use crate::navigateur::{au_montage, fenetre, lance, maintenant_ms};
use crate::porte::{decor, defilant};
use crate::profil::{profil, PartageEnCours, NOM_MAX};
use crate::session;
use crate::trace::{self, Point, Resume};
use crate::ui::{lettres, mots, roule};

/// Une position part au plus toutes les cinq secondes, comme dans l'app.
const INTERVALLE_MS: f64 = 5000.0;

/// Ce que l'écran reçoit.
pub struct Proprietes {
    /// « J'ai un compte » : vers la porte.
    pub vers_compte: Rc<dyn Fn()>,
}

/// Un message, et s'il s'agit d'une erreur.
type Message = Option<(String, bool)>;

/// L'écran de partage.
pub fn partage(p: Proprietes) -> Node {
    let depart = profil().state();
    let nom = use_state(depart.nom);
    let en_cours = use_state(depart.partage);
    let resume = use_state(Resume::default());
    let message: State<Message> = use_state(None);
    let occupe = use_state(false);

    // Le magasin est la seule source : l'écran le suit, y compris quand un
    // autre onglet le change.
    let abonnement: Rc<RefCell<Option<LocalSubscription>>> =
        Rc::new(RefCell::new(Some(profil().subscribe_local(move |pr| {
            if nom.is_alive() {
                nom.set(pr.nom.clone());
                en_cours.set(pr.partage.clone());
            }
        }))));
    let suivi: Rc<RefCell<Option<Surveillance>>> = Rc::default();
    {
        let (abonnement, suivi) = (abonnement.clone(), suivi.clone());
        on_cleanup(move || {
            abonnement.borrow_mut().take();
            suivi.borrow_mut().take();
        });
    }

    // Oublie le partage ici : le magasin, la trace, la surveillance.
    let oublie: Rc<dyn Fn()> = {
        let suivi = suivi.clone();
        Rc::new(move || {
            suivi.borrow_mut().take();
            profil().update(|pr| pr.partage = None);
            if resume.is_alive() {
                resume.set(Resume::default());
            }
            lance(trace::efface());
        })
    };

    // Suit la position et l'envoie au lien, au plus toutes les cinq secondes.
    let suit: Rc<dyn Fn(PartageEnCours)> = {
        let (suivi, oublie) = (suivi.clone(), oublie.clone());
        Rc::new(move |pc: PartageEnCours| {
            let dernier = Rc::new(Cell::new(f64::NEG_INFINITY));
            let oublie = oublie.clone();
            let surveillance = localisation::surveille(
                move |c| {
                    let t = maintenant_ms();
                    if t - dernier.get() < INTERVALLE_MS {
                        return;
                    }
                    dernier.set(t);
                    let (pc, oublie) = (pc.clone(), oublie.clone());
                    lance(async move {
                        match api::partage_envoie(&pc.jeton, &pc.secret, c).await {
                            Ok(()) => {
                                let point = Point {
                                    t,
                                    lat: c.lat,
                                    lon: c.lon,
                                };
                                trace::ajoute(point).await;
                                if resume.is_alive() {
                                    resume.update(|r| r.ajoute(point));
                                }
                            }
                            Err(Erreur::Statut(404, _)) => {
                                oublie();
                                if message.is_alive() {
                                    message.set(Some((
                                        "Le partage a expiré — relance-le pour un nouveau lien."
                                            .into(),
                                        true,
                                    )));
                                }
                            }
                            // Réseau coupé : la position suivante réessaiera.
                            Err(_) => {}
                        }
                    });
                },
                move |e| {
                    if message.is_alive() {
                        message.set(Some((e.into(), true)));
                    }
                },
            );
            if surveillance.is_none() {
                message.set(Some((
                    "Ce navigateur ne donne pas de position.".into(),
                    true,
                )));
            }
            *suivi.borrow_mut() = surveillance;
        })
    };

    // Un partage resté ouvert au dernier passage reprend, avec sa trace.
    if let Some(pc) = untrack(|| en_cours.get()) {
        let suit = suit.clone();
        au_montage(move || {
            suit(pc);
            lance(async move {
                let points = trace::tous().await;
                if resume.is_alive() {
                    resume.set(Resume::de(&points));
                }
            });
        });
    }

    let commence = {
        let suit = suit.clone();
        move |e: Event| {
            e.prevent_default();
            if untrack(|| occupe.get()) {
                return;
            }
            if !localisation::disponible() {
                message.set(Some((
                    "Ce navigateur ne donne pas de position.".into(),
                    true,
                )));
                return;
            }
            occupe.set(true);
            message.set(None);
            let suit = suit.clone();
            lance(async move {
                let nom = profil().get(|pr| pr.nom_affiche());
                let r = api::partage_ouvre(&nom).await;
                if !occupe.is_alive() {
                    return;
                }
                occupe.set(false);
                match r {
                    Ok(o) => {
                        let pc = PartageEnCours {
                            jeton: o.token,
                            secret: o.secret,
                            depuis_ms: maintenant_ms(),
                        };
                        trace::efface().await;
                        resume.set(Resume::default());
                        profil().update(|pr| pr.partage = Some(pc.clone()));
                        suit(pc);
                    }
                    Err(e) => message.set(Some((e.message(), true))),
                }
            });
        }
    };

    let arrete = {
        let oublie = oublie.clone();
        move |_: Event| {
            if let Some(pc) = untrack(|| en_cours.get()) {
                // Le serveur au mieux : injoignable, le lien expirera seul.
                lance(async move {
                    let _ = api::partage_arrete(&pc.jeton, &pc.secret).await;
                });
            }
            oublie();
            message.set(Some((
                "Partage arrêté : le lien ne montre plus rien.".into(),
                false,
            )));
        }
    };

    let lien = memo(move || {
        en_cours.with(|pc| {
            pc.as_ref().map(|pc| {
                let origine = fenetre()
                    .and_then(|f| f.location().origin().ok())
                    .unwrap_or_default();
                api::url_suivi(&session::base_api(), &origine, &pc.jeton)
            })
        })
    });

    let copie = move |_: Event| {
        if let Some(l) = untrack(|| lien.get()) {
            let texte = if presse_papiers(&l) {
                "Lien copié."
            } else {
                "Copie impossible ici : sélectionne le lien à la main."
            };
            message.set(Some((texte.into(), false)));
        }
    };

    let envoie = move |_: Event| {
        if let Some(l) = untrack(|| lien.get()) {
            if !feuille_de_partage(&l) && presse_papiers(&l) {
                message.set(Some(("Lien copié.".into(), false)));
            }
        }
    };

    let partage_actif = memo(move || en_cours.with(Option::is_some));

    let champ_nom = label()
        .class("champ")
        .style("--i:0")
        .child(span().text("Ton nom"))
        .child(
            input()
                .attr("type", "text")
                .attr("placeholder", "Un ami")
                .attr("autocomplete", "nickname")
                .attr("maxlength", NOM_MAX.to_string())
                .attr_dyn("value", move || nom.get())
                // Gardé tel que tapé : nettoyé seulement au départ, sinon
                // l'espace entre prénom et nom ne se taperait pas.
                .on_input(move |e: Event| {
                    let v: String = e.value().chars().take(NOM_MAX).collect();
                    profil().update(|pr| pr.nom = v);
                }),
        );

    let messages = move || {
        let m = message.get();
        untrack(|| {
            m.map(|(texte, erreur)| {
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
            .collect::<Vec<Node>>()
        })
    };

    let corps = move || {
        let actif = partage_actif.get();
        untrack(|| -> Vec<Node> {
            if !actif {
                return vec![
                    p_().class("porte-note")
                        .style("--i:1")
                        .text(
                            "Ton nom et ton partage restent dans ce navigateur. Le serveur ne \
                             garde que ta dernière position, le temps du partage.",
                        )
                        .into(),
                    button()
                        .attr("type", "submit")
                        .class("principal")
                        .attr("data-aimant", "")
                        .attr("data-onde", "")
                        .style("--i:2")
                        .bool_attr("disabled", move || occupe.get())
                        .child(span().children_dyn(move || {
                            vec![if occupe.get() {
                                span()
                                    .class("roue")
                                    .attr("aria-label", "Un instant…")
                                    .into()
                            } else {
                                "Partager ma position".into()
                            }]
                        }))
                        .into(),
                ];
            }
            vec![
                p_().class("porte-note")
                    .style("--i:1")
                    .text("Ta position est partagée. Envoie ce lien à qui doit te suivre :")
                    .into(),
                input()
                    .class("partage-lien")
                    .attr("readonly", "")
                    .attr("aria-label", "Lien de suivi")
                    .attr_dyn("value", move || lien.get().unwrap_or_default())
                    .into(),
                div()
                    .class("partage-actions")
                    .child(
                        button()
                            .attr("type", "button")
                            .class("principal")
                            .attr("data-onde", "")
                            .on_click(envoie)
                            .text("Envoyer le lien"),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .class("secondaire")
                            .attr("data-onde", "")
                            .on_click(copie)
                            .text("Copier"),
                    )
                    .into(),
                p_().class("partage-etat")
                    .attr("aria-live", "polite")
                    .text_dyn(move || etat(resume.get()))
                    .into(),
                button()
                    .attr("type", "button")
                    .class("lien")
                    .on_click(arrete.clone())
                    .text("Arrêter le partage")
                    .into(),
            ]
        })
    };

    let vers_compte = p.vers_compte.clone();
    let carte = form()
        .class("porte-carte")
        .attr("novalidate", "")
        .attr("data-incline", "3")
        .on("submit", commence)
        .child(champ_nom)
        .children_dyn(corps)
        .children_dyn(messages)
        .child(
            div().class("porte-liens").style("--i:4").child(
                button()
                    .attr("type", "button")
                    .class("lien")
                    .attr("data-onde", "")
                    .on_click(move |_: Event| vers_compte())
                    .text("J'ai un compte — se connecter"),
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
                                .text("Sans compte, sans app"),
                        )
                        .child(
                            h1().attr("data-mvt", "lettres")
                                .attr("aria-label", "MonCap GPS")
                                .child(lettres("MonCap GPS")),
                        )
                        .child(
                            p_().class("porte-accroche")
                                .attr("data-mvt", "mots")
                                .child(mots("Partage ta position en un lien.", 2)),
                        ),
                )
                .child(
                    div()
                        .class("porte-cadre")
                        .attr("data-mvt", "devoile")
                        .child(carte),
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

/// Ce que dit la ligne d'état d'un partage.
pub fn etat(r: Resume) -> String {
    match r.dernier {
        None => "En attente de ta position…".into(),
        Some(p) => {
            let n = r.points;
            let pluriel = if n > 1 { "s" } else { "" };
            match heure(p.t) {
                Some(h) => format!("{n} position{pluriel} envoyée{pluriel} · dernière à {h}"),
                None => format!("{n} position{pluriel} envoyée{pluriel}"),
            }
        }
    }
}

/// L'heure locale d'un instant, `14:05:09` (absente hors navigateur).
fn heure(ms: f64) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
        Some(format!(
            "{:02}:{:02}:{:02}",
            d.get_hours(),
            d.get_minutes(),
            d.get_seconds()
        ))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ms;
        None
    }
}

/// `navigator.<objet>.<methode>(arg)`, si le navigateur l'a.
fn appelle_navigateur(objet: Option<&str>, methode: &str, arg: &wasm_bindgen::JsValue) -> bool {
    use wasm_bindgen::JsCast;
    let Some(f) = fenetre() else {
        return false;
    };
    let mut cible: wasm_bindgen::JsValue = f.navigator().into();
    if let Some(o) = objet {
        match js_sys::Reflect::get(&cible, &o.into()) {
            Ok(v) if !v.is_undefined() => cible = v,
            _ => return false,
        }
    }
    let Ok(fonction) = js_sys::Reflect::get(&cible, &methode.into()) else {
        return false;
    };
    let Some(fonction) = fonction.dyn_ref::<js_sys::Function>() else {
        return false;
    };
    fonction.call1(&cible, arg).is_ok()
}

/// Copie dans le presse-papiers.
fn presse_papiers(texte: &str) -> bool {
    appelle_navigateur(Some("clipboard"), "writeText", &texte.into())
}

/// La feuille de partage du système (mobile surtout), si elle existe.
fn feuille_de_partage(lien: &str) -> bool {
    let donnees = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&donnees, &"title".into(), &"Suis ma position".into());
    let _ = js_sys::Reflect::set(&donnees, &"url".into(), &lien.into());
    appelle_navigateur(None, "share", &donnees)
}

/// L'élément `<p>`.
fn p_() -> Element {
    el("p")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_etat_compte_les_positions() {
        assert_eq!(etat(Resume::default()), "En attente de ta position…");
        let point = Point {
            t: 0.0,
            lat: 1.0,
            lon: 2.0,
        };
        assert_eq!(
            etat(Resume {
                points: 1,
                dernier: Some(point)
            }),
            "1 position envoyée"
        );
        assert_eq!(
            etat(Resume {
                points: 3,
                dernier: Some(point)
            }),
            "3 positions envoyées"
        );
    }

    #[test]
    fn sans_partage_on_se_nomme_puis_on_partage() {
        let html = partage(Proprietes {
            vers_compte: Rc::new(|| {}),
        })
        .render();
        assert!(html.contains("Ton nom"), "{html}");
        assert!(html.contains("Partager ma position"), "{html}");
        assert!(html.contains("J&#x27;ai un compte"), "{html}");
        assert!(html.contains(r#"autocomplete="nickname""#), "{html}");
    }
}
