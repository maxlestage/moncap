//! Fabrique le site : les neuf coquilles HTML, le WebAssembly, la feuille de
//! style, les images, le plan du site et les consignes aux robots.
//!
//! Remplace l'ancien `build.tsx`. Il compile en natif et partage le contenu,
//! les types et le module de référencement avec l'application qui tourne
//! dans le navigateur : le texte n'existe qu'une fois, et le compilateur
//! garantit que les deux en voient la même version.
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
    let mut pages: Vec<(String, usize)> = Vec::new();
    for l in Langue::TOUTES {
        let c = l.contenu();

        let tete = seo::tete(c, l, &feuille, &logo, &favicon);
        let page = coquille(l, "accueil", &tete, &seo::repli(c, l), &wasm, &colle, &logo);
        pages.push((
            c.chemin.to_string(),
            ecrit_page(&sortie.dossier, c.chemin, &page)?,
        ));

        for (g, pg, nom) in [
            (Groupe::Mentions, &c.mentions, "mentions"),
            (
                Groupe::Confidentialite,
                &c.confidentialite,
                "confidentialite",
            ),
        ] {
            let tete = seo::tete_legale(c, l, pg, g, &feuille, &favicon);
            // Le repli d'une page légale est son texte même : il est court,
            // il doit rester lisible sans script, et c'est justement la page
            // qu'on vient chercher quand on en a besoin.
            let repli = repli_legale(pg);
            let page = coquille(l, nom, &tete, &repli, &wasm, &colle, &logo);
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
    println!("rendu par le navigateur : le corps part vide, la tête est complète,");
    println!("et chaque page garde un repli <noscript> lisible sans WebAssembly.");
    Ok(())
}

/// La coquille HTML d'une page.
///
/// Le corps ne contient que le rideau, le point de montage et le repli :
/// c'est le WebAssembly qui peint la page. Le rideau couvre l'écran pendant
/// qu'il arrive — une marque plutôt qu'une page blanche — et l'application
/// le lève une fois montée. Sans script, il ne s'affiche pas. La tête, elle, est complète — titres, Open
/// Graph, données structurées, alternates — parce qu'un aperçu de lien ne
/// lit rien d'autre, et qu'il n'a aucune raison d'attendre un moteur.
///
/// La langue et la page voyagent en attributs `data-` plutôt qu'en code
/// compilé : un seul WebAssembly sert les neuf pages, et le serveur peut
/// changer la langue d'une adresse sans recompiler quoi que ce soit.
fn coquille(
    l: Langue,
    page: &str,
    tete: &str,
    repli: &str,
    wasm: &str,
    colle: &str,
    logo: &str,
) -> String {
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
    <div id="racine" data-langue="{code}" data-page="{page}" data-logo="{logo}"></div>
{repli}
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

/// Le repli d'une page légale : son texte, en entier.
fn repli_legale(pg: &vitrine::contenu::PageLegale) -> String {
    let mut s = String::from("<noscript><div class=\"dedans juridique repli\">");
    s.push_str(&format!("<h1>{}</h1>", seo::texte(pg.titre)));
    for sec in pg.sections {
        s.push_str(&format!("<h2>{}</h2>", seo::texte(sec.titre)));
        for para in sec.corps {
            s.push_str(&format!("<p>{}</p>", seo::texte(para)));
        }
    }
    s.push_str("</div></noscript>");
    s
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

    /// Le rideau couvre la page pendant l'arrivée du WebAssembly, et c'est
    /// l'application qui le lève. Sans script, personne ne le lèverait : il
    /// ne doit alors pas s'afficher du tout, sans quoi le repli `<noscript>`
    /// — tout ce qu'un tel visiteur peut lire — resterait caché dessous.
    #[test]
    fn sans_script_le_rideau_ne_cache_rien() {
        let html = coquille(
            Langue::Fr,
            "accueil",
            "",
            "<noscript>repli</noscript>",
            "/v/a.wasm",
            "/v/a.js",
            "/v/l.webp",
        );
        assert!(html.contains(r#"id="rideau""#));
        let tete = html.split("</head>").next().unwrap_or_default();
        assert!(
            tete.contains("<noscript><style>.rideau{display:none}</style></noscript>"),
            "la tête doit masquer le rideau quand le script ne tourne pas"
        );
    }
}
