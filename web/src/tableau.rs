//! Le tableau de bord : la carte, la recherche, le temps réel, les
//! positions et l'itinéraire. Portage de l'ancien `App.tsx`, fonction pour
//! fonction.
//!
//! L'état vit dans un seul `State` — le réducteur d'[`etat`](crate::etat) —
//! et chaque morceau de l'écran n'en lit que sa part, par un `memo` : une
//! voiture qui avance ne redessine pas la liste des positions.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gloo_timers::callback::{Interval, Timeout};
use paquets::active::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlInputElement;

use crate::api::{self, Erreur};
use crate::app::FinDeSession;
use crate::carte::{self, carte, Cadrage};
use crate::catalogue::{Categorie, Ressources, AVATARS, CATEGORIES};
use crate::compte::{self, compte};
use crate::etat::{Action, Annonce, Etat};
use crate::geo;
use crate::localisation::{self, Surveillance};
use crate::navigateur::{self, au_montage, brut, fenetre, lance, maintenant_ms};
use crate::nominatim;
use crate::session::{self, Session};
use crate::temps_reel::{Connexion, Envoi, Evenement};
use crate::types::{Coord, Lieu, NouvellePosition, Position};
use crate::ui::{compteur, lettres, roule};

/// Ce que le tableau reçoit de la racine.
pub struct Proprietes {
    /// La session : son jeton change quand le mot de passe change.
    pub session: State<Option<Session>>,
    pub ressources: Rc<Ressources>,
    pub fin: Rc<dyn Fn(FinDeSession)>,
    pub renouvelle: Rc<dyn Fn(Session)>,
}

/// Ce que les rappels du tableau partagent : l'état, le jeton, et la sortie.
#[derive(Clone)]
struct Ctx {
    etat: State<Rc<Etat>>,
    jeton: State<String>,
    fin: Rc<dyn Fn(FinDeSession)>,
}

impl Ctx {
    /// Le tableau est-il encore à l'écran ? Une réponse qui arrive après la
    /// déconnexion n'a plus personne à qui parler.
    fn vivant(&self) -> bool {
        self.etat.is_alive()
    }

    fn envoie(&self, a: Action) {
        if !self.vivant() {
            return;
        }
        let avant = untrack(|| self.etat.get());
        let apres = avant.clone().reduce(a);
        if !Rc::ptr_eq(&avant, &apres) {
            self.etat.set(apres);
        }
    }

    /// Le jeton du moment — lu avant chaque requête, il suit les
    /// renouvellements.
    fn jeton(&self) -> Option<String> {
        if !self.vivant() {
            return None;
        }
        Some(untrack(|| self.jeton.get())).filter(|j| !j.is_empty())
    }

    /// Une requête a échoué : une session refusée referme la porte, le
    /// reste s'annonce.
    fn echec(&self, e: Erreur) {
        if !self.vivant() {
            return;
        }
        if e == Erreur::NonAutorise {
            (self.fin)(FinDeSession::Expiree);
        } else {
            self.envoie(Action::Erreur(e.message()));
        }
    }

    /// Recharge les positions et les chiffres.
    fn recharge(&self) {
        let Some(jeton) = self.jeton() else { return };
        let ctx = self.clone();
        lance(async move {
            let r = async {
                let positions = api::positions(&jeton).await?;
                let stats = api::stats(&jeton).await?;
                Ok::<_, Erreur>((positions, stats))
            }
            .await;
            match r {
                Ok((positions, stats)) => ctx.envoie(Action::Donnees { positions, stats }),
                Err(e) => ctx.echec(e),
            }
        });
    }

    /// Lance une requête, et annonce son échec.
    fn requete<F, T>(
        &self,
        f: impl FnOnce(String) -> F + 'static,
        ok: impl FnOnce(&Ctx, T) + 'static,
    ) where
        F: std::future::Future<Output = Result<T, Erreur>> + 'static,
    {
        let Some(jeton) = self.jeton() else { return };
        let ctx = self.clone();
        lance(async move {
            match f(jeton).await {
                Ok(v) if ctx.vivant() => ok(&ctx, v),
                Ok(_) => {}
                Err(e) => ctx.echec(e),
            }
        });
    }
}

/// Ce qu'il faut pour dire sa position au temps réel.
#[derive(Clone)]
struct Emetteur {
    connexion: Rc<RefCell<Option<Connexion>>>,
    dernier_envoi: Rc<Cell<Option<(f64, Coord)>>>,
    avatar: Rc<RefCell<String>>,
    nom: State<String>,
}

impl Emetteur {
    /// Envoie la position si la règle de `geo::doit_envoyer` le permet.
    fn propose(&self, c: Coord) {
        let t = maintenant_ms();
        if !geo::doit_envoyer(self.dernier_envoi.get(), t, c) || !self.nom.is_alive() {
            return;
        }
        let nom = untrack(|| self.nom.get());
        let envoi = Envoi::Live {
            lat: c.lat,
            lon: c.lon,
            label: if nom.is_empty() { "Moi".into() } else { nom },
            // Lu à chaque envoi : changer d'avatar se voit tout de suite
            // chez les autres. L'ancien front gardait celui du démarrage du
            // partage.
            avatar: self.avatar.borrow().clone(),
        };
        let parti = self
            .connexion
            .borrow()
            .as_ref()
            .is_some_and(|cx| cx.envoie(&envoi));
        if parti {
            self.dernier_envoi.set(Some((t, c)));
        }
    }
}

/// L'élément `<p>`.
fn p_() -> Element {
    el("p")
}

/// Le champ qui a déclenché un événement.
fn champ_de(e: &Event) -> Option<HtmlInputElement> {
    brut(e)?.target()?.dyn_into().ok()
}

/// Le tableau de bord.
pub fn tableau(p: Proprietes) -> Node {
    let session = p.session;
    let res = p.ressources;
    let etat = use_state(Rc::new(Etat::default()));
    let jeton =
        memo(move || session.with(|s| s.as_ref().map(|s| s.jeton.clone()).unwrap_or_default()));
    let utilisateur = memo(move || {
        session.with(|s| {
            s.as_ref()
                .map(|s| s.utilisateur.clone())
                .unwrap_or_default()
        })
    });
    let ctx = Ctx {
        etat,
        jeton,
        fin: p.fin.clone(),
    };

    // Les parts de l'état que l'écran lit.
    let positions = memo(move || etat.with(|e| e.positions.clone()));
    let alertes = memo(move || etat.with(|e| e.alertes.clone()));
    let voitures = memo(move || etat.with(|e| Rc::new(e.voitures())));
    let moi = memo(move || etat.with(|e| e.moi));
    let partage = memo(move || etat.with(|e| e.partage));
    let connecte = memo(move || etat.with(|e| e.connecte));
    let stats = memo(move || etat.with(|e| e.stats.clone()));
    let itineraire = memo(move || etat.with(|e| e.itineraire.clone()));
    let annonce = memo(move || etat.with(|e| e.annonce.clone()));
    let cadrages = memo(move || etat.with(|e| e.cadrages));
    let n_direct = memo(move || voitures.with(|v| v.len()));
    let en_direct = memo(move || n_direct.get() > 0);

    let connexion: Rc<RefCell<Option<Connexion>>> = Rc::default();
    let surveillance: Rc<RefCell<Option<Surveillance>>> = Rc::default();
    let horloges: Rc<RefCell<Vec<Interval>>> = Rc::default();
    let avatar = use_state(session::avatar());
    let avatar_courant = Rc::new(RefCell::new(session::avatar()));

    let cadrage = use_state(Cadrage::default());
    let destination = use_state(None::<Lieu>);
    let recherche = use_state(String::new());
    let resultats = use_state(None::<Vec<Lieu>>);
    let cherche_en_cours = use_state(false);
    let brouillon = use_state(None::<Coord>);
    let nom_brouillon = use_state(String::new());
    let edition = use_state(None::<(i64, String)>);
    let edition_id = memo(move || edition.with(|e| e.as_ref().map(|(id, _)| *id)));
    let vitesse = use_state(50.0f64);
    let sortie_demandee = use_state(false);

    // Ce qui tourne en dehors de la vue s'arrête avec elle.
    {
        let (connexion, surveillance, horloges) =
            (connexion.clone(), surveillance.clone(), horloges.clone());
        on_cleanup(move || {
            connexion.borrow_mut().take();
            surveillance.borrow_mut().take();
            horloges.borrow_mut().clear();
        });
    }

    // --- Chargement des positions et des chiffres --------------------------
    {
        let ctx = ctx.clone();
        au_montage(move || ctx.recharge());
    }

    // --- Le temps réel ---------------------------------------------------------
    // Rouvert quand le jeton change : un nouveau mot de passe rend une
    // session neuve, l'ancienne ne vaut plus rien.
    {
        let ctx = ctx.clone();
        let connexion = connexion.clone();
        let derniere_verification = Rc::new(Cell::new(f64::NEG_INFINITY));
        effect(move || {
            let j = jeton.get();
            untrack(|| {
                connexion.borrow_mut().take();
                if j.is_empty() {
                    ctx.envoie(Action::Connexion(false));
                    return;
                }
                let lieu = fenetre().map(|f| f.location());
                let protocole = lieu
                    .as_ref()
                    .and_then(|l| l.protocol().ok())
                    .unwrap_or_default();
                let hote = lieu
                    .as_ref()
                    .and_then(|l| l.host().ok())
                    .unwrap_or_default();
                let url = api::url_ws(&session::base_api(), &protocole, &hote, &j);
                let (c1, c2, c3) = (ctx.clone(), ctx.clone(), ctx.clone());
                let derniere = derniere_verification.clone();
                *connexion.borrow_mut() = Some(Connexion::ouvre(
                    url,
                    move |ev| match ev {
                        Evenement::PositionsChanged => c1.recharge(),
                        evenement => c1.envoie(Action::Recu {
                            evenement,
                            maintenant_ms: maintenant_ms(),
                        }),
                    },
                    move |c| c2.envoie(Action::Connexion(c)),
                    // Une poignée de main refusée ressemble à une coupure :
                    // on demande au serveur si la session vit encore, au
                    // plus toutes les dix secondes.
                    move || {
                        let t = maintenant_ms();
                        if t - derniere.get() < 10_000.0 {
                            return;
                        }
                        derniere.set(t);
                        // Seul un refus compte : un serveur injoignable
                        // ne déconnecte personne, et ne s'annonce pas
                        // toutes les dix secondes.
                        let Some(j) = c3.jeton() else { return };
                        let c = c3.clone();
                        lance(async move {
                            if let Err(Erreur::NonAutorise) = api::moi(&j).await {
                                c.echec(Erreur::NonAutorise);
                            }
                        });
                    },
                ));
            });
        });
    }

    // --- Le partage de position ------------------------------------------------
    let emetteur = Emetteur {
        connexion: connexion.clone(),
        dernier_envoi: Rc::new(Cell::new(None)),
        avatar: avatar_courant.clone(),
        nom: utilisateur,
    };
    let derniere_position = Rc::new(Cell::new(None::<Coord>));

    let demarre_partage: Rc<dyn Fn()> = {
        let surveillance = surveillance.clone();
        let emetteur = emetteur.clone();
        let derniere_position = derniere_position.clone();
        let ctx = ctx.clone();
        Rc::new(move || {
            if surveillance.borrow().is_some() {
                return;
            }
            let emetteur = emetteur.clone();
            let derniere_position = derniere_position.clone();
            let (c_pos, c_err) = (ctx.clone(), ctx.clone());
            let s = localisation::surveille(
                move |c| {
                    c_pos.envoie(Action::Moi(c));
                    derniere_position.set(Some(c));
                    emetteur.propose(c);
                },
                move |m| {
                    c_err.envoie(Action::Erreur(m.into()));
                    c_err.envoie(Action::Partage(false));
                },
            );
            match s {
                Some(s) => {
                    *surveillance.borrow_mut() = Some(s);
                    ctx.envoie(Action::Partage(true));
                }
                None => ctx.envoie(Action::Erreur("Géolocalisation indisponible.".into())),
            }
        })
    };

    // Le partage démarre seul, comme avant : tout le monde apparaît sur la
    // carte des autres. Le navigateur demande d'abord la permission.
    //
    // Les horloges tournent à côté : le battement — le GPS d'un appareil
    // immobile peut se taire, et c'est justement là qu'il faut redire qu'on
    // est là —, la purge des voitures muettes, et la resynchronisation des
    // signalements (le serveur purge les expirés sans le dire à personne).
    {
        let demarre = demarre_partage.clone();
        let surveillance = surveillance.clone();
        let emetteur = emetteur.clone();
        let derniere_position = derniere_position.clone();
        let horloges = horloges.clone();
        let ctx = ctx.clone();
        au_montage(move || {
            demarre();
            let battement = Interval::new((geo::BATTEMENT_MS / 2.0) as u32, move || {
                if let (true, Some(c)) = (surveillance.borrow().is_some(), derniere_position.get())
                {
                    emetteur.propose(c);
                }
            });
            let c = ctx.clone();
            let purge = Interval::new(3000, move || {
                c.envoie(Action::Purge {
                    maintenant_ms: maintenant_ms(),
                })
            });
            let resync = Interval::new(60_000, move || {
                ctx.requete(
                    |j| async move { api::alertes(&j).await },
                    |ctx, a| ctx.envoie(Action::Alertes(a)),
                );
            });
            horloges.borrow_mut().extend([battement, purge, resync]);
        });
    }

    let bascule_partage = {
        let surveillance = surveillance.clone();
        let demarre = demarre_partage.clone();
        let ctx = ctx.clone();
        move |_: Event| {
            if untrack(|| partage.get()) {
                surveillance.borrow_mut().take();
                ctx.envoie(Action::Partage(false));
            } else {
                demarre();
            }
        }
    };

    // --- Les signalements --------------------------------------------------------
    let signale = {
        let connexion = connexion.clone();
        let ctx = ctx.clone();
        move |c: Categorie| {
            let connexion = connexion.clone();
            let c_envoi = ctx.clone();
            let envoie = move |ici: Coord| {
                let envoi = Envoi::Alert {
                    category: c.id.into(),
                    lat: ici.lat,
                    lon: ici.lon,
                    label: c.nom.into(),
                };
                let parti = connexion
                    .borrow()
                    .as_ref()
                    .is_some_and(|cx| cx.envoie(&envoi));
                c_envoi.envoie(if parti {
                    Action::Info(format!("{} {} : signalé, merci.", c.emoji, c.nom))
                } else {
                    Action::Erreur("Hors ligne : le signalement n'est pas parti.".into())
                });
            };
            match untrack(|| moi.get()) {
                Some(ici) => envoie(ici),
                None => {
                    let c_err = ctx.clone();
                    if !localisation::une_fois(envoie, move |_| {
                        c_err.envoie(Action::Erreur("Position requise pour signaler.".into()))
                    }) {
                        ctx.envoie(Action::Erreur("Géolocalisation indisponible.".into()));
                    }
                }
            }
        }
    };

    let vote: Rc<dyn Fn(i64, bool)> = {
        let ctx = ctx.clone();
        Rc::new(move |id, oui| {
            ctx.requete(
                move |j| async move { api::vote(&j, id, oui).await },
                |_, ()| {},
            );
        })
    };

    // --- La carte se recadre quand les positions changent de place ----------
    effect(move || {
        let n = cadrages.get();
        untrack(|| {
            let liste = positions.get();
            if n > 0 && !liste.is_empty() {
                cadrage.update(|c| {
                    c.numero += 1;
                    c.points = Rc::new(liste.iter().map(Position::coord).collect());
                    c.zoom_max = if liste.len() == 1 { 12.0 } else { 16.0 };
                });
            }
        });
    });

    // --- Au premier point GPS, la carte vient sur nous ------------------------
    // Sans positions enregistrées pour la cadrer, elle restait sur la France
    // entière, le point bleu perdu au milieu.
    let deja_venue = use_state(false);
    effect(move || {
        let Some(ici) = moi.get() else {
            return;
        };
        untrack(|| {
            if deja_venue.get() || !positions.with(|p| p.is_empty()) {
                return;
            }
            deja_venue.set(true);
            cadrage.update(|c| {
                c.numero += 1;
                c.points = Rc::new(vec![ici]);
                c.zoom_max = 13.0;
            });
        });
    });

    // --- La recherche de destination --------------------------------------------
    let cherche = {
        let ctx = ctx.clone();
        move |e: Event| {
            e.prevent_default();
            let q = untrack(|| recherche.get()).trim().to_string();
            if q.chars().count() < 3 || untrack(|| cherche_en_cours.get()) {
                return;
            }
            cherche_en_cours.set(true);
            let ctx = ctx.clone();
            lance(async move {
                let pres_de = untrack(|| moi.get());
                let r = nominatim::cherche(&q, pres_de).await;
                if !ctx.vivant() {
                    return;
                }
                match r {
                    Ok(l) => resultats.set(Some(l)),
                    Err(m) => ctx.envoie(Action::Erreur(m)),
                }
                cherche_en_cours.set(false);
            });
        }
    };

    let choisit = move |l: Lieu| {
        let mut points = vec![l.coord];
        points.extend(untrack(|| moi.get()));
        cadrage.update(|c| {
            c.numero += 1;
            c.points = Rc::new(points);
            c.zoom_max = 13.0;
        });
        destination.set(Some(l));
        resultats.set(None);
        recherche.set(String::new());
    };

    // --- Poser une position --------------------------------------------------------
    let pose: Rc<dyn Fn(Coord)> = Rc::new(move |c: Coord| {
        nom_brouillon.set(String::new());
        brouillon.set(Some(c));
    });

    let ajoute = {
        let ctx = ctx.clone();
        move |e: Event| {
            e.prevent_default();
            let Some(c) = untrack(|| brouillon.get()) else {
                return;
            };
            let label = match untrack(|| nom_brouillon.get()).trim() {
                "" => "Point".to_string(),
                t => t.to_string(),
            };
            brouillon.set(None);
            let p = NouvellePosition {
                lat: c.lat,
                lon: c.lon,
                label: label.clone(),
            };
            ctx.requete(
                move |j| async move { api::ajoute(&j, &p).await },
                move |ctx, _| {
                    ctx.envoie(Action::Info(format!("« {label} » ajoutée.")));
                    ctx.recharge();
                },
            );
        }
    };

    let bulle_brouillon: Rc<dyn Fn() -> Node> = Rc::new(move || {
        let ajoute = ajoute.clone();
        form()
            .class("nommer")
            .on("submit", ajoute)
            .child(
                label()
                    .attr("for", "nom-position")
                    .text("Nom de la position"),
            )
            .child(
                input()
                    .id("nom-position")
                    .attr("maxlength", "120")
                    .attr("placeholder", "Point")
                    .attr_dyn("value", move || nom_brouillon.get())
                    .on_input(move |e| nom_brouillon.set(e.value()))
                    // Le champ vient d'être posé dans la bulle : on lui
                    // donne la main au tour suivant, une fois la bulle en
                    // place.
                    .on_mount(|dom| {
                        if let Some(el) = navigateur::element(dom) {
                            Timeout::new(30, move || {
                                if let Some(i) = el.dyn_ref::<HtmlInputElement>() {
                                    let _ = i.focus();
                                }
                            })
                            .forget();
                        }
                    }),
            )
            .child(
                div()
                    .class("nommer-actions")
                    .child(
                        button()
                            .attr("type", "submit")
                            .class("petit")
                            .text("Ajouter"),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .class("petit secondaire")
                            .on_click(move |_| brouillon.set(None))
                            .text("Annuler"),
                    ),
            )
            .into()
    });

    // --- La liste des positions -----------------------------------------------------
    let supprime = {
        let ctx = ctx.clone();
        move |id: i64| {
            ctx.requete(
                move |j| async move { api::supprime(&j, id).await },
                |ctx, ()| ctx.recharge(),
            );
        }
    };

    let renomme = {
        let ctx = ctx.clone();
        move |e: Event| {
            e.prevent_default();
            let Some((id, texte)) = untrack(|| edition.get()) else {
                return;
            };
            edition.set(None);
            let Some(q) = untrack(|| positions.with(|v| v.iter().find(|q| q.id == id).cloned()))
            else {
                return;
            };
            let label = texte.trim().to_string();
            if label.is_empty() || label == q.label {
                return;
            }
            let p = NouvellePosition {
                lat: q.lat,
                lon: q.lon,
                label,
            };
            ctx.requete(
                move |j| async move { api::modifie(&j, id, &p).await },
                |ctx, _| ctx.recharge(),
            );
        }
    };

    let calcule = {
        let ctx = ctx.clone();
        move |_: Event| {
            let points: Vec<Coord> =
                untrack(|| positions.with(|v| v.iter().map(Position::coord).collect()));
            if points.len() < 2 {
                return;
            }
            let v = untrack(|| vitesse.get());
            ctx.requete(
                move |j| async move { api::itineraire(&j, &points, v).await },
                move |ctx, r| ctx.envoie(Action::Itineraire(Some((r, v)))),
            );
        }
    };

    let importe = {
        let ctx = ctx.clone();
        move |e: Event| {
            let Some(entree) = champ_de(&e) else { return };
            let fichier = entree.files().and_then(|f| f.get(0));
            entree.set_value("");
            let Some(fichier) = fichier else { return };
            let ctx = ctx.clone();
            lance(async move {
                let texte = JsFuture::from(fichier.text())
                    .await
                    .ok()
                    .and_then(|v| v.as_string());
                let Some(texte) = texte else {
                    ctx.envoie(Action::Erreur("Fichier illisible.".into()));
                    return;
                };
                ctx.requete(
                    move |j| async move { api::importe_gpx(&j, texte).await },
                    |ctx, v| {
                        ctx.envoie(Action::Info(match v.len() {
                            0 => "Aucun point trouvé dans ce fichier.".into(),
                            1 => "1 position importée.".into(),
                            n => format!("{n} positions importées."),
                        }));
                        ctx.recharge();
                    },
                );
            });
        }
    };

    // --- Les annonces s'effacent seules -----------------------------------------------
    {
        let ctx = ctx.clone();
        let minuteur: Rc<RefCell<Option<Timeout>>> = Rc::default();
        let numero = memo(move || annonce.with(|a| a.as_ref().map(|a| a.numero)));
        {
            let minuteur = minuteur.clone();
            on_cleanup(move || drop(minuteur.borrow_mut().take()));
        }
        effect(move || {
            let n = numero.get();
            untrack(|| {
                let ctx = ctx.clone();
                *minuteur.borrow_mut() =
                    n.map(|n| Timeout::new(5000, move || ctx.envoie(Action::EffaceAnnonce(n))));
            });
        });
    }

    let deconnexion = {
        let fin = p.fin.clone();
        move |_: Event| {
            if untrack(|| sortie_demandee.get()) {
                return;
            }
            sortie_demandee.set(true);
            fin(FinDeSession::Volontaire);
        }
    };

    // --- Le rendu ---------------------------------------------------------------------
    let etat_direct = move || {
        if connecte.get() {
            "Temps réel connecté"
        } else {
            "Temps réel déconnecté"
        }
        .to_string()
    };

    let barre = header()
        .class("barre")
        .attr("data-mvt", "monte")
        .child(
            a().class("marque")
                .href("/")
                .attr("aria-label", "MonCap GPS — le site")
                .child(
                    img()
                        .attr("src", res.logo.clone())
                        .attr("width", "34")
                        .attr("height", "34")
                        .attr("alt", ""),
                )
                .child(span().text("MonCap GPS")),
        )
        .child(
            span()
                .class("direct")
                .class_if("on", move || connecte.get())
                .attr("role", "status")
                .attr_dyn("title", etat_direct)
                .child(span().class("visuel-cache").text_dyn(etat_direct)),
        )
        .child(
            div()
                .class("identite")
                .child(
                    span()
                        .class("pastille")
                        // Une image neuve quand l'avatar change : la
                        // nouvelle saute à sa place.
                        .children_dyn({
                            let res = res.clone();
                            move || {
                                let a = avatar.get();
                                vec![img().attr("src", res.avatar(&a)).attr("alt", "").into()]
                            }
                        })
                        .child(span().text_dyn(move || utilisateur.get())),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .class("fantome")
                        .attr("data-aimant", "")
                        .attr("data-onde", "")
                        .bool_attr("disabled", move || sortie_demandee.get())
                        .on_click(deconnexion)
                        .child(span().child(roule("Déconnexion"))),
                ),
        );

    let accueil = section()
        .class("accueil")
        .child(
            h1().attr("data-mvt", "lettres")
                .attr("aria-label", "Où va-t-on ?")
                .child(lettres("Où va-t-on ?")),
        )
        .child(
            form()
                .class("recherche")
                .attr("role", "search")
                .attr("data-mvt", "monte")
                .on("submit", cherche)
                .child(
                    span()
                        .class("recherche-icone")
                        .attr("aria-hidden", "true")
                        .text("🔎"),
                )
                .child(
                    input()
                        .attr("type", "search")
                        .attr("enterkeyhint", "search")
                        .attr("aria-label", "Adresse ou lieu")
                        .attr("placeholder", "Adresse ou lieu, puis Entrée")
                        .attr_dyn("value", move || recherche.get())
                        .on_input(move |e| {
                            recherche.set(e.value());
                            if untrack(|| resultats.with(Option::is_some)) {
                                resultats.set(None);
                            }
                        }),
                )
                .child(
                    button()
                        .attr("type", "submit")
                        .class("petit")
                        .attr("data-aimant", "")
                        .attr("data-onde", "")
                        .bool_attr("disabled", move || {
                            cherche_en_cours.get()
                                || recherche.with(|r| r.trim().chars().count() < 3)
                        })
                        .child(span().children_dyn(move || {
                            vec![if cherche_en_cours.get() {
                                span().class("roue").attr("aria-label", "Recherche…").into()
                            } else {
                                "Chercher".into()
                            }]
                        })),
                ),
        )
        .children_dyn(move || {
            let Some(liste) = resultats.get() else {
                return Vec::new();
            };
            untrack(|| {
                let mut lignes: Vec<Node> = Vec::new();
                if liste.is_empty() {
                    lignes.push(li().class("vide").text("Aucun lieu trouvé.").into());
                }
                for (i, l) in liste.into_iter().enumerate() {
                    let nom = l.nom.clone();
                    lignes.push(
                        li().style(format!("--i:{i}"))
                            .child(
                                button()
                                    .attr("type", "button")
                                    .on_click(move |_| choisit(l.clone()))
                                    .text("📍 ")
                                    .text(nom),
                            )
                            .into(),
                    );
                }
                vec![ul().class("resultats").children(lignes).into()]
            })
        })
        .children_dyn(move || {
            let Some(d) = destination.get() else {
                return Vec::new();
            };
            untrack(|| {
                let cible = d.coord;
                vec![p_()
                    .class("cap")
                    .child(
                        span()
                            .text("🎯 ")
                            .child(strong().text("Vers : "))
                            .text(d.nom_court()),
                    )
                    .children_dyn(move || {
                        if !moi.with(Option::is_some) {
                            return Vec::new();
                        }
                        vec![span()
                            .class("cap-mesure")
                            .text_dyn(move || {
                                moi.get()
                                    .map(|m| {
                                        format!(
                                            "{} · direction {}",
                                            geo::distance_courte(
                                                geo::distance_km(m, cible) * 1000.0
                                            ),
                                            geo::boussole(geo::cap(m, cible)),
                                        )
                                    })
                                    .unwrap_or_default()
                            })
                            .into()]
                    })
                    .child(
                        button()
                            .attr("type", "button")
                            .class("effacer")
                            .attr("aria-label", "Effacer la destination")
                            .on_click(move |_| destination.set(None))
                            .text("✕"),
                    )
                    .into()]
            })
        });

    let la_carte = carte(carte::Proprietes {
        positions,
        voitures,
        alertes,
        moi,
        destination,
        brouillon,
        bulle_brouillon,
        cadrage,
        on_clic: pose,
        on_vote: vote,
        on_echap: Rc::new(move || brouillon.set(None)),
        ressources: res.clone(),
    });

    let chiffres = dl()
        .class("chiffres")
        .attr("data-mvt", "cascade")
        .child(
            div()
                .style("--i:0")
                .attr("data-incline", "8")
                .child(dt().text("positions"))
                .child(dd().child(compteur(
                    move || stats.with(|s| s.as_ref().map_or(0.0, |s| s.count as f64)),
                    0,
                    "",
                ))),
        )
        .child(
            div()
                .style("--i:1")
                .attr("data-incline", "8")
                .child(dt().text("de tracé"))
                .child(dd().child(compteur(
                    move || stats.with(|s| s.as_ref().map_or(0.0, |s| s.total_km)),
                    1,
                    " km",
                ))),
        )
        .child(
            div()
                .style("--i:2")
                .attr("data-incline", "8")
                .child(dt().text("en direct"))
                .child(dd().child(compteur(move || n_direct.get() as f64, 0, ""))),
        );

    let sur_la_route = article()
        .class("panneau")
        .attr("data-mvt", "monte")
        .attr("data-incline", "2")
        .child(h2().text("Sur la route"))
        .child(
            div()
                .class("rangee")
                .child(
                    button()
                        .attr("type", "button")
                        .attr("data-aimant", "")
                        .attr("data-onde", "")
                        .class_if("actif", move || partage.get())
                        .on_click(bascule_partage)
                        .child(span().text_dyn(move || {
                            if partage.get() {
                                "🟢 Partage en direct…"
                            } else {
                                "📍 Partager ma position"
                            }
                            .to_string()
                        })),
                )
                .children_dyn(move || {
                    if !en_direct.get() {
                        return Vec::new();
                    }
                    vec![span()
                        .class("badge")
                        .child(span().class("badge-pouls").attr("aria-hidden", "true"))
                        .text_dyn(move || format!("{} en direct", n_direct.get()))
                        .into()]
                }),
        )
        .child(
            div().class("signalements").children(
                CATEGORIES
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let c = *c;
                        let signale = signale.clone();
                        button()
                            .attr("type", "button")
                            .class("signaler")
                            .attr("data-onde", "")
                            .style(format!("--i:{i}"))
                            .on_click(move |_| signale(c))
                            .child(
                                span()
                                    .class("signaler-emoji")
                                    .attr("aria-hidden", "true")
                                    .text(c.emoji),
                            )
                            .text(c.nom)
                    })
                    .collect::<Vec<_>>(),
            ),
        );

    let liste_positions = ol()
        .class("liste")
        .child(keyed(
            move || {
                let en_edition = edition_id.get();
                positions.with(|v| {
                    v.iter()
                        .cloned()
                        .enumerate()
                        .map(|(i, q)| (i, en_edition == Some(q.id), q))
                        .collect::<Vec<_>>()
                })
            },
            |(_, _, q)| q.id,
            move |(i, en_edition, q)| {
                let id = q.id;
                let ligne = li().style(format!("--i:{}", (*i).min(8))).child(
                    span()
                        .class("numero")
                        .attr("aria-hidden", "true")
                        .text((i + 1).to_string()),
                );
                if *en_edition {
                    let depart = untrack(|| edition.get())
                        .map(|(_, t)| t)
                        .unwrap_or_default();
                    ligne
                        .child(
                            form()
                                .class("renommer")
                                .on("submit", renomme.clone())
                                .child(
                                    input()
                                        .attr("aria-label", "Nouveau nom")
                                        .attr("value", depart)
                                        .attr("maxlength", "120")
                                        .on_input(move |e| edition.set(Some((id, e.value())))),
                                )
                                .child(button().attr("type", "submit").class("petit").text("OK"))
                                .child(
                                    button()
                                        .attr("type", "button")
                                        .class("petit secondaire")
                                        .on_click(move |_| edition.set(None))
                                        .text("Annuler"),
                                ),
                        )
                        .into()
                } else {
                    let label = q.label.clone();
                    let supprime = supprime.clone();
                    ligne
                        .child(
                            span()
                                .class("etiquette")
                                .child(strong().text(q.label.clone()))
                                .child(em().text(format!("{:.4}, {:.4}", q.lat, q.lon))),
                        )
                        .child(
                            span()
                                .class("actions")
                                .child(
                                    button()
                                        .attr("type", "button")
                                        .class("petit secondaire")
                                        .on_click(move |_| edition.set(Some((id, label.clone()))))
                                        .text("Renommer"),
                                )
                                .child(
                                    button()
                                        .attr("type", "button")
                                        .class("petit danger")
                                        .on_click(move |_| supprime(id))
                                        .text("Supprimer"),
                                ),
                        )
                        .into()
                }
            },
        ))
        .children_dyn(move || {
            if !positions.with(|v| v.is_empty()) {
                return Vec::new();
            }
            vec![li()
                .class("vide")
                .text("Aucune position pour l'instant.")
                .into()]
        });

    let itineraire_panneau = article()
        .class("panneau")
        .attr("data-mvt", "monte")
        .attr("data-incline", "2")
        .child(h2().text("Itinéraire"))
        .child(
            p_().class("indice")
                .text("Posez des positions sur la carte, puis reliez-les."),
        )
        .child(
            div()
                .class("rangee")
                .child(
                    label()
                        .class("vitesse")
                        .text("Vitesse")
                        .child(
                            input()
                                .attr("type", "number")
                                .attr("min", "1")
                                .attr("max", "300")
                                .attr("inputmode", "numeric")
                                .attr("value", "50")
                                .on_input(move |e| {
                                    let v = champ_de(&e)
                                        .map(|i| i.value_as_number())
                                        .unwrap_or(f64::NAN);
                                    vitesse.set(if v.is_finite() && v >= 1.0 { v } else { 1.0 });
                                }),
                        )
                        .text("km/h"),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("data-aimant", "")
                        .attr("data-onde", "")
                        .bool_attr("disabled", move || positions.with(|v| v.len() < 2))
                        .on_click(calcule)
                        .child(span().child(roule("Itinéraire complet"))),
                )
                .child(
                    a().class("bouton secondaire")
                        .attr_dyn("href", move || api::url_gpx(&jeton.get()))
                        .attr("data-onde", "")
                        .attr("target", "_blank")
                        .attr("rel", "noopener")
                        .child(span().child(roule("Exporter GPX"))),
                )
                .child(
                    label()
                        .class("bouton secondaire")
                        .attr("data-onde", "")
                        .child(span().child(roule("Importer GPX")))
                        .child(
                            input()
                                .attr("type", "file")
                                .attr("accept", ".gpx,application/gpx+xml,text/xml")
                                .class("visuel-cache")
                                .on("change", importe),
                        ),
                ),
        )
        .children_dyn(move || {
            let Some((r, v)) = itineraire.get() else {
                return Vec::new();
            };
            vec![p_()
                .class("resultat-itineraire")
                .child(strong().text(geo::distance_courte(r.total_km * 1000.0)))
                .text(format!(
                    " · {} à {v:.0} km/h",
                    geo::duree_courte(r.duration_min)
                ))
                .into()]
        })
        .child(liste_positions);

    // Quatorze voitures, c'est un mur d'images : replié par défaut,
    // l'avatar courant en montre déjà la couleur en haut.
    let avatars = details()
        .class("panneau avatars")
        .attr("data-mvt", "monte")
        .child(summary().child(h2().text("Mon avatar")).children_dyn({
            let res = res.clone();
            move || {
                let a = avatar.get();
                vec![img().attr("src", res.avatar(&a)).attr("alt", "").into()]
            }
        }))
        .child(
            div().class("grille-avatars").children(
                AVATARS
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let id = a.id;
                        let avatar_courant = avatar_courant.clone();
                        button()
                            .attr("type", "button")
                            .attr("title", a.nom)
                            .attr("aria-label", a.nom)
                            .attr("data-onde", "")
                            .attr_dyn("aria-pressed", move || avatar.with(|x| x == id).to_string())
                            .class("avatar")
                            .class_if("choisi", move || avatar.with(|x| x == id))
                            .style(format!("--i:{i}"))
                            .on_click(move |_| {
                                session::ecris_avatar(id);
                                *avatar_courant.borrow_mut() = id.to_string();
                                avatar.set(id.to_string());
                            })
                            .child(
                                img()
                                    .attr("src", res.avatar(id))
                                    .attr("alt", "")
                                    .attr("loading", "lazy"),
                            )
                    })
                    .collect::<Vec<_>>(),
            ),
        );

    let mon_compte = compte(compte::Proprietes {
        jeton,
        utilisateur,
        fin: p.fin.clone(),
        renouvelle: p.renouvelle.clone(),
    });

    let annonces = div()
        .class("annonces")
        .attr("aria-live", "polite")
        .children_dyn({
            let ctx = ctx.clone();
            move || {
                let Some(a) = annonce.get() else {
                    return Vec::new();
                };
                untrack(|| vec![bandeau(a, ctx.clone())])
            }
        });

    div()
        .class("tableau")
        // La barre de lecture : la page parcourue (`--defile`).
        .child(
            div()
                .class("lecture")
                .attr("aria-hidden", "true")
                .attr("data-defile", "")
                .child(span()),
        )
        .child(barre)
        .child(accueil)
        .child(
            div()
                .class("carte-cadre")
                .attr("data-mvt", "devoile")
                .child(la_carte),
        )
        .child(
            p_().class("astuce")
                .text("Touchez la carte pour poser une position. Double-touchez pour zoomer."),
        )
        .child(chiffres)
        .child(
            section()
                .class("panneaux")
                .child(sur_la_route)
                .child(itineraire_panneau)
                .child(avatars)
                .child(mon_compte),
        )
        .child(annonces)
        .into()
}

/// Une annonce, avec de quoi la fermer.
fn bandeau(a: Annonce, ctx: Ctx) -> Node {
    let n = a.numero;
    p_().class("annonce")
        .class_if("erreur", {
            let erreur = a.erreur;
            move || erreur
        })
        .attr("role", if a.erreur { "alert" } else { "status" })
        .text(a.texte)
        .child(
            button()
                .attr("type", "button")
                .attr("aria-label", "Fermer")
                .on_click(move |_| ctx.envoie(Action::EffaceAnnonce(n)))
                .text("✕"),
        )
        .into()
}
