//! Les outils de construction de la vitrine et du front web.
//!
//! Ils vivaient dans le `construit` de la vitrine ; le front web en a
//! exactement le même besoin. Une copie aurait divergé à la première
//! correction — la minification de la feuille en a déjà connu trois.
//!
//! Chaque fichier servi porte l'empreinte de son contenu dans son nom.
//! C'est la seule façon de corriger un cache déjà pris : un en-tête ne
//! s'applique qu'à une réponse que le navigateur demande, tandis qu'une
//! adresse neuve n'a jamais d'entrée en cache.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Le dossier produit, et le préfixe sous lequel le serveur le monte.
pub struct Sortie {
    pub dossier: PathBuf,
    pub base: &'static str,
}

impl Sortie {
    /// Vide et recrée le dossier.
    pub fn neuve(dossier: PathBuf, base: &'static str) -> Result<Sortie, String> {
        let _ = fs::remove_dir_all(&dossier);
        fs::create_dir_all(&dossier).map_err(|e| format!("création de {dossier:?} : {e}"))?;
        Ok(Sortie { dossier, base })
    }

    /// Écrit des octets sous un nom empreint ; rend l'adresse publique.
    pub fn ecrit_empreinte(
        &self,
        base: &str,
        extension: &str,
        octets: &[u8],
    ) -> Result<String, String> {
        let nom = format!("{base}-{}.{extension}", empreinte(octets));
        fs::write(self.dossier.join(&nom), octets)
            .map_err(|e| format!("écriture de {nom} : {e}"))?;
        Ok(format!("{}/{nom}", self.base))
    }

    /// Copie un fichier sous un nom empreint, en gardant sa tige et son
    /// extension.
    pub fn copie_empreinte(&self, source: &Path) -> Result<String, String> {
        let tige = source
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("nom de fichier illisible")?;
        let ext = source
            .extension()
            .and_then(|s| s.to_str())
            .ok_or("extension illisible")?;
        self.copie_empreinte_nommee(source, tige, ext)
    }

    pub fn copie_empreinte_nommee(
        &self,
        source: &Path,
        base: &str,
        extension: &str,
    ) -> Result<String, String> {
        let octets = fs::read(source).map_err(|e| format!("lecture de {source:?} : {e}"))?;
        self.ecrit_empreinte(base, extension, &octets)
    }

    /// Le poids d'un fichier produit, d'après son adresse publique.
    pub fn poids(&self, chemin_public: &str) -> usize {
        fs::metadata(self.dossier.join(nom_fichier(chemin_public)))
            .map(|m| m.len() as usize)
            .unwrap_or(0)
    }
}

// --- WebAssembly ----------------------------------------------------------

/// Compile l'application en WebAssembly, puis la passe à `wasm-bindgen` et
/// `wasm-opt`.
pub fn construit_wasm(racine: &Path, sortie: &Sortie) -> Result<(String, String), String> {
    lance(
        "cargo",
        &[
            "build",
            "--bin",
            "app",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
        ],
        racine,
    )?;

    let brut = racine.join("target/wasm32-unknown-unknown/release/app.wasm");
    let interm = racine.join("target/wasm-bindgen");
    let _ = fs::remove_dir_all(&interm);
    lance(
        "wasm-bindgen",
        &[
            "--target",
            "web",
            "--no-typescript",
            "--out-dir",
            interm.to_str().ok_or("chemin non UTF-8")?,
            brut.to_str().ok_or("chemin non UTF-8")?,
        ],
        racine,
    )?;

    let avant = interm.join("app_bg.wasm");
    let apres = interm.join("app_opt.wasm");
    // Les fonctionnalités sont demandées explicitement : le `wasm-opt`
    // distribué par npm est plus ancien que le compilateur Rust, et refuse
    // sinon les opérations de mémoire en bloc que celui-ci émet.
    let opt = lance(
        "wasm-opt",
        &[
            "-Oz",
            "--strip-debug",
            "--strip-producers",
            "--enable-bulk-memory",
            "--enable-nontrapping-float-to-int",
            "--enable-sign-ext",
            "--enable-mutable-globals",
            "--enable-reference-types",
            "-o",
            apres.to_str().ok_or("chemin non UTF-8")?,
            avant.to_str().ok_or("chemin non UTF-8")?,
        ],
        racine,
    );
    // `wasm-opt` est un confort, pas une dépendance : absent, on livre la
    // sortie de `wasm-bindgen`, qui est déjà correcte — simplement plus
    // grosse. Faire échouer la construction pour un optimiseur manquant
    // empêcherait de livrer pour une raison qui ne regarde pas le produit.
    let final_wasm = if opt.is_ok() {
        apres
    } else {
        eprintln!("note : wasm-opt indisponible, le WebAssembly part non optimisé");
        avant
    };

    let wasm = sortie.copie_empreinte_nommee(&final_wasm, "app", "wasm")?;
    let colle_src = interm.join("app.js");
    // La colle référence le `.wasm` par son nom d'origine ; on lui donne le
    // nôtre, empreinte comprise, sans quoi elle irait chercher un fichier
    // qui n'existe pas sous ce nom dans `dist`.
    let colle_txt = fs::read_to_string(&colle_src)
        .map_err(|e| format!("lecture de la colle : {e}"))?
        .replace("app_bg.wasm", &nom_fichier(&wasm));
    let colle = sortie.ecrit_empreinte("app", "js", colle_txt.as_bytes())?;
    Ok((wasm, colle))
}

pub fn lance(programme: &str, args: &[&str], dossier: &Path) -> Result<(), String> {
    let sortie = Command::new(programme)
        .args(args)
        .current_dir(dossier)
        .output()
        .map_err(|e| format!("{programme} introuvable : {e}"))?;
    if sortie.status.success() {
        return Ok(());
    }
    Err(format!(
        "{programme} a échoué :\n{}",
        String::from_utf8_lossy(&sortie.stderr)
    ))
}

// --- Fichiers -------------------------------------------------------------

/// Empreinte du contenu, en base 36 sur huit caractères.
///
/// FNV-1a plutôt qu'une dépendance : il ne s'agit pas de résister à une
/// attaque mais de changer de nom quand le contenu change, et soixante-quatre
/// bits y suffisent très largement.
pub fn empreinte(octets: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for o in octets {
        h ^= *o as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    let chiffres = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut s = Vec::new();
    while s.len() < 8 {
        s.push(chiffres[(h % 36) as usize]);
        h /= 36;
    }
    String::from_utf8(s).expect("base 36 est de l'ASCII")
}

pub fn nom_fichier(chemin_public: &str) -> String {
    chemin_public.rsplit('/').next().unwrap_or("").to_string()
}

pub fn ko(octets: usize) -> String {
    format!("{:.1}", octets as f64 / 1024.0)
}

// --- Feuille de style -----------------------------------------------------

/// Allège la feuille sans la réécrire.
///
/// Volontairement timide : on retire les commentaires et on resserre les
/// blancs, rien d'autre. Un vrai minificateur réordonne les règles et
/// raccourcit les couleurs, ce qui demande de comprendre la cascade — et la
/// feuille de ce site tient sur `:has()`, des animations liées au défilement
/// et quatre thèmes imbriqués. Le serveur comprime déjà en gzip et brotli,
/// où l'essentiel du gain se trouve de toute façon.
///
/// Les chaînes sont traversées sans y toucher : une URL ou un `content:`
/// peut contenir `/*` ou deux espaces qui comptent.
pub fn minifie_css(source: &str) -> String {
    /// La ponctuation de structure : ce qui sépare déjà, et n'a donc pas
    /// besoin d'un blanc de chaque côté.
    ///
    /// « : » n'en fait pas partie, et c'est délibéré : en CSS il sert aussi
    /// aux pseudo-classes, où `a :hover` (un descendant survolé) et
    /// `a:hover` (le lien survolé) désignent deux choses différentes. Sans
    /// analyser la feuille, on ne peut pas distinguer les deux — on laisse
    /// donc l'espace, qui ne coûte presque rien une fois comprimé.
    const STRUCTURE: [char; 4] = ['{', '}', ';', ','];

    let o: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    // Un blanc retenu, écrit seulement si ce qui suit en a besoin. C'est ce
    // qui permet de resserrer la ponctuation **sans jamais toucher au
    // contenu des chaînes** : tout se décide ici, où l'on sait où l'on est.
    let mut blanc = false;
    let mut i = 0;

    while i < o.len() {
        let c = o[i];

        // Commentaire : retiré, mais il laisse une césure — sans elle,
        // « a/* */b » deviendrait « ab ».
        if c == '/' && i + 1 < o.len() && o[i + 1] == '*' {
            i += 2;
            while i + 1 < o.len() && !(o[i] == '*' && o[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(o.len());
            blanc = !out.is_empty();
            continue;
        }

        if c.is_whitespace() {
            while i < o.len() && o[i].is_whitespace() {
                i += 1;
            }
            blanc = !out.is_empty();
            continue;
        }

        // Un point-virgule juste avant une accolade fermante ne sert à rien.
        if c == ';' {
            let mut j = i + 1;
            while j < o.len() && o[j].is_whitespace() {
                j += 1;
            }
            if j < o.len() && o[j] == '}' {
                i = j;
                continue;
            }
        }

        if STRUCTURE.contains(&c) {
            // Le blanc retenu tombe, et celui qui précède aussi.
            blanc = false;
            while out.ends_with(' ') {
                out.pop();
            }
            out.push(c);
            i += 1;
            // Et le blanc qui suit : on le consomme tout de suite.
            while i < o.len() && o[i].is_whitespace() {
                i += 1;
            }
            continue;
        }

        if blanc {
            out.push(' ');
            blanc = false;
        }

        // Chaîne : recopiée telle quelle, délimiteur compris. Rien de ce qui
        // suit ne doit la réécrire — un `content: "a , b"` garde ses blancs
        // et sa virgule.
        if c == '"' || c == '\'' {
            out.push(c);
            i += 1;
            while i < o.len() && o[i] != c {
                if o[i] == '\\' && i + 1 < o.len() {
                    out.push(o[i]);
                    i += 1;
                }
                out.push(o[i]);
                i += 1;
            }
            if i < o.len() {
                out.push(o[i]);
                i += 1;
            }
            continue;
        }

        out.push(c);
        i += 1;
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_empreinte_change_avec_le_contenu() {
        assert_ne!(empreinte(b"a"), empreinte(b"b"));
        assert_eq!(empreinte(b"abc"), empreinte(b"abc"));
        assert_eq!(empreinte(b"abc").len(), 8);
        assert!(empreinte(b"abc").chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn la_minification_retire_les_commentaires() {
        assert_eq!(minifie_css("a { color: red } /* note */"), "a{color: red}");
    }

    /// Le cas qui fait les dégâts : un commentaire collé entre deux mots.
    #[test]
    fn un_commentaire_ne_soude_pas_deux_mots() {
        assert_eq!(minifie_css("a/* x */b { c: d }"), "a b{c: d}");
    }

    #[test]
    fn les_chaines_traversent_intactes() {
        // Un `content:` peut contenir ce qui ressemble à un commentaire.
        assert_eq!(
            minifie_css(r#"a::after { content: "/*  x  */" }"#),
            r#"a::after{content: "/*  x  */"}"#
        );
    }

    /// Les animations liées au défilement ont des valeurs à plusieurs mots :
    /// les écraser casserait silencieusement toutes les révélations.
    #[test]
    fn les_valeurs_a_plusieurs_mots_gardent_leurs_blancs() {
        let css = ".a { animation-range: entry 8% cover 26%; }";
        assert_eq!(minifie_css(css), ".a{animation-range: entry 8% cover 26%}");
    }

    /// Le défaut que j'avais introduit : le resserrage de ponctuation
    /// s'appliquait aussi au contenu des chaînes. Un `content` ne doit
    /// jamais être réécrit, virgules et blancs compris.
    #[test]
    fn la_ponctuation_dans_une_chaine_est_laissee_tranquille() {
        assert_eq!(
            minifie_css(r#"a::before { content: "x , y ; z { }" }"#),
            r#"a::before{content: "x , y ; z { }"}"#
        );
    }

    /// Un point-virgule juste avant une accolade fermante est redondant.
    #[test]
    fn le_dernier_point_virgule_disparait() {
        assert_eq!(minifie_css("a { b: c; }"), "a{b: c}");
        // Mais pas les autres.
        assert_eq!(minifie_css("a { b: c; d: e; }"), "a{b: c;d: e}");
    }

    #[test]
    fn la_minification_allege_vraiment() {
        let gros = "/* un commentaire assez long pour peser */\n.a {\n  color: red;\n}\n";
        let petit = minifie_css(gros);
        assert!(petit.len() < gros.len() / 2, "{petit:?}");
    }
}
