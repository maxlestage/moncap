//! Fabrique le front web : la coquille HTML, le WebAssembly, la feuille de
//! style et les images, chacun sous un nom empreint de son contenu.
//!
//! Remplace `bun build`. La chaîne est celle de la vitrine — cargo,
//! `wasm-bindgen`, `wasm-opt` —, partagée par la crate `fabrique`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use fabrique::{ko, nom_fichier, Sortie};
use web::catalogue::AVATARS;

/// Le dossier produit.
const SORTIE: &str = "dist";
/// Le préfixe sous lequel le serveur monte ses fichiers (voir `src/main.rs`).
const BASE: &str = "/web";

fn main() {
    if let Err(e) = construit() {
        eprintln!("échec de la construction : {e}");
        std::process::exit(1);
    }
}

fn construit() -> Result<(), String> {
    let racine = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sortie = Sortie::neuve(racine.join(SORTIE), BASE)?;

    let css = fs::read_to_string(racine.join("src/styles.css"))
        .map_err(|e| format!("lecture de la feuille : {e}"))?;
    let feuille =
        sortie.ecrit_empreinte("styles", "css", fabrique::minifie_css(&css).as_bytes())?;

    let logo = sortie.copie_empreinte(&racine.join("assets/logo.webp"))?;
    let favicon = sortie.copie_empreinte(&racine.join("assets/favicon.png"))?;
    let mut avatars = BTreeMap::new();
    for a in AVATARS {
        let chemin = racine.join(format!("assets/avatars/{}.webp", a.id));
        avatars.insert(a.id, sortie.copie_empreinte(&chemin)?);
    }

    let (wasm, colle) = fabrique::construit_wasm(&racine, &sortie)?;

    let ressources = serde_json::json!({ "logo": logo, "avatars": avatars }).to_string();
    let page = coquille(&feuille, &favicon, &logo, &wasm, &colle, &ressources);
    fs::write(sortie.dossier.join("index.html"), &page)
        .map_err(|e| format!("écriture de la coquille : {e}"))?;

    // --- Compte rendu ---------------------------------------------------------
    println!("{:<26} {:>6} Ko", "index.html", ko(page.len()));
    for f in [&feuille, &wasm, &colle, &logo] {
        println!("{:<26} {:>6} Ko", nom_fichier(f), ko(sortie.poids(f)));
    }
    let images: usize = avatars.values().map(|a| sortie.poids(a)).sum();
    println!("{:<26} {:>6} Ko", "avatars (14)", ko(images));
    Ok(())
}

/// Échappe une valeur d'attribut HTML entre guillemets doubles.
fn attribut(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

/// La coquille : la tête, le rideau, le point de montage.
///
/// Le rideau est écrit ici, en HTML et en CSS, et non par le WebAssembly :
/// c'est lui qu'on voit pendant que le WebAssembly arrive, à la place d'une
/// page blanche. L'application le lève une fois montée.
fn coquille(
    feuille: &str,
    favicon: &str,
    logo: &str,
    wasm: &str,
    colle: &str,
    ressources: &str,
) -> String {
    format!(
        r##"<!doctype html>
<html lang="fr">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
    <title>MonCap GPS</title>
    <meta name="description" content="Votre carte MonCap GPS : positions, itinéraires, signalements et conducteurs en direct." />
    <meta name="robots" content="noindex" />
    <meta name="theme-color" content="#0e2a5e" />
    <link rel="icon" type="image/png" href="{favicon}" />
    <link rel="stylesheet" href="{feuille}" />
    <link rel="modulepreload" href="{colle}" />
    <link rel="preload" href="{wasm}" as="fetch" type="application/wasm" crossorigin="anonymous" />
    <link rel="preconnect" href="https://tile.openstreetmap.org" />
    <noscript><style>.rideau{{display:none}}</style></noscript>
  </head>
  <body>
    <div id="rideau" class="rideau" aria-hidden="true">
      <img src="{logo}" width="88" height="88" alt="" />
      <span class="rideau-barre"><span></span></span>
    </div>
    <div id="racine" data-ressources="{ressources}"></div>
    <noscript>
      <p class="sans-script">MonCap GPS a besoin de JavaScript et de WebAssembly pour afficher la carte. <a href="/">Retour au site</a></p>
    </noscript>
    <script type="module">
      import init from "{colle}";
      init({{ module_or_path: "{wasm}" }});
    </script>
  </body>
</html>
"##,
        ressources = attribut(ressources),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_json_traverse_l_attribut_intact() {
        let json = r#"{"logo":"/web/logo-a.webp","avatars":{"green":"/web/g.webp"}}"#;
        let html = coquille("/f.css", "/i.png", "/l.webp", "/a.wasm", "/a.js", json);
        assert!(html.contains("data-ressources=\"{&quot;logo&quot;"));
        assert!(
            !html.contains("data-ressources=\"{\"logo"),
            "un guillemet nu casserait l'attribut"
        );
    }

    #[test]
    fn la_coquille_porte_le_titre_que_le_serveur_verifie() {
        let html = coquille("/f.css", "/i.png", "/l.webp", "/a.wasm", "/a.js", "{}");
        assert!(html.contains("<title>MonCap GPS</title>"));
        assert!(html.contains(r#"id="rideau""#));
        assert!(html.contains(r#"id="racine""#));
    }
}
