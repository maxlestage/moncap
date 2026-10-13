//! Fabrique le site : les neuf pages HTML, le WebAssembly, la feuille de
//! style, les images, le plan du site et les consignes aux robots.
//!
//! Remplace l'ancien `build.tsx`. Il compile en natif et **rend chaque page
//! en entier** avec les vues `active` de `src/app.rs` : la tête et le corps
//! partent complets, le WebAssembly ne fait plus que réhydrater les îlots et
//! animer. Le texte n'existe qu'une fois, et ce que le navigateur réhydrate
//! est ce que le rendu a écrit — mêmes fonctions, mêmes propriétés.
//!
//! Chaque fichier servi porte l'empreinte de son contenu dans son nom. C'est
//! la seule façon de corriger un cache déjà pris : un en-tête ne s'applique
//! qu'à une réponse que le navigateur demande, tandis qu'une adresse neuve
//! n'a jamais d'entrée en cache.
//!
//! Les outils eux-mêmes — empreinte, allègement de la feuille, chaîne
//! WebAssembly — vivent dans la crate `fabrique`, partagée avec le front web.

use std::fs;
use std::path::{Path, PathBuf};

use fabrique::{ko, nom_fichier, Sortie};
use vitrine::contenu::Langue;
use vitrine::seo::{self, Groupe};
use vitrine::{vitrine, Page, Props, Ressources};

/// Le dossier produit.
const SORTIE: &str = "dist";
/// Préfixe sous lequel le serveur monte le dossier construit.
const BASE: &str = "/vitrine";

fn main() {
    if let Err(e) = construit() {
        eprintln!("échec de la construction : {e}");
        std::process::exit(1);
    }
}

fn construit() -> Result<(), String> {
    let racine = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sortie = Sortie::neuve(racine.join(SORTIE), BASE)?;

    // --- Feuille de style ------------------------------------------------
    let css_source = fs::read_to_string(racine.join("src/styles.css"))
        .map_err(|e| format!("lecture de la feuille : {e}"))?;
    let css = fabrique::minifie_css(&css_source);
    let feuille = sortie.ecrit_empreinte("styles", "css", css.as_bytes())?;

    // --- Images ----------------------------------------------------------
    let logo = sortie.copie_empreinte(&racine.join("assets/logo.webp"))?;
    let favicon = sortie.copie_empreinte(&racine.join("assets/favicon.png"))?;

    // --- WebAssembly -----------------------------------------------------
    let (wasm, colle) = fabrique::construit_wasm(&racine, &sortie)?;

    // --- Pages -----------------------------------------------------------
    let res = Ressources { logo: logo.clone() };
    let annee = annee_courante();
    let mut pages: Vec<(String, usize)> = Vec::new();
    for l in Langue::TOUTES {
        let c = l.contenu();
        let corps = |page| {
            vitrine(&Props {
                langue: l,
                page,
                res: res.clone(),
                annee,
            })
            .render()
        };

        let tete = seo::tete(c, l, &feuille, &logo, &favicon);
        let page = coquille(l, &tete, &corps(Page::Accueil), &wasm, &colle, &logo);
        pages.push((
            c.chemin.to_string(),
            ecrit_page(&sortie.dossier, c.chemin, &page)?,
        ));

        for (g, pg, quoi) in [
            (Groupe::Mentions, &c.mentions, Page::Mentions),
            (
                Groupe::Confidentialite,
                &c.confidentialite,
                Page::Confidentialite,
            ),
        ] {
            let tete = seo::tete_legale(c, l, pg, g, &feuille, &favicon);
            let page = coquille(l, &tete, &corps(quoi), &wasm, &colle, &logo);
            pages.push((
                pg.chemin.to_string(),
                ecrit_page(&sortie.dossier, pg.chemin, &page)?,
            ));
        }
    }

    // --- Référencement ---------------------------------------------------
    fs::write(sortie.dossier.join("sitemap.xml"), seo::sitemap())
        .map_err(|e| format!("plan du site : {e}"))?;
    fs::write(sortie.dossier.join("robots.txt"), seo::robots())
        .map_err(|e| format!("robots : {e}"))?;

    // --- Compte rendu ----------------------------------------------------
    for (chemin, octets) in &pages {
        println!("{chemin:<24} {:>6} Ko", ko(*octets));
    }
    println!();
    for f in [&feuille, &wasm, &colle, &logo] {
        println!("{:<24} {:>6} Ko", nom_fichier(f), ko(sortie.poids(f)));
    }
    println!();
    println!("rendu à la construction : chaque page part complète, tête et corps ;");
    println!("le WebAssembly réhydrate les îlots et anime, il ne peint rien.");
    Ok(())
}

/// La page HTML complète.
///
/// Le corps est le rendu des vues, écrit dans le point de montage : lisible
/// tel quel, sans script, par un visiteur, un robot ou un aperçu de lien. Le
/// rideau le couvre pendant que le WebAssembly arrive — le temps que le
/// moteur de mouvement prenne la main, sans quoi les titres apparaîtraient
/// d'abord immobiles puis rejoueraient leur entrée — et le moteur le lève.
/// Sans script, il ne s'affiche pas : la page est alors simplement immobile.
fn coquille(l: Langue, tete: &str, corps: &str, wasm: &str, colle: &str, logo: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="{code}">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
    <meta name="author" content="Maxime Nathan Lestage" />
{tete}
    <link rel="modulepreload" href="{colle}" />
    <link rel="preload" href="{wasm}" as="fetch" type="application/wasm" crossorigin="anonymous" />
    <noscript><style>.rideau{{display:none}}</style></noscript>
  </head>
  <body>
    <div id="rideau" class="rideau" aria-hidden="true">
      <img class="logo" src="{logo}" width="88" height="88" alt="" />
      <span class="rideau-barre"><span></span></span>
    </div>
    <div id="racine">{corps}</div>
    <script type="module">
      import init from "{colle}";
      init({{ module_or_path: "{wasm}" }});
    </script>
  </body>
</html>
"#,
        code = l.code(),
    )
}

/// L'année courante, en temps universel, sans dépendance de calendrier :
/// le nombre de jours depuis 1970 converti en date civile (l'algorithme
/// `civil_from_days` de Howard Hinnant). Le navigateur la remplace de toute
/// façon par la sienne ; celle-ci sert au HTML de départ.
fn annee_courante() -> i32 {
    let secondes = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    annee_du_jour((secondes / 86_400) as i64)
}

fn annee_du_jour(jours: i64) -> i32 {
    let z = jours + 719_468;
    let ere = z.div_euclid(146_097);
    let jde = z.rem_euclid(146_097);
    let ade = (jde - jde / 1460 + jde / 36_524 - jde / 146_096) / 365;
    let jda = jde - (365 * ade + ade / 4 - ade / 100);
    let mp = (5 * jda + 2) / 153;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 };
    (ade + ere * 400 + i64::from(mois <= 2)) as i32
}

/// Le français occupe la racine ; tout le reste, un sous-dossier.
fn ecrit_page(sortie: &Path, chemin: &str, page: &str) -> Result<usize, String> {
    let fichier = if chemin == "/" {
        sortie.join("index.html")
    } else {
        sortie
            .join(chemin.trim_start_matches('/'))
            .join("index.html")
    };
    if let Some(parent) = fichier.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("création de {parent:?} : {e}"))?;
    }
    fs::write(&fichier, page).map_err(|e| format!("écriture de {fichier:?} : {e}"))?;
    Ok(page.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use paquets::active::html::h2;
    use paquets::active::Node;

    fn page(l: Langue, quoi: Page) -> String {
        let corps = vitrine(&Props {
            langue: l,
            page: quoi,
            res: Ressources {
                logo: "/v/l.webp".into(),
            },
            annee: 2026,
        })
        .render();
        coquille(l, "", &corps, "/v/a.wasm", "/v/a.js", "/v/l.webp")
    }

    /// Le rideau couvre la page pendant l'arrivée du WebAssembly, et c'est
    /// le moteur qui le lève. Sans script, personne ne le lèverait : il ne
    /// doit alors pas s'afficher du tout, sans quoi la page — complète dans
    /// le HTML — resterait cachée dessous.
    #[test]
    fn sans_script_le_rideau_ne_cache_rien() {
        let html = page(Langue::Fr, Page::Accueil);
        assert!(html.contains(r#"id="rideau""#));
        let tete = html.split("</head>").next().unwrap_or_default();
        assert!(
            tete.contains("<noscript><style>.rideau{display:none}</style></noscript>"),
            "la tête doit masquer le rideau quand le script ne tourne pas"
        );
    }

    /// Ce que le test d'intégration du serveur lit dans le HTML brut — sans
    /// base de données, il ne tourne qu'à la main : gardé ici aussi. La page
    /// part complète, ces phrases n'attendent donc aucun script.
    #[test]
    fn le_html_brut_porte_ce_que_le_serveur_verifie() {
        for (l, phrase) in [
            (Langue::Fr, "Comparez, puis roulez"),
            (Langue::Es, "Compara y conduce"),
            (Langue::En, "Compare, then drive"),
        ] {
            let html = page(l, Page::Accueil);
            assert!(html.contains(&format!(r#"<html lang="{}">"#, l.code())));
            assert!(html.contains(phrase), "{phrase} absent");
            assert!(html.contains(r#"href="/app""#), "pas de lien de connexion");
            assert!(html.contains("Maxime Nathan Lestage"), "pas de signature");
            assert!(html.contains(r#"<img class="logo" src="/v/l.webp""#));
            assert!(html.contains(r#"aria-current="true""#));
            for autre in Langue::TOUTES.into_iter().filter(|a| *a != l) {
                assert!(
                    html.contains(&format!(r#"href="{}""#, autre.chemin())),
                    "{} doit mener à {}",
                    l.code(),
                    autre.code()
                );
            }
        }
    }

    /// Chaque sélecteur de langue — bandeau et pied — marque la courante, et
    /// une seule : un lien vers soi n'offre rien.
    #[test]
    fn chaque_selecteur_marque_une_seule_langue() {
        for l in Langue::TOUTES {
            for quoi in [Page::Accueil, Page::Mentions] {
                let html = page(l, quoi);
                let selecteurs = html.matches(r#"<nav class="langues""#).count();
                assert!(selecteurs >= 1);
                assert_eq!(
                    html.matches(r#"aria-current="true""#).count(),
                    selecteurs,
                    "{} : une langue courante par sélecteur",
                    l.code()
                );
            }
        }
    }

    /// Le texte d'une page légale est dans son HTML, en entier : c'est la
    /// page qu'on vient chercher quand on en a besoin, script ou pas.
    #[test]
    fn le_texte_legal_est_dans_le_html() {
        for l in Langue::TOUTES {
            let c = l.contenu();
            let html = page(l, Page::Confidentialite);
            for sec in c.confidentialite.sections {
                let titre = Node::from(h2().text(sec.titre)).render();
                assert!(html.contains(&titre), "{} : {titre} absent", l.code());
            }
        }
    }

    #[test]
    fn l_annee_se_lit_dans_le_nombre_de_jours() {
        assert_eq!(annee_du_jour(0), 1970);
        assert_eq!(annee_du_jour(364), 1970);
        assert_eq!(annee_du_jour(365), 1971);
        // 2024-12-31 et 2025-01-01 : une année bissextile se ferme au bon jour.
        assert_eq!(annee_du_jour(20_088), 2024);
        assert_eq!(annee_du_jour(20_089), 2025);
        assert!(annee_courante() >= 2026);
    }
}
