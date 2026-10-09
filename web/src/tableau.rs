//! Le tableau de bord : la carte, la recherche, le temps réel, les
//! positions et l'itinéraire. Portage de l'ancien `App.tsx`, fonction pour
//! fonction.

use std::cell::RefCell;
use std::rc::Rc;

use gloo_timers::callback::{Interval, Timeout};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::HtmlInputElement;
use yew::prelude::*;

use crate::api::{self, Erreur};
use crate::carte::{Cadrage, Carte};
use crate::catalogue::{Categorie, Ressources, AVATARS, CATEGORIES};
use crate::etat::{Action, Etat};
use crate::geo;
use crate::localisation::{self, Surveillance};
use crate::nominatim;
use crate::session::{self, Session};
use crate::temps_reel::{Connexion, Envoi, Evenement};
use crate::types::{Coord, Lieu, NouvellePosition, Position};
use crate::ui::{Compteur, Mots};

#[derive(Properties, PartialEq)]
pub struct Props {
    pub session: Session,
    pub on_deconnexion: Callback<()>,
}

fn maintenant_ms() -> f64 {
    js_sys::Date::now()
}

/// Ce qu'il faut pour dire sa position au temps réel.
#[derive(Clone)]
struct Emetteur {
    connexion: Rc<RefCell<Option<Connexion>>>,
    dernier_envoi: Rc<RefCell<Option<(f64, Coord)>>>,
    avatar: Rc<RefCell<String>>,
    nom: String,
}

impl Emetteur {
    /// Envoie la position si la règle de `geo::doit_envoyer` le permet.
    fn propose(&self, c: Coord) {
        let t = maintenant_ms();
        if !geo::doit_envoyer(*self.dernier_envoi.borrow(), t, c) {
            return;
        }
        let envoi = Envoi::Live {
            lat: c.lat,
            lon: c.lon,
            label: self.nom.clone(),
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
            *self.dernier_envoi.borrow_mut() = Some((t, c));
        }
    }
}

#[function_component]
pub fn Tableau(p: &Props) -> Html {
    let etat = use_reducer(Etat::default);
    let res = use_context::<Rc<Ressources>>().unwrap_or_default();
    let jeton = p.session.jeton.clone();
    let utilisateur = p.session.utilisateur.clone();

    let connexion: Rc<RefCell<Option<Connexion>>> = use_mut_ref(|| None);
    let surveillance: Rc<RefCell<Option<Surveillance>>> = use_mut_ref(|| None);
    let dernier_envoi = use_mut_ref(|| None::<(f64, Coord)>);
    let avatar = use_state(session::avatar);
    let avatar_courant = use_mut_ref(session::avatar);

    let cadrage = use_state(Cadrage::default);
    let destination = use_state(|| None::<Lieu>);
    let recherche = use_state(String::new);
    let resultats = use_state(|| None::<Vec<Lieu>>);
    let cherche_en_cours = use_state_eq(|| false);
    let brouillon = use_state(|| None::<Coord>);
    let nom_brouillon = use_state(String::new);
    let champ_brouillon = use_node_ref();
    let edition = use_state(|| None::<(i64, String)>);
    let vitesse = use_state(|| 50.0f64);

    // --- Erreurs : une session expirée referme la porte ---------------------
    let echec = {
        let d = etat.dispatcher();
        let deco = p.on_deconnexion.clone();
        Callback::from(move |e: Erreur| {
            if e == Erreur::NonAutorise {
                deco.emit(());
            } else {
                d.dispatch(Action::Erreur(e.message()));
            }
        })
    };

    // --- Chargement des positions et des chiffres --------------------------
    let recharge = {
        let d = etat.dispatcher();
        let echec = echec.clone();
        use_callback(jeton.clone(), move |_: (), jeton: &String| {
            let d = d.clone();
            let echec = echec.clone();
            let jeton = jeton.clone();
            spawn_local(async move {
                let r = async {
                    let positions = api::positions(&jeton).await?;
                    let stats = api::stats(&jeton).await?;
                    Ok::<_, Erreur>((positions, stats))
                }
                .await;
                match r {
                    Ok((positions, stats)) => d.dispatch(Action::Donnees { positions, stats }),
                    Err(e) => echec.emit(e),
                }
            });
        })
    };

    {
        let recharge = recharge.clone();
        use_effect_with((), move |_| recharge.emit(()));
    }

    // --- Le temps réel ---------------------------------------------------------
    {
        let connexion = connexion.clone();
        let d = etat.dispatcher();
        let recharge = recharge.clone();
        use_effect_with(jeton.clone(), move |jeton| {
            let lieu = web_sys::window().map(|f| f.location());
            let protocole = lieu
                .as_ref()
                .and_then(|l| l.protocol().ok())
                .unwrap_or_default();
            let hote = lieu
                .as_ref()
                .and_then(|l| l.host().ok())
                .unwrap_or_default();
            let url = api::url_ws(&session::base_api(), &protocole, &hote, jeton);
            let d2 = d.clone();
            let sur_evenement = Callback::from(move |ev: Evenement| match ev {
                Evenement::PositionsChanged => recharge.emit(()),
                evenement => d.dispatch(Action::Recu {
                    evenement,
                    maintenant_ms: maintenant_ms(),
                }),
            });
            let sur_etat = Callback::from(move |c: bool| d2.dispatch(Action::Connexion(c)));
            *connexion.borrow_mut() = Some(Connexion::ouvre(url, sur_evenement, sur_etat));
            move || {
                connexion.borrow_mut().take();
            }
        });
    }

    // --- Les horloges : purge des voitures muettes, resynchronisation des
    // signalements (le serveur purge les expirés sans le dire à personne).
    {
        let d = etat.dispatcher();
        let jeton = jeton.clone();
        use_effect_with((), move |_| {
            let d1 = d.clone();
            let purge = Interval::new(3000, move || {
                d1.dispatch(Action::Purge {
                    maintenant_ms: maintenant_ms(),
                })
            });
            let resync = Interval::new(60_000, move || {
                let d = d.clone();
                let jeton = jeton.clone();
                spawn_local(async move {
                    if let Ok(a) = api::alertes(&jeton).await {
                        d.dispatch(Action::Alertes(a));
                    }
                });
            });
            move || {
                drop(purge);
                drop(resync);
            }
        });
    }

    // --- Le partage de position ------------------------------------------------
    let emetteur = Emetteur {
        connexion: connexion.clone(),
        dernier_envoi: dernier_envoi.clone(),
        avatar: avatar_courant.clone(),
        nom: if utilisateur.is_empty() {
            "Moi".to_string()
        } else {
            utilisateur.clone()
        },
    };
    let derniere_position = use_mut_ref(|| None::<Coord>);

    let demarre_partage = {
        let surveillance = surveillance.clone();
        let emetteur = emetteur.clone();
        let derniere_position = derniere_position.clone();
        let d = etat.dispatcher();
        Callback::from(move |_: ()| {
            if surveillance.borrow().is_some() {
                return;
            }
            let emetteur = emetteur.clone();
            let derniere_position = derniere_position.clone();
            let d_pos = d.clone();
            let d_err = d.clone();
            let s = localisation::surveille(
                move |c| {
                    d_pos.dispatch(Action::Moi(c));
                    *derniere_position.borrow_mut() = Some(c);
                    emetteur.propose(c);
                },
                move |m| {
                    d_err.dispatch(Action::Erreur(m.into()));
                    d_err.dispatch(Action::Partage(false));
                },
            );
            match s {
                Some(s) => {
                    *surveillance.borrow_mut() = Some(s);
                    d.dispatch(Action::Partage(true));
                }
                None => d.dispatch(Action::Erreur("Géolocalisation indisponible.".into())),
            }
        })
    };

    // Le partage démarre seul, comme avant : tout le monde apparaît sur la
    // carte des autres. Le navigateur demande d'abord la permission.
    //
    // Le battement tourne à côté : le GPS d'un appareil immobile peut se
    // taire, et c'est justement là qu'il faut redire qu'on est là.
    {
        let demarre = demarre_partage.clone();
        let surveillance = surveillance.clone();
        let emetteur = emetteur.clone();
        let derniere_position = derniere_position.clone();
        use_effect_with((), move |_| {
            demarre.emit(());
            let s = surveillance.clone();
            let battement = Interval::new((geo::BATTEMENT_MS / 2.0) as u32, move || {
                let position = *derniere_position.borrow();
                if let (true, Some(c)) = (s.borrow().is_some(), position) {
                    emetteur.propose(c);
                }
            });
            move || {
                drop(battement);
                surveillance.borrow_mut().take();
            }
        });
    }

    let bascule_partage = {
        let surveillance = surveillance.clone();
        let demarre = demarre_partage.clone();
        let d = etat.dispatcher();
        let partage = etat.partage;
        Callback::from(move |_| {
            if partage {
                surveillance.borrow_mut().take();
                d.dispatch(Action::Partage(false));
            } else {
                demarre.emit(());
            }
        })
    };

    // --- Les signalements --------------------------------------------------------
    let signale = {
        let connexion = connexion.clone();
        let d = etat.dispatcher();
        let moi = etat.moi;
        Callback::from(move |c: Categorie| {
            let connexion = connexion.clone();
            let d = d.clone();
            let d_envoi = d.clone();
            let envoie = move |ici: Coord| {
                let d = d_envoi;
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
                d.dispatch(if parti {
                    Action::Info(format!("{} {} : signalé, merci.", c.emoji, c.nom))
                } else {
                    Action::Erreur("Hors ligne : le signalement n'est pas parti.".into())
                });
            };
            match moi {
                Some(ici) => envoie(ici),
                None => {
                    let d_err = d.clone();
                    if !localisation::une_fois(envoie, move |_| {
                        d_err.dispatch(Action::Erreur("Position requise pour signaler.".into()))
                    }) {
                        d.dispatch(Action::Erreur("Géolocalisation indisponible.".into()));
                    }
                }
            }
        })
    };

    let vote = {
        let jeton = jeton.clone();
        let echec = echec.clone();
        Callback::from(move |(id, oui): (i64, bool)| {
            let jeton = jeton.clone();
            let echec = echec.clone();
            spawn_local(async move {
                if let Err(e) = api::vote(&jeton, id, oui).await {
                    echec.emit(e);
                }
            });
        })
    };

    // --- La carte se recadre quand les positions changent de place ----------
    {
        let cadrage = cadrage.clone();
        let positions = etat.positions.clone();
        use_effect_with(etat.cadrages, move |n| {
            if *n > 0 && !positions.is_empty() {
                cadrage.set(Cadrage {
                    numero: cadrage.numero + 1,
                    points: Rc::new(positions.iter().map(Position::coord).collect()),
                    zoom_max: if positions.len() == 1 { 12.0 } else { 16.0 },
                });
            }
        });
    }

    // --- La recherche de destination --------------------------------------------
    let cherche = {
        let recherche = recherche.clone();
        let resultats = resultats.clone();
        let en_cours = cherche_en_cours.clone();
        let d = etat.dispatcher();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let q = recherche.trim().to_string();
            if q.chars().count() < 3 || *en_cours {
                return;
            }
            en_cours.set(true);
            let resultats = resultats.clone();
            let en_cours = en_cours.clone();
            let d = d.clone();
            spawn_local(async move {
                match nominatim::cherche(&q).await {
                    Ok(l) => resultats.set(Some(l)),
                    Err(m) => d.dispatch(Action::Erreur(m)),
                }
                en_cours.set(false);
            });
        })
    };

    let choisit = {
        let destination = destination.clone();
        let resultats = resultats.clone();
        let recherche = recherche.clone();
        let cadrage = cadrage.clone();
        let moi = etat.moi;
        Callback::from(move |l: Lieu| {
            let mut points = vec![l.coord];
            points.extend(moi);
            cadrage.set(Cadrage {
                numero: cadrage.numero + 1,
                points: Rc::new(points),
                zoom_max: 13.0,
            });
            destination.set(Some(l));
            resultats.set(None);
            recherche.set(String::new());
        })
    };

    // --- Poser une position --------------------------------------------------------
    let pose = {
        let brouillon = brouillon.clone();
        let nom = nom_brouillon.clone();
        let champ = champ_brouillon.clone();
        Callback::from(move |c: Coord| {
            brouillon.set(Some(c));
            nom.set(String::new());
            // Le champ vient d'être rendu dans la bulle : on lui donne la
            // main au tour suivant, une fois qu'il existe.
            let champ = champ.clone();
            Timeout::new(30, move || {
                if let Some(i) = champ.cast::<HtmlInputElement>() {
                    let _ = i.focus();
                }
            })
            .forget();
        })
    };

    let annule_brouillon = {
        let brouillon = brouillon.clone();
        Callback::from(move |_: ()| brouillon.set(None))
    };

    let ajoute = {
        let brouillon = brouillon.clone();
        let nom = nom_brouillon.clone();
        let jeton = jeton.clone();
        let echec = echec.clone();
        let recharge = recharge.clone();
        let d = etat.dispatcher();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let Some(c) = *brouillon else { return };
            let label = match nom.trim() {
                "" => "Point".to_string(),
                t => t.to_string(),
            };
            brouillon.set(None);
            let jeton = jeton.clone();
            let echec = echec.clone();
            let recharge = recharge.clone();
            let d = d.clone();
            spawn_local(async move {
                let p = NouvellePosition {
                    lat: c.lat,
                    lon: c.lon,
                    label: label.clone(),
                };
                match api::ajoute(&jeton, &p).await {
                    Ok(_) => {
                        d.dispatch(Action::Info(format!("« {label} » ajoutée.")));
                        recharge.emit(());
                    }
                    Err(e) => echec.emit(e),
                }
            });
        })
    };

    let bulle_brouillon = {
        let nom = nom_brouillon.clone();
        let annule = annule_brouillon.clone();
        html! {
            <form class="nommer" onsubmit={ajoute}>
                <label for="nom-position">{ "Nom de la position" }</label>
                <input id="nom-position" ref={champ_brouillon.clone()} maxlength="120"
                       placeholder="Point" value={(*nom).clone()}
                       oninput={Callback::from(move |e: InputEvent| {
                           nom.set(e.target_unchecked_into::<HtmlInputElement>().value())
                       })} />
                <div class="nommer-actions">
                    <button type="submit" class="petit">{ "Ajouter" }</button>
                    <button type="button" class="petit secondaire"
                            onclick={Callback::from(move |_| annule.emit(()))}>
                        { "Annuler" }
                    </button>
                </div>
            </form>
        }
    };

    // --- La liste des positions -----------------------------------------------------
    let supprime = {
        let jeton = jeton.clone();
        let echec = echec.clone();
        let recharge = recharge.clone();
        Callback::from(move |id: i64| {
            let jeton = jeton.clone();
            let echec = echec.clone();
            let recharge = recharge.clone();
            spawn_local(async move {
                match api::supprime(&jeton, id).await {
                    Ok(()) => recharge.emit(()),
                    Err(e) => echec.emit(e),
                }
            });
        })
    };

    let renomme = {
        let jeton = jeton.clone();
        let echec = echec.clone();
        let recharge = recharge.clone();
        let edition = edition.clone();
        let positions = etat.positions.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let Some((id, texte)) = (*edition).clone() else {
                return;
            };
            edition.set(None);
            let Some(q) = positions.iter().find(|q| q.id == id).cloned() else {
                return;
            };
            let label = texte.trim().to_string();
            if label.is_empty() || label == q.label {
                return;
            }
            let jeton = jeton.clone();
            let echec = echec.clone();
            let recharge = recharge.clone();
            spawn_local(async move {
                let p = NouvellePosition {
                    lat: q.lat,
                    lon: q.lon,
                    label,
                };
                match api::modifie(&jeton, id, &p).await {
                    Ok(_) => recharge.emit(()),
                    Err(e) => echec.emit(e),
                }
            });
        })
    };

    let calcule = {
        let jeton = jeton.clone();
        let echec = echec.clone();
        let d = etat.dispatcher();
        let positions = etat.positions.clone();
        let vitesse = *vitesse;
        Callback::from(move |_| {
            if positions.len() < 2 {
                return;
            }
            let points: Vec<Coord> = positions.iter().map(Position::coord).collect();
            let jeton = jeton.clone();
            let echec = echec.clone();
            let d = d.clone();
            spawn_local(async move {
                match api::itineraire(&jeton, &points, vitesse).await {
                    Ok(r) => d.dispatch(Action::Itineraire(Some((r, vitesse)))),
                    Err(e) => echec.emit(e),
                }
            });
        })
    };

    let importe = {
        let jeton = jeton.clone();
        let echec = echec.clone();
        let recharge = recharge.clone();
        let d = etat.dispatcher();
        Callback::from(move |e: Event| {
            let entree: HtmlInputElement = e.target_unchecked_into();
            let fichier = entree.files().and_then(|f| f.get(0));
            entree.set_value("");
            let Some(fichier) = fichier else { return };
            let jeton = jeton.clone();
            let echec = echec.clone();
            let recharge = recharge.clone();
            let d = d.clone();
            spawn_local(async move {
                let texte = JsFuture::from(fichier.text())
                    .await
                    .ok()
                    .and_then(|v| v.as_string());
                let Some(texte) = texte else {
                    d.dispatch(Action::Erreur("Fichier illisible.".into()));
                    return;
                };
                match api::importe_gpx(&jeton, texte).await {
                    Ok(v) => {
                        let n = v.len();
                        d.dispatch(Action::Info(match n {
                            0 => "Aucun point trouvé dans ce fichier.".into(),
                            1 => "1 position importée.".into(),
                            n => format!("{n} positions importées."),
                        }));
                        recharge.emit(());
                    }
                    Err(e) => echec.emit(e),
                }
            });
        })
    };

    let choisit_avatar = {
        let avatar = avatar.clone();
        let avatar_courant = avatar_courant.clone();
        Callback::from(move |id: &'static str| {
            session::ecris_avatar(id);
            *avatar_courant.borrow_mut() = id.to_string();
            avatar.set(id.to_string());
        })
    };

    // --- Les annonces s'effacent seules -----------------------------------------------
    {
        let d = etat.dispatcher();
        let numero = etat.annonce.as_ref().map(|a| a.numero);
        use_effect_with(numero, move |n| {
            let minuteur =
                n.map(|n| Timeout::new(5000, move || d.dispatch(Action::EffaceAnnonce(n))));
            move || drop(minuteur)
        });
    }

    let deconnexion = {
        let deco = p.on_deconnexion.clone();
        Callback::from(move |_| deco.emit(()))
    };

    // --- Le rendu ---------------------------------------------------------------------
    let voitures = Rc::new(etat.voitures());
    let n_direct = voitures.len();
    let stats = etat.stats.clone();

    html! {
        <div class="tableau">
            <header class="barre" data-mvt="monte">
                <a class="marque" href="/" aria-label="MonCap GPS — le site">
                    <img src={res.logo.clone()} width="34" height="34" alt="" />
                    <span>{ "MonCap GPS" }</span>
                </a>
                <span class={classes!("direct", etat.connecte.then_some("on"))}
                      role="status"
                      title={if etat.connecte { "Temps réel connecté" } else { "Temps réel déconnecté" }}>
                    <span class="visuel-cache">
                        { if etat.connecte { "Temps réel connecté" } else { "Temps réel déconnecté" } }
                    </span>
                </span>
                <div class="identite">
                    <span class="pastille">
                        <img src={res.avatar(&avatar)} alt="" />
                        <span>{ &utilisateur }</span>
                    </span>
                    <button type="button" class="fantome" data-aimant="" onclick={deconnexion}>
                        <span>{ "Déconnexion" }</span>
                    </button>
                </div>
            </header>

            <section class="accueil">
                <h1 data-mvt="mots"><Mots texte="Où va-t-on ?" /></h1>
                <form class="recherche" role="search" data-mvt="monte" onsubmit={cherche}>
                    <span class="recherche-icone" aria-hidden="true">{ "🔎" }</span>
                    <input type="search" enterkeyhint="search"
                           aria-label="Adresse ou lieu"
                           placeholder="Adresse ou lieu, puis Entrée"
                           value={(*recherche).clone()}
                           oninput={{
                               let recherche = recherche.clone();
                               let resultats = resultats.clone();
                               Callback::from(move |e: InputEvent| {
                                   recherche.set(e.target_unchecked_into::<HtmlInputElement>().value());
                                   resultats.set(None);
                               })
                           }} />
                    <button type="submit" class="petit" data-aimant=""
                            disabled={*cherche_en_cours || recherche.trim().chars().count() < 3}>
                        <span>
                            if *cherche_en_cours { <span class="roue" aria-label="Recherche…" /> }
                            else { { "Chercher" } }
                        </span>
                    </button>
                </form>

                if let Some(liste) = (*resultats).clone() {
                    <ul class="resultats">
                        if liste.is_empty() {
                            <li class="vide">{ "Aucun lieu trouvé." }</li>
                        }
                        { for liste.into_iter().enumerate().map(|(i, l)| {
                            let choisit = choisit.clone();
                            let nom = l.nom.clone();
                            html! {
                                <li key={i} style={format!("--i:{i}")}>
                                    <button type="button" onclick={Callback::from(move |_| choisit.emit(l.clone()))}>
                                        { "📍 " }{ nom }
                                    </button>
                                </li>
                            }
                        }) }
                    </ul>
                }

                if let Some(d) = (*destination).clone() {
                    <p class="cap" key={d.nom.clone()}>
                        <span>{ "🎯 " }<strong>{ "Vers : " }</strong>{ d.nom_court() }</span>
                        if let Some(moi) = etat.moi {
                            <span class="cap-mesure">
                                { format!(
                                    "{} · direction {}",
                                    geo::distance_courte(geo::distance_km(moi, d.coord) * 1000.0),
                                    geo::boussole(geo::cap(moi, d.coord)),
                                ) }
                            </span>
                        }
                        <button type="button" class="effacer" aria-label="Effacer la destination"
                                onclick={{
                                    let destination = destination.clone();
                                    Callback::from(move |_| destination.set(None))
                                }}>{ "✕" }</button>
                    </p>
                }
            </section>

            <div class="carte-cadre" data-mvt="devoile">
                <Carte positions={etat.positions.clone()}
                       voitures={voitures}
                       alertes={etat.alertes.clone()}
                       moi={etat.moi}
                       destination={(*destination).clone()}
                       brouillon={*brouillon}
                       bulle_brouillon={bulle_brouillon}
                       cadrage={(*cadrage).clone()}
                       on_clic={pose}
                       on_vote={vote}
                       on_echap={annule_brouillon} />
            </div>
            <p class="astuce">
                { "Touchez la carte pour poser une position. Double-touchez pour zoomer." }
            </p>

            <dl class="chiffres" data-mvt="cascade">
                <div style="--i:0">
                    <dt>{ "positions" }</dt>
                    <dd><Compteur valeur={stats.as_ref().map(|s| s.count as f64).unwrap_or(0.0)} /></dd>
                </div>
                <div style="--i:1">
                    <dt>{ "de tracé" }</dt>
                    <dd><Compteur valeur={stats.as_ref().map(|s| s.total_km).unwrap_or(0.0)}
                                  decimales={1} suffixe=" km" /></dd>
                </div>
                <div style="--i:2">
                    <dt>{ "en direct" }</dt>
                    <dd><Compteur valeur={n_direct as f64} /></dd>
                </div>
            </dl>

            <section class="panneaux">
                <article class="panneau" data-mvt="monte">
                    <h2>{ "Sur la route" }</h2>
                    <div class="rangee">
                        <button type="button" data-aimant=""
                                class={classes!(etat.partage.then_some("actif"))}
                                onclick={bascule_partage}>
                            <span>
                                { if etat.partage { "🟢 Partage en direct…" } else { "📍 Partager ma position" } }
                            </span>
                        </button>
                        if n_direct > 0 {
                            <span class="badge">{ format!("{n_direct} en direct") }</span>
                        }
                    </div>
                    <div class="signalements">
                        { for CATEGORIES.iter().enumerate().map(|(i, c)| {
                            let c = *c;
                            let signale = signale.clone();
                            html! {
                                <button key={c.id} type="button" class="signaler" style={format!("--i:{i}")}
                                        onclick={Callback::from(move |_| signale.emit(c))}>
                                    <span class="signaler-emoji" aria-hidden="true">{ c.emoji }</span>
                                    { c.nom }
                                </button>
                            }
                        }) }
                    </div>
                </article>

                <article class="panneau" data-mvt="monte">
                    <h2>{ "Itinéraire" }</h2>
                    <p class="indice">{ "Posez des positions sur la carte, puis reliez-les." }</p>
                    <div class="rangee">
                        <label class="vitesse">
                            { "Vitesse" }
                            <input type="number" min="1" max="300" inputmode="numeric"
                                   value={vitesse.to_string()}
                                   oninput={{
                                       let vitesse = vitesse.clone();
                                       Callback::from(move |e: InputEvent| {
                                           let v = e.target_unchecked_into::<HtmlInputElement>()
                                               .value_as_number();
                                           vitesse.set(if v.is_finite() && v >= 1.0 { v } else { 1.0 });
                                       })
                                   }} />
                            { "km/h" }
                        </label>
                        <button type="button" data-aimant="" onclick={calcule}
                                disabled={etat.positions.len() < 2}>
                            <span>{ "Itinéraire complet" }</span>
                        </button>
                        <a class="bouton secondaire" href={api::url_gpx(&jeton)}
                           target="_blank" rel="noopener">{ "Exporter GPX" }</a>
                        <label class="bouton secondaire">
                            { "Importer GPX" }
                            <input type="file" accept=".gpx,application/gpx+xml,text/xml"
                                   class="visuel-cache" onchange={importe} />
                        </label>
                    </div>

                    if let Some((r, v)) = etat.itineraire.clone() {
                        <p class="resultat-itineraire" key={format!("{}-{v}", r.total_km)}>
                            <strong>{ geo::distance_courte(r.total_km * 1000.0) }</strong>
                            { format!(" · {} à {v:.0} km/h", geo::duree_courte(r.duration_min)) }
                        </p>
                    }

                    <ol class="liste">
                        { for etat.positions.iter().enumerate().map(|(i, q)| {
                            let id = q.id;
                            let en_edition = edition.as_ref().filter(|(e, _)| *e == id).map(|(_, t)| t.clone());
                            html! {
                                <li key={id} style={format!("--i:{}", i.min(8))}>
                                    <span class="numero" aria-hidden="true">{ i + 1 }</span>
                                    if let Some(texte) = en_edition {
                                        <form class="renommer" onsubmit={renomme.clone()}>
                                            <input aria-label="Nouveau nom" value={texte} maxlength="120"
                                                   oninput={{
                                                       let edition = edition.clone();
                                                       Callback::from(move |e: InputEvent| {
                                                           let v = e.target_unchecked_into::<HtmlInputElement>().value();
                                                           edition.set(Some((id, v)));
                                                       })
                                                   }} />
                                            <button type="submit" class="petit">{ "OK" }</button>
                                            <button type="button" class="petit secondaire"
                                                    onclick={{
                                                        let edition = edition.clone();
                                                        Callback::from(move |_| edition.set(None))
                                                    }}>{ "Annuler" }</button>
                                        </form>
                                    } else {
                                        <span class="etiquette">
                                            <strong>{ &q.label }</strong>
                                            <em>{ format!("{:.4}, {:.4}", q.lat, q.lon) }</em>
                                        </span>
                                        <span class="actions">
                                            <button type="button" class="petit secondaire"
                                                    onclick={{
                                                        let edition = edition.clone();
                                                        let label = q.label.clone();
                                                        Callback::from(move |_| edition.set(Some((id, label.clone()))))
                                                    }}>{ "Renommer" }</button>
                                            <button type="button" class="petit danger"
                                                    onclick={{
                                                        let supprime = supprime.clone();
                                                        Callback::from(move |_| supprime.emit(id))
                                                    }}>{ "Supprimer" }</button>
                                        </span>
                                    }
                                </li>
                            }
                        }) }
                        if etat.positions.is_empty() {
                            <li class="vide">{ "Aucune position pour l'instant." }</li>
                        }
                    </ol>
                </article>

                // Quatorze voitures, c'est un mur d'images : replié par
                // défaut, l'avatar courant en montre déjà la couleur en haut.
                <details class="panneau avatars" data-mvt="monte">
                    <summary>
                        <h2>{ "Mon avatar" }</h2>
                        <img src={res.avatar(&avatar)} alt="" />
                    </summary>
                    <div class="grille-avatars">
                        { for AVATARS.iter().enumerate().map(|(i, a)| {
                            let choisit = choisit_avatar.clone();
                            let id = a.id;
                            html! {
                                <button key={id} type="button" title={a.nom} aria-label={a.nom}
                                        aria-pressed={(*avatar == id).to_string()}
                                        class={classes!("avatar", (*avatar == id).then_some("choisi"))}
                                        style={format!("--i:{i}")}
                                        onclick={Callback::from(move |_| choisit.emit(id))}>
                                    <img src={res.avatar(id)} alt="" loading="lazy" />
                                </button>
                            }
                        }) }
                    </div>
                </details>
            </section>

            <div class="annonces" aria-live="polite">
                if let Some(a) = etat.annonce.clone() {
                    <p key={a.numero} class={classes!("annonce", a.erreur.then_some("erreur"))}
                       role={if a.erreur { "alert" } else { "status" }}>
                        { a.texte }
                        <button type="button" aria-label="Fermer"
                                onclick={{
                                    let d = etat.dispatcher();
                                    let n = a.numero;
                                    Callback::from(move |_| d.dispatch(Action::EffaceAnnonce(n)))
                                }}>{ "✕" }</button>
                    </p>
                }
            </div>
        </div>
    }
}
