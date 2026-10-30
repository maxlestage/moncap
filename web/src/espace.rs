//! Mon espace : se nommer et partager sa position — sans compte ni serveur.
//!
//! Tout reste dans le navigateur, par Memoire, l'ORM de Paquets :
//!   - le nom, dans le `localStorage`, par un `Store` (il revient à chaque
//!     visite, et suit d'un onglet à l'autre) ;
//!   - les positions envoyées et reçues, dans une base Memoire rangée dans
//!     **IndexedDB** (`IndexedDbStorage`), ou à défaut dans le
//!     `localStorage` quand le navigateur refuse IndexedDB.
//!
//! Partager, c'est envoyer un lien qui **porte la position** :
//! `…/app#position=lat,lon,heure,nom`. Le fragment après `#` ne part jamais
//! vers un serveur : le destinataire l'ouvre dans son navigateur, la page le
//! lit et le garde dans sa propre base. Rien ne dépend du serveur de
//! l'application, qui peut être arrêté.

use std::rc::Rc;

use paquets::active::prelude::*;
use paquets::orm::web::{LocalStorageDb, Store};
use paquets::orm::{Executor, Model};
use serde::{Deserialize, Serialize};

use crate::localisation;
use crate::navigateur::{self, au_montage, fenetre, lance};

/// Le préfixe du fragment qui porte une position.
pub const FRAGMENT: &str = "#position=";
/// Le fragment de l'espace lui-même.
pub const FRAGMENT_ESPACE: &str = "#espace";
/// Longueur maximale d'un nom.
const NOM_MAX: usize = 60;

/// Ce que l'espace garde dans le `localStorage`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Profil {
    pub nom: String,
}

/// Le profil de ce navigateur.
pub fn profil() -> Store<Profil> {
    Store::new("moncap.profil")
}

/// Une position envoyée ou reçue, gardée dans la base du navigateur.
#[paquets::model(table = "partages")]
pub struct PositionPartagee {
    pub id: i64,
    pub nom: String,
    pub lat: f64,
    pub lon: f64,
    /// Le moment du partage, en millisecondes Unix.
    pub quand: i64,
    /// Reçue d'un lien, ou envoyée d'ici.
    pub recue: bool,
}

/// Un nom propre à partager : sans espaces autour ni caractères de contrôle,
/// borné.
pub fn nettoie_nom(nom: &str) -> String {
    nom.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(NOM_MAX)
        .collect()
}

/// Encode une composante d'adresse : tout sauf lettres, chiffres et `-_.~`
/// passe en `%XX` (UTF-8).
fn encode(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    for octet in texte.bytes() {
        if octet.is_ascii_alphanumeric() || b"-_.~".contains(&octet) {
            sortie.push(octet as char);
        } else {
            sortie.push_str(&format!("%{octet:02X}"));
        }
    }
    sortie
}

/// L'inverse d'[`encode`] ; `None` si une séquence est mal formée.
fn decode(texte: &str) -> Option<String> {
    let octets = texte.as_bytes();
    let mut sortie = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        match octets[i] {
            b'%' => {
                let hex = std::str::from_utf8(octets.get(i + 1..i + 3)?).ok()?;
                sortie.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                sortie.push(b' ');
                i += 1;
            }
            o => {
                sortie.push(o);
                i += 1;
            }
        }
    }
    String::from_utf8(sortie).ok()
}

/// Le lien qui porte une position, sur la page de l'application de `origine`
/// (`https://exemple.fr`).
pub fn lien(origine: &str, nom: &str, lat: f64, lon: f64, quand: i64) -> String {
    format!(
        "{}/app{FRAGMENT}{lat:.6},{lon:.6},{quand},{}",
        origine.trim_end_matches('/'),
        encode(&nettoie_nom(nom))
    )
}

/// Une position lue dans un fragment `#position=…` ; `None` s'il n'en porte
/// pas, ou une illisible.
pub fn lis_fragment(fragment: &str) -> Option<PositionPartagee> {
    let reste = fragment.strip_prefix(FRAGMENT)?;
    let mut parties = reste.splitn(4, ',');
    let lat: f64 = parties.next()?.parse().ok()?;
    let lon: f64 = parties.next()?.parse().ok()?;
    let quand: i64 = parties.next()?.parse().ok()?;
    let nom = nettoie_nom(&decode(parties.next().unwrap_or(""))?);
    let valide = lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon);
    valide.then_some(PositionPartagee {
        id: 0,
        nom,
        lat,
        lon,
        quand,
        recue: true,
    })
}

/// « il y a 5 min », « il y a 2 h », « il y a 3 j ».
pub fn depuis(quand_ms: i64, maintenant_ms: f64) -> String {
    let minutes = ((maintenant_ms - quand_ms as f64) / 60_000.0).max(0.0);
    if minutes < 1.0 {
        "à l'instant".into()
    } else if minutes < 60.0 {
        format!("il y a {} min", minutes.floor())
    } else if minutes < 48.0 * 60.0 {
        format!("il y a {} h", (minutes / 60.0).floor())
    } else {
        format!("il y a {} j", (minutes / 1440.0).floor())
    }
}

/// La base des positions : IndexedDB, à défaut le `localStorage`.
async fn ouvre_base() -> Option<Rc<LocalStorageDb>> {
    let db = match paquets::IndexedDbStorage::open("moncap").await {
        Ok(stockage) => LocalStorageDb::open_with("partages", stockage).ok()?,
        Err(_) => LocalStorageDb::open("moncap.partages").ok()?,
    };
    db.create_table_if_not_exists::<PositionPartagee>()
        .await
        .ok()?;
    Some(Rc::new(db))
}

async fn toutes(db: &LocalStorageDb) -> Vec<PositionPartagee> {
    PositionPartagee::query()
        .order_by(PositionPartagee::QUAND.desc())
        .order_by(PositionPartagee::ID.desc())
        .all(db)
        .await
        .unwrap_or_default()
}

/// Range une position, sauf si la même (même heure, mêmes coordonnées) y est
/// déjà : rouvrir un lien reçu ne le double pas.
async fn garde(db: &LocalStorageDb, mut p: PositionPartagee) {
    let deja = PositionPartagee::query()
        .filter(PositionPartagee::QUAND.eq(p.quand))
        .filter(PositionPartagee::LAT.eq(p.lat))
        .filter(PositionPartagee::LON.eq(p.lon))
        .count(db)
        .await
        .unwrap_or(0);
    if deja == 0 {
        let _ = p.insert(db).await;
    }
}

/// Attend une promesse du navigateur sans s'occuper de son issue : un partage
/// annulé ou un presse-papiers refusé n'est pas une erreur — le lien reste
/// affiché, à copier à la main.
fn sans_suite(promesse: wasm_bindgen::JsValue) {
    if let Ok(p) = wasm_bindgen::JsCast::dyn_into::<js_sys::Promise>(promesse) {
        lance(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(p).await;
        });
    }
}

/// Envoie un lien : la feuille de partage du système quand il y en a une,
/// sinon le presse-papiers. Rend vrai si quelque chose a été proposé.
fn envoie(titre: &str, lien: &str) -> bool {
    use js_sys::{Function, Object, Reflect};
    use wasm_bindgen::{JsCast, JsValue};

    let Some(navigateur) = fenetre().map(|f| f.navigator()) else {
        return false;
    };
    if let Ok(share) = Reflect::get(&navigateur, &"share".into()) {
        if let Some(share) = share.dyn_ref::<Function>() {
            let donnees = Object::new();
            let _ = Reflect::set(&donnees, &"title".into(), &titre.into());
            let _ = Reflect::set(&donnees, &"url".into(), &lien.into());
            return share.call1(&navigateur, &donnees).map(sans_suite).is_ok();
        }
    }
    let presse_papiers = Reflect::get(&navigateur, &"clipboard".into()).unwrap_or(JsValue::NULL);
    if let Ok(ecris) = Reflect::get(&presse_papiers, &"writeText".into()) {
        if let Some(ecris) = ecris.dyn_ref::<Function>() {
            return ecris
                .call1(&presse_papiers, &lien.into())
                .map(sans_suite)
                .is_ok();
        }
    }
    false
}

/// L'origine de la page (`https://exemple.fr`).
fn origine() -> String {
    fenetre()
        .and_then(|f| f.location().origin().ok())
        .unwrap_or_default()
}

/// Le fragment courant de l'adresse.
pub fn fragment() -> String {
    fenetre()
        .and_then(|f| f.location().hash().ok())
        .unwrap_or_default()
}

/// Ce que l'espace reçoit.
pub struct Proprietes {
    /// Une position reçue par lien, à montrer et à garder.
    pub recue: Option<PositionPartagee>,
    /// Revenir à l'application.
    pub retour: Rc<dyn Fn()>,
}

/// L'espace.
pub fn espace(p: Proprietes) -> Node {
    let profil = profil();
    let nom = use_state(profil.get(|s| s.nom.clone()));
    let base = use_state(None::<Rc<LocalStorageDb>>);
    let historique = use_state(Rc::new(Vec::<PositionPartagee>::new()));
    let avis = use_state(None::<(String, bool)>);
    let occupe = use_state(false);
    let dernier_lien = use_state(None::<String>);

    let recharge = move || {
        if let Some(db) = untrack(|| base.get()) {
            lance(async move {
                let liste = toutes(&db).await;
                if historique.is_alive() {
                    historique.set(Rc::new(liste));
                }
            });
        }
    };

    {
        let recue = p.recue.clone();
        au_montage(move || {
            lance(async move {
                let Some(db) = ouvre_base().await else {
                    avis.set(Some((
                        "Ce navigateur refuse de garder des données : rien ne sera retenu.".into(),
                        true,
                    )));
                    return;
                };
                if let Some(r) = recue {
                    garde(&db, r).await;
                }
                let liste = toutes(&db).await;
                if base.is_alive() {
                    base.set(Some(db));
                    historique.set(Rc::new(liste));
                }
            });
        });
    }

    let ecris_nom = {
        let profil = profil.clone();
        move |e: Event| {
            let v = e.value();
            profil.update(|s| s.nom = nettoie_nom(&v));
            nom.set(v);
        }
    };

    let partage = {
        let profil = profil.clone();
        move |_: Event| {
            if untrack(|| occupe.get()) {
                return;
            }
            occupe.set(true);
            avis.set(None);
            let qui = profil.get(|s| s.nom.clone());
            let lance_ok = localisation::une_fois(
                move |c| {
                    let quand = navigateur::maintenant_ms() as i64;
                    let l = lien(&origine(), &qui, c.lat, c.lon, quand);
                    let titre = if qui.is_empty() {
                        "Ma position".to_string()
                    } else {
                        format!("Position de {qui}")
                    };
                    let propose = envoie(&titre, &l);
                    avis.set(Some((
                        if propose {
                            "Lien prêt : envoie-le à qui tu veux.".into()
                        } else {
                            "Copie le lien ci-dessous et envoie-le.".into()
                        },
                        false,
                    )));
                    dernier_lien.set(Some(l));
                    occupe.set(false);
                    if let Some(db) = untrack(|| base.get()) {
                        lance(async move {
                            garde(
                                &db,
                                PositionPartagee {
                                    id: 0,
                                    nom: qui,
                                    lat: c.lat,
                                    lon: c.lon,
                                    quand,
                                    recue: false,
                                },
                            )
                            .await;
                            recharge();
                        });
                    }
                },
                move |m| {
                    avis.set(Some((m.to_string(), true)));
                    occupe.set(false);
                },
            );
            if !lance_ok {
                avis.set(Some((
                    "Ce navigateur ne donne pas de position.".into(),
                    true,
                )));
                occupe.set(false);
            }
        }
    };

    let oublie = move |id: i64| {
        if let Some(db) = untrack(|| base.get()) {
            lance(async move {
                let _ = PositionPartagee::delete_many()
                    .filter(PositionPartagee::ID.eq(id))
                    .execute(&*db)
                    .await;
                recharge();
            });
        }
    };

    let carte_recue: Option<Node> = p.recue.as_ref().map(|r| {
        let qui = if r.nom.is_empty() {
            "Quelqu'un".to_string()
        } else {
            r.nom.clone()
        };
        section()
            .class("espace-recue")
            .child(h2().text(format!("📍 {qui} est ici")))
            .child(p_().class("espace-doux").text(format!(
                "{:.5}, {:.5} · {}",
                r.lat,
                r.lon,
                depuis(r.quand, navigateur::maintenant_ms())
            )))
            .child(liens_carte(r.lat, r.lon))
            .into()
    });

    let retour = p.retour.clone();
    div()
        .class("espace")
        .child(
            header()
                .class("espace-tete")
                .child(h1().text("Mon espace"))
                .child(
                    button()
                        .attr("type", "button")
                        .class("lien")
                        .on_click(move |_| retour())
                        .text("← Retour"),
                ),
        )
        .children(carte_recue)
        .child(
            section()
                .class("espace-moi")
                .child(
                    label()
                        .class("champ")
                        .child(span().text("Mon nom"))
                        .child(
                            input()
                                .attr("type", "text")
                                .attr("placeholder", "Comment les autres te voient")
                                .attr("maxlength", NOM_MAX.to_string())
                                .attr("autocomplete", "nickname")
                                .attr_dyn("value", move || nom.get())
                                .on_input(ecris_nom),
                        ),
                )
                .child(
                    p_().class("espace-doux").text(
                        "Gardé dans ce navigateur seulement. Il accompagne les positions que tu partages.",
                    ),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .class("principal")
                        .attr("data-onde", "")
                        .bool_attr("disabled", move || occupe.get())
                        .on_click(partage)
                        .text_dyn(move || {
                            if occupe.get() {
                                "Position en cours…"
                            } else {
                                "Partager ma position"
                            }
                            .to_string()
                        }),
                )
                .children_dyn(move || {
                    let a = avis.get();
                    untrack(|| {
                        a.map(|(texte, erreur)| {
                            p_().class(if erreur { "porte-message erreur" } else { "porte-message info" })
                                .attr("role", if erreur { "alert" } else { "status" })
                                .text(texte)
                                .into()
                        })
                        .into_iter()
                        .collect()
                    })
                })
                .children_dyn(move || {
                    let l = dernier_lien.get();
                    untrack(|| {
                        l.map(|l| {
                            input()
                                .class("espace-lien")
                                .attr("readonly", "")
                                .attr("aria-label", "Lien à partager")
                                .attr("value", l)
                                .into()
                        })
                        .into_iter()
                        .collect()
                    })
                }),
        )
        .child(
            section()
                .class("espace-historique")
                .child(h2().text("Positions partagées"))
                .children_dyn(move || {
                    let liste = historique.get();
                    if liste.is_empty() {
                        return vec![p_()
                            .class("espace-doux")
                            .text("Rien encore : celles que tu envoies et reçois restent ici.")
                            .into()];
                    }
                    let maintenant = navigateur::maintenant_ms();
                    vec![ul()
                        .children(liste.iter().map(|r| {
                            let id = r.id;
                            let qui = match (r.recue, r.nom.is_empty()) {
                                (true, true) => "Reçue".to_string(),
                                (true, false) => format!("De {}", r.nom),
                                (false, _) => "Envoyée".to_string(),
                            };
                            li().child(
                                div()
                                    .child(strong().text(qui))
                                    .child(span().class("espace-doux").text(format!(
                                        " · {}",
                                        depuis(r.quand, maintenant)
                                    ))),
                            )
                            .child(liens_carte(r.lat, r.lon))
                            .child(
                                button()
                                    .attr("type", "button")
                                    .class("lien")
                                    .attr("aria-label", "Oublier cette position")
                                    .on_click(move |_| oublie(id))
                                    .text("Oublier"),
                            )
                        }))
                        .into()]
                }),
        )
        .into()
}

/// `p` est pris par le paramètre des propriétés : l'élément `<p>` passe par
/// ici, comme dans la porte et la carte.
fn p_() -> Element {
    el("p")
}

/// Où voir un point : OpenStreetMap, et Plans sur les appareils Apple.
fn liens_carte(lat: f64, lon: f64) -> Node {
    div()
        .class("espace-liens")
        .child(
            a().attr(
                "href",
                format!("https://www.openstreetmap.org/?mlat={lat:.6}&mlon={lon:.6}#map=17/{lat:.6}/{lon:.6}"),
            )
            .attr("target", "_blank")
            .attr("rel", "noopener")
            .text("OpenStreetMap"),
        )
        .child(
            a().attr("href", format!("https://maps.apple.com/?ll={lat:.6},{lon:.6}&q=Position"))
                .attr("target", "_blank")
                .attr("rel", "noopener")
                .text("Plans"),
        )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_lien_porte_la_position_et_le_nom() {
        let l = lien(
            "https://moncap.fr/",
            "Zoé & Max",
            44.837_8,
            -0.579_2,
            1_700_000_000_000,
        );
        assert_eq!(
            l,
            "https://moncap.fr/app#position=44.837800,-0.579200,1700000000000,Zo%C3%A9%20%26%20Max"
        );
        let fragment = &l[l.find('#').unwrap()..];
        let p = lis_fragment(fragment).unwrap();
        assert_eq!(p.nom, "Zoé & Max");
        assert_eq!(
            (p.lat, p.lon, p.quand),
            (44.8378, -0.5792, 1_700_000_000_000)
        );
        assert!(p.recue);
    }

    #[test]
    fn un_fragment_illisible_ne_donne_rien() {
        for faux in [
            "",
            "#espace",
            "#position=",
            "#position=91,0,1,x",
            "#position=1,200,1,x",
            "#position=a,b,c,d",
            "#position=1,2,3,%ZZ",
            "#position=NaN,2,3,x",
        ] {
            assert!(lis_fragment(faux).is_none(), "{faux}");
        }
        // Sans nom : une position anonyme.
        assert_eq!(lis_fragment("#position=1,2,3").unwrap().nom, "");
    }

    #[test]
    fn le_nom_est_nettoye_et_borne() {
        assert_eq!(nettoie_nom("  Max\n "), "Max");
        assert_eq!(nettoie_nom(&"é".repeat(100)).chars().count(), NOM_MAX);
    }

    #[test]
    fn depuis_quand() {
        let t = 10_000_000.0;
        assert_eq!(depuis(10_000_000, t), "à l'instant");
        assert_eq!(depuis(10_000_000 - 5 * 60_000, t), "il y a 5 min");
        assert_eq!(depuis(10_000_000 - 3 * 3_600_000, t), "il y a 3 h");
        assert_eq!(depuis(10_000_000 - 72 * 3_600_000, t), "il y a 3 j");
    }

    #[test]
    fn l_espace_se_rend() {
        let html = espace(Proprietes {
            recue: lis_fragment("#position=44.8,-0.5,0,Lou"),
            retour: Rc::new(|| {}),
        })
        .render();
        assert!(html.contains("Mon espace"), "{html}");
        assert!(html.contains("Lou est ici"));
        assert!(html.contains("Mon nom"));
        assert!(html.contains("Partager ma position"));
        assert!(html.contains("openstreetmap.org/?mlat=44.800000"));
    }
}
