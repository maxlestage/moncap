//! Les adresses expédiées au visiteur, relues dans le rendu.
//!
//! La page a longtemps porté deux liens vers le dépôt — « Voir le projet »
//! et « Code source ». Le dépôt était privé : un visiteur non connecté
//! recevait un 404. Rien ne le signalait, ni la construction, ni une
//! capture, ni un test : **une adresse morte se rend exactement comme une
//! adresse valide**.
//!
//! Ce test lit donc les `href` du HTML réellement produit et n'accepte
//! qu'une liste connue. Il vaut pour tout lien futur, pas seulement pour
//! ceux-là. La liste est calculée : les pages internes viennent de
//! `contenu.rs` — le serveur doit servir chacune, voir la vitrine dans
//! `src/main.rs` —, les externes de `SOURCES`.
//!
//! Il rend les vues exactement comme `construit` les écrit dans `dist/` :
//! une même fonction, `vitrine::vitrine`, rendue en HTML par `active`. Ce
//! que ce test lit est donc ce que le visiteur reçoit, au caractère près.

use std::collections::HashSet;

use vitrine::app::{vitrine, Page, Props, Ressources};
use vitrine::contenu::{Langue, CONNEXION, SOURCES};

/// Rend une page en HTML, hors de tout navigateur.
fn rendu(langue: Langue, page: Page) -> String {
    vitrine(&Props {
        langue,
        page,
        res: Ressources {
            logo: "/vitrine/logo.webp".into(),
        },
        annee: 2026,
    })
    .render()
}

/// Les neuf pages, dans les trois langues.
fn toutes() -> Vec<(String, String)> {
    let mut v = Vec::new();
    for l in Langue::TOUTES {
        for (page, nom) in [
            (Page::Accueil, "accueil"),
            (Page::Mentions, "mentions"),
            (Page::Confidentialite, "confidentialité"),
        ] {
            v.push((format!("{} {nom}", l.code()), rendu(l, page)));
        }
    }
    v
}

/// Les `href` d'un document.
fn adresses(html: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut reste = html;
    while let Some(i) = reste.find("href=\"") {
        reste = &reste[i + 6..];
        match reste.find('"') {
            Some(j) => {
                v.push(reste[..j].to_string());
                reste = &reste[j + 1..];
            }
            None => break,
        }
    }
    v
}

fn autorisees() -> HashSet<String> {
    let mut a: HashSet<String> = HashSet::new();
    a.insert(CONNEXION.to_string());
    // La flèche de défilement reste sur la page : son ancre de repli vise le
    // bandeau, le reste de son intelligence est dans `fleche.rs`.
    a.insert("#haut".to_string());
    for l in Langue::TOUTES {
        let c = l.contenu();
        a.insert(c.chemin.to_string());
        a.insert(c.mentions.chemin.to_string());
        a.insert(c.confidentialite.chemin.to_string());
        // Les ancres du pied visent la page principale de la langue.
        for ancre in c.pied.ancres {
            a.insert(format!("{}#{}", c.chemin, ancre.id));
        }
    }
    for s in SOURCES {
        a.insert(s.url.to_string());
    }
    a
}

#[test]
fn les_pages_ne_pointent_que_vers_des_adresses_que_ce_serveur_sert() {
    let permises = autorisees();
    for (quoi, html) in toutes() {
        let liens = adresses(&html);
        assert!(!liens.is_empty(), "{quoi} : aucun lien, le rendu a échoué");
        for lien in liens {
            assert!(
                permises.contains(&lien),
                "{quoi} : « {lien} » n'est servi par personne"
            );
        }
    }
}

/// Chaque page doit pouvoir mener aux deux autres langues : c'est le seul
/// chemin vers une traduction, et il n'existe qu'au pied et au bandeau.
#[test]
fn chaque_page_mene_aux_autres_langues() {
    for (quoi, html) in toutes() {
        let liens = adresses(&html);
        let code = quoi.split(' ').next().unwrap_or("");
        for autre in Langue::TOUTES.iter().filter(|l| l.code() != code) {
            assert!(
                liens.iter().any(|h| h == autre.chemin()),
                "{quoi} : pas de chemin vers {}",
                autre.code()
            );
        }
    }
}

/// Les pages légales doivent rester joignables depuis n'importe où : c'est
/// une obligation, pas un confort.
#[test]
fn le_legal_est_joignable_depuis_chaque_page() {
    for (quoi, html) in toutes() {
        let liens = adresses(&html);
        let code = quoi.split(' ').next().unwrap_or("");
        let l = Langue::depuis_code(code).expect("langue connue");
        let c = l.contenu();
        for chemin in [c.mentions.chemin, c.confidentialite.chemin] {
            assert!(
                liens.iter().any(|h| h == chemin),
                "{quoi} : {chemin} injoignable"
            );
        }
    }
}

/// Le texte de la page doit être dans la langue de la page. Un bloc recopié
/// d'une langue à l'autre se verrait ici, et nulle part ailleurs.
#[test]
fn chaque_page_est_dans_sa_langue() {
    for l in Langue::TOUTES {
        let html = rendu(l, Page::Accueil);
        let c = l.contenu();
        assert!(
            html.contains(c.chapeau),
            "{} : le chapeau de la langue manque",
            l.code()
        );
        // Et surtout : aucun chapeau d'une autre langue.
        for autre in Langue::TOUTES.iter().filter(|a| **a != l) {
            assert!(
                !html.contains(autre.contenu().chapeau),
                "{} : le chapeau de {} s'est invité",
                l.code(),
                autre.code()
            );
        }
    }
}

/// Les maquettes doivent dire ce que l'application dit, dans la langue de la
/// page. C'est la raison d'être de `appstrings.rs`.
#[test]
fn les_maquettes_parlent_la_langue_de_la_page() {
    use vitrine::appstrings::app;
    for l in Langue::TOUTES {
        let html = rendu(l, Page::Accueil);
        for cle in ["Voiture", "Démarrer", "Quitter"] {
            let attendu = app(l, cle);
            assert!(
                html.contains(attendu),
                "{} : « {attendu} » absent de la maquette",
                l.code()
            );
        }
    }
}

/// Le comparateur ne doit jamais laisser croire qu'il propose moins
/// d'itinéraires qu'annoncé : la maquette en montre trois, et la ligne
/// « ⌄ … » dit qu'il y en a d'autres.
#[test]
fn la_maquette_ne_dement_pas_le_chiffre_annonce() {
    for l in Langue::TOUTES {
        let html = rendu(l, Page::Accueil);
        let c = l.contenu();
        assert!(
            html.contains(c.reste),
            "{} : la mention du reste manque",
            l.code()
        );
    }
}
