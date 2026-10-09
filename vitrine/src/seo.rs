//! Le référencement : adresse canonique, plan du site, consignes aux robots
//! et données structurées.
//!
//! Un module partagé plutôt que du texte dans le constructeur : les tests
//! appellent les mêmes fonctions que la construction — ce que Google lira
//! est ce que les tests ont vu.
//!
//! **Ce module porte plus de poids qu'avant.** La page est désormais rendue
//! par le navigateur : le corps part vide, et tout ce qu'un robot ou un
//! aperçu de lien trouve immédiatement est ce qui est écrit ici. Les aperçus
//! (messageries, réseaux) ne lisent que la tête et restent donc parfaits ;
//! l'indexation, elle, dépend de l'exécution du WebAssembly par le moteur.
//! Le repli `<noscript>` ci-dessous existe pour cette raison.

use crate::contenu::{Contenu, Langue, PageLegale, AUTEUR, CONNEXION};

/// L'adresse publique canonique du site.
///
/// Réglable par `SITE_PUBLIC` à la construction : le jour où le vrai nom de
/// domaine arrive, une variable suffit — canonique, hreflang, plan du site,
/// Open Graph et données structurées suivent tous, d'un coup.
pub fn public() -> String {
    let brut = std::env::var("SITE_PUBLIC").unwrap_or_default();
    let brut = brut.trim();
    let adresse = if brut.is_empty() {
        "https://moncap-c41a5aaf07e8.herokuapp.com"
    } else {
        brut
    };
    // La barre de fin est coupée : sans cela, `{public}{chemin}` donnerait
    // « //es ».
    adresse.trim_end_matches('/').to_string()
}

/// La balise que Google Search Console réclame pour prouver qu'on tient le
/// site. Le jeton est propre au compte Google et ne peut pas vivre dans le
/// dépôt. Absent, rien n'est écrit — pas de balise vide, que Google lirait
/// comme une vérification ratée.
pub fn verification_google() -> Vec<String> {
    match std::env::var("GOOGLE_SITE_VERIFICATION") {
        Ok(j) if !j.trim().is_empty() => vec![format!(
            r#"<meta name="google-site-verification" content="{}" />"#,
            attr(j.trim())
        )],
        _ => Vec::new(),
    }
}

/// Locale Open Graph par langue (le format `fr_FR` est celui du protocole).
pub const fn og_locale(l: Langue) -> &'static str {
    match l {
        Langue::Fr => "fr_FR",
        Langue::Es => "es_ES",
        Langue::En => "en_US",
    }
}

/// Échappe un texte destiné à un attribut HTML.
pub fn attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// Échappe un texte destiné au corps du document.
pub fn texte(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Quel groupe de pages traduites entre elles.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Groupe {
    Accueil,
    Mentions,
    Confidentialite,
}

impl Groupe {
    pub const TOUS: [Groupe; 3] = [Groupe::Accueil, Groupe::Mentions, Groupe::Confidentialite];

    /// Le chemin de ce groupe dans une langue donnée.
    pub fn chemin(self, l: Langue) -> &'static str {
        let c = l.contenu();
        match self {
            Groupe::Accueil => c.chemin,
            Groupe::Mentions => c.mentions.chemin,
            Groupe::Confidentialite => c.confidentialite.chemin,
        }
    }
}

/// Les neuf pages publiques : trois principales et six légales.
pub fn pages_publiques() -> Vec<(Langue, Groupe)> {
    Langue::TOUTES
        .into_iter()
        .flat_map(|l| Groupe::TOUS.into_iter().map(move |g| (l, g)))
        .collect()
}

/// Les balises `alternate` d'un groupe, en adresses **absolues** : un
/// hreflang relatif est invalide pour les moteurs.
///
/// La version française fait `x-default` : c'est la langue du projet.
fn alternates(g: Groupe) -> Vec<String> {
    let p = public();
    let mut v: Vec<String> = Langue::TOUTES
        .into_iter()
        .map(|l| {
            format!(
                r#"<link rel="alternate" hreflang="{}" href="{p}{}" />"#,
                l.code(),
                g.chemin(l)
            )
        })
        .collect();
    v.push(format!(
        r#"<link rel="alternate" hreflang="x-default" href="{p}{}" />"#,
        g.chemin(Langue::Fr)
    ));
    v
}

/// Le plan du site. Chaque page — principales ET légales — porte les
/// alternates hreflang de son groupe, en adresses absolues : une page légale
/// sans alternates laissait Google deviner que /es/privacidad traduit
/// /confidentialite.
pub fn sitemap() -> String {
    let p = public();
    let mut urls = String::new();
    for g in Groupe::TOUS {
        let alt: String = Langue::TOUTES
            .into_iter()
            .map(|l| {
                format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{p}{}\"/>\n",
                    l.code(),
                    g.chemin(l)
                )
            })
            .collect::<String>()
            + &format!(
                "    <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"{p}{}\"/>",
                g.chemin(Langue::Fr)
            );
        for l in Langue::TOUTES {
            urls.push_str(&format!(
                "  <url>\n    <loc>{p}{}</loc>\n{alt}\n  </url>\n",
                g.chemin(l)
            ));
        }
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" \
         xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n{urls}</urlset>\n"
    )
}

/// Les consignes aux robots : tout est ouvert sauf ce qui n'est pas une page
/// — l'application, les liens de partage et de suivi (contenu privé des
/// utilisateurs), la réinitialisation, l'API.
pub fn robots() -> String {
    format!(
        "User-agent: *\n\
         Disallow: /app\n\
         Disallow: /t\n\
         Disallow: /live\n\
         Disallow: /reinitialisation\n\
         Disallow: /auth/\n\
         Disallow: /tiles/\n\
         Disallow: /offline/\n\
         \n\
         Sitemap: {}/sitemap.xml\n",
        public()
    )
}

/// Un objet JSON écrit à la main.
///
/// Dépendre de `serde_json` pour neuf documents connus d'avance ferait
/// entrer un analyseur complet dans la construction pour rien. L'échappement
/// est le seul point délicat, et il est testé.
fn json_texte(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Un `<` dans du JSON inclus dans `<script>` peut fermer la
            // balise ; on l'échappe en unicode, ce que JSON autorise.
            '<' => out.push_str("\\u003c"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Les données structurées schema.org : une application iOS, son auteur, ses
/// deux prix. Pas d'offre à 0 € : l'essai de quatorze jours n'est pas une
/// formule, et un prix gratuit ici serait un mensonge en résultat enrichi.
pub fn json_ld(c: &Contenu) -> String {
    let p = public();
    let traits: Vec<String> = c
        .tarifs
        .plus
        .points
        .iter()
        .chain(c.tarifs.pro.points.iter())
        .map(|s| json_texte(s))
        .collect();
    format!(
        "{{\"@context\":\"https://schema.org\",\"@type\":\"SoftwareApplication\",\
         \"name\":\"MonCap GPS\",\"operatingSystem\":\"iOS\",\
         \"applicationCategory\":\"TravelApplication\",\"description\":{desc},\
         \"inLanguage\":{lang},\"url\":{url},\
         \"author\":{{\"@type\":\"Person\",\"name\":{auteur}}},\
         \"featureList\":[{traits}],\
         \"offers\":[\
         {{\"@type\":\"Offer\",\"name\":\"MonCap Plus\",\"price\":\"3.99\",\"priceCurrency\":\"EUR\"}},\
         {{\"@type\":\"Offer\",\"name\":\"MonCap Pro\",\"price\":\"9.99\",\"priceCurrency\":\"EUR\"}}\
         ]}}",
        desc = json_texte(c.description),
        lang = json_texte(c.code),
        url = json_texte(&format!("{p}{}", c.chemin)),
        auteur = json_texte(AUTEUR),
        traits = traits.join(","),
    )
}

/// Le fil d'Ariane d'une page légale : Accueil → la page. Deux niveaux, pas
/// de théâtre — c'est ce que le résultat de recherche affiche sous le titre.
pub fn json_ld_legale(c: &Contenu, pg: &PageLegale) -> String {
    let p = public();
    format!(
        "{{\"@context\":\"https://schema.org\",\"@type\":\"BreadcrumbList\",\
         \"itemListElement\":[\
         {{\"@type\":\"ListItem\",\"position\":1,\"name\":\"MonCap GPS\",\"item\":{accueil}}},\
         {{\"@type\":\"ListItem\",\"position\":2,\"name\":{titre},\"item\":{page}}}\
         ]}}",
        accueil = json_texte(&format!("{p}{}", c.chemin)),
        titre = json_texte(pg.titre),
        page = json_texte(&format!("{p}{}", pg.chemin)),
    )
}

/// La tête d'une page principale.
pub fn tete(c: &Contenu, l: Langue, feuille: &str, logo: &str, favicon: &str) -> String {
    let p = public();
    let mut v = vec![
        format!("<title>{}</title>", attr(c.titre)),
        format!(
            r#"<meta name="description" content="{}" />"#,
            attr(c.description)
        ),
        format!(r#"<link rel="canonical" href="{p}{}" />"#, c.chemin),
        r#"<meta name="robots" content="index,follow,max-image-preview:large" />"#.into(),
    ];
    v.extend(verification_google());
    v.push(r##"<meta name="theme-color" content="#0e2a5e" />"##.into());
    v.push(r#"<meta property="og:type" content="website" />"#.into());
    v.push(r#"<meta property="og:site_name" content="MonCap GPS" />"#.into());
    v.push(format!(
        r#"<meta property="og:url" content="{p}{}" />"#,
        c.chemin
    ));
    v.push(format!(
        r#"<meta property="og:title" content="{}" />"#,
        attr(c.titre)
    ));
    v.push(format!(
        r#"<meta property="og:description" content="{}" />"#,
        attr(c.description)
    ));
    v.push(format!(
        r#"<meta property="og:image" content="{p}{logo}" />"#
    ));
    v.push(format!(
        r#"<meta property="og:locale" content="{}" />"#,
        og_locale(l)
    ));
    for autre in Langue::TOUTES.into_iter().filter(|a| *a != l) {
        v.push(format!(
            r#"<meta property="og:locale:alternate" content="{}" />"#,
            og_locale(autre)
        ));
    }
    v.push(r#"<meta name="twitter:card" content="summary" />"#.into());
    v.push(format!(
        r#"<meta name="twitter:title" content="{}" />"#,
        attr(c.titre)
    ));
    v.push(format!(
        r#"<meta name="twitter:description" content="{}" />"#,
        attr(c.description)
    ));
    v.push(format!(
        r#"<meta name="twitter:image" content="{p}{logo}" />"#
    ));
    v.push(format!(
        r#"<link rel="icon" type="image/png" href="{favicon}" />"#
    ));
    v.push(format!(r#"<link rel="apple-touch-icon" href="{logo}" />"#));
    v.extend(alternates(Groupe::Accueil));
    v.push(format!(
        r#"<script type="application/ld+json">{}</script>"#,
        json_ld(c)
    ));
    v.push(format!(r#"<link rel="stylesheet" href="{feuille}" />"#));
    v.into_iter()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// La tête d'une page légale.
pub fn tete_legale(
    c: &Contenu,
    l: Langue,
    pg: &PageLegale,
    g: Groupe,
    feuille: &str,
    favicon: &str,
) -> String {
    let p = public();
    let mut v = vec![
        format!("<title>{} — MonCap GPS</title>", attr(pg.titre)),
        format!(
            r#"<meta name="description" content="{}" />"#,
            attr(pg.description)
        ),
        format!(r#"<link rel="canonical" href="{p}{}" />"#, pg.chemin),
        r#"<meta name="robots" content="index,follow,max-image-preview:large" />"#.into(),
    ];
    v.extend(verification_google());
    v.push(r##"<meta name="theme-color" content="#0e2a5e" />"##.into());
    v.push(r#"<meta property="og:type" content="website" />"#.into());
    v.push(r#"<meta property="og:site_name" content="MonCap GPS" />"#.into());
    v.push(format!(
        r#"<meta property="og:url" content="{p}{}" />"#,
        pg.chemin
    ));
    v.push(format!(
        r#"<meta property="og:title" content="{}" />"#,
        attr(pg.titre)
    ));
    v.push(format!(
        r#"<meta property="og:description" content="{}" />"#,
        attr(pg.description)
    ));
    v.push(format!(
        r#"<meta property="og:locale" content="{}" />"#,
        og_locale(l)
    ));
    v.extend(alternates(g));
    v.push(format!(
        r#"<script type="application/ld+json">{}</script>"#,
        json_ld_legale(c, pg)
    ));
    v.push(format!(
        r#"<link rel="icon" type="image/png" href="{favicon}" />"#
    ));
    v.push(format!(r#"<link rel="stylesheet" href="{feuille}" />"#));
    v.into_iter()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Le repli affiché quand le WebAssembly ne s'exécute pas.
///
/// La page est rendue par le navigateur : sans WebAssembly, le corps reste
/// vide. Ce bloc n'est pas un ornement d'accessibilité mais le seul texte
/// que verront un robot qui n'exécute rien, un navigateur ancien, ou
/// quelqu'un qui a coupé les scripts.
///
/// Il ne refait pas la page : il en donne la substance — ce que l'app fait,
/// combien elle coûte, et par où continuer. Mieux vaut une page sobre et
/// vraie qu'un simulacre de la vraie.
pub fn repli(c: &Contenu, l: Langue) -> String {
    let p = public();
    let mut s = String::from("<noscript><div class=\"dedans repli\">");
    s.push_str(&format!("<h1>{} {}</h1>", texte(c.h1[0]), texte(c.h1[1])));
    s.push_str(&format!("<p>{}</p>", texte(c.chapeau)));
    // La porte de l'application. Depuis que la page est rendue par le
    // WebAssembly, ce repli était le seul HTML lisible sans script — et il
    // ne menait plus nulle part : le test d'intégration du serveur qui
    // cherche ce lien échouait depuis.
    s.push_str(&format!(
        "<p><a href=\"{CONNEXION}\">{}</a></p>",
        texte(c.se_connecter)
    ));
    for sec in [&c.comparateur.titre, &c.guidage.titre, &c.hors_ligne.titre] {
        s.push_str(&format!("<h2>{}</h2>", texte(sec)));
    }
    for intro in [c.comparateur.intro, c.guidage.intro, c.hors_ligne.intro] {
        s.push_str(&format!("<p>{}</p>", texte(intro)));
    }
    s.push_str(&format!("<h2>{}</h2>", texte(c.tarifs.titre)));
    for f in [&c.tarifs.plus, &c.tarifs.pro] {
        s.push_str(&format!(
            "<p>{} — {} {}</p>",
            texte(f.nom),
            texte(f.prix),
            texte(f.periode)
        ));
    }
    s.push_str("<p>");
    // Les trois langues, la courante marquée et non liée — comme le
    // sélecteur de la page rendue : un lien vers soi n'offre rien.
    for autre in Langue::TOUTES {
        if autre == l {
            s.push_str(&format!(
                "<span aria-current=\"true\">{}</span> ",
                texte(autre.contenu().nom)
            ));
        } else {
            s.push_str(&format!(
                "<a href=\"{}\" hreflang=\"{}\">{}</a> ",
                autre.chemin(),
                autre.code(),
                texte(autre.contenu().nom)
            ));
        }
    }
    s.push_str(&format!(
        "<a href=\"{}\">{}</a> <a href=\"{}\">{}</a>",
        c.mentions.chemin,
        texte(c.mentions.titre),
        c.confidentialite.chemin,
        texte(c.confidentialite.titre)
    ));
    s.push_str("</p>");
    s.push_str(&format!(
        "<p>{} <a href=\"{p}{}\">{}</a></p>",
        texte(c.signature),
        c.chemin,
        texte(AUTEUR)
    ));
    s.push_str("</div></noscript>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sans script, le repli est toute la page : il doit mener à
    /// l'application et dire dans quelle langue on se trouve. Le serveur le
    /// vérifie aussi, mais seulement avec une base de données sous la main.
    #[test]
    fn le_repli_mene_a_l_application_et_dit_sa_langue() {
        for l in Langue::TOUTES {
            let r = repli(l.contenu(), l);
            assert!(
                r.contains(&format!("href=\"{CONNEXION}\"")),
                "{} : pas de lien de connexion",
                l.code()
            );
            assert_eq!(
                r.matches("aria-current=\"true\"").count(),
                1,
                "{} : la langue courante doit être marquée, une fois",
                l.code()
            );
        }
    }

    #[test]
    fn l_adresse_publique_n_a_pas_de_barre_finale() {
        // Sans cela, « {public}{chemin} » donnerait « //es ».
        assert!(!public().ends_with('/'));
        assert!(public().starts_with("http"));
    }

    #[test]
    fn les_neuf_pages_publiques_sont_la() {
        let p = pages_publiques();
        assert_eq!(p.len(), 9, "trois langues × trois pages");
        let mut chemins: Vec<&str> = p.iter().map(|(l, g)| g.chemin(*l)).collect();
        chemins.sort_unstable();
        chemins.dedup();
        assert_eq!(chemins.len(), 9, "aucun chemin en double");
    }

    #[test]
    fn le_plan_du_site_cite_les_neuf_pages_en_absolu() {
        let s = sitemap();
        for (l, g) in pages_publiques() {
            let url = format!("<loc>{}{}</loc>", public(), g.chemin(l));
            assert!(s.contains(&url), "{url} absent du plan du site");
        }
        assert!(
            !s.contains("<loc>/"),
            "aucune adresse ne doit être relative"
        );
    }

    #[test]
    fn chaque_groupe_a_son_x_default_en_francais() {
        for g in Groupe::TOUS {
            let a = alternates(g);
            let def: Vec<&String> = a.iter().filter(|s| s.contains("x-default")).collect();
            assert_eq!(def.len(), 1, "un seul x-default par groupe");
            assert!(
                def[0].contains(&format!("{}{}", public(), g.chemin(Langue::Fr))),
                "x-default doit viser le français : {}",
                def[0]
            );
        }
    }

    #[test]
    fn les_alternates_couvrent_les_trois_langues() {
        for g in Groupe::TOUS {
            let a = alternates(g).join("");
            for l in Langue::TOUTES {
                assert!(
                    a.contains(&format!(r#"hreflang="{}""#, l.code())),
                    "{:?} manque dans le groupe {g:?}",
                    l
                );
            }
        }
    }

    #[test]
    fn les_robots_ferment_ce_qui_n_est_pas_une_page() {
        let r = robots();
        for prive in ["/app", "/t", "/live", "/auth/", "/offline/"] {
            assert!(
                r.contains(&format!("Disallow: {prive}")),
                "{prive} doit être fermé"
            );
        }
        assert!(r.contains(&format!("Sitemap: {}/sitemap.xml", public())));
    }

    #[test]
    fn le_json_ld_ne_promet_jamais_la_gratuite() {
        // Un prix à 0 € en résultat enrichi serait un mensonge : l'essai de
        // quatorze jours n'est pas une formule.
        for l in Langue::TOUTES {
            let j = json_ld(l.contenu());
            assert!(!j.contains("\"0\""), "aucune offre à zéro");
            assert!(!j.contains("0.00"), "aucune offre à zéro");
            assert!(j.contains("3.99") && j.contains("9.99"));
        }
    }

    #[test]
    fn le_json_ld_echappe_ce_qui_fermerait_la_balise() {
        // Une description contenant « </script> » casserait la page.
        // Le « < » part en \u003c, ce que JSON autorise et qu'aucun
        // analyseur de balises ne prend pour une fermeture.
        assert_eq!(json_texte("a</script>b"), r#""a\u003c/script>b""#);
        assert_eq!(json_texte(r#"un "mot""#), r#""un \"mot\"""#);
    }

    #[test]
    fn les_attributs_sont_echappes() {
        assert_eq!(attr(r#"a & b "c" <d>"#), "a &amp; b &quot;c&quot; &lt;d>");
    }

    #[test]
    fn la_tete_porte_tout_ce_qu_un_apercu_de_lien_lit() {
        // Le corps part vide : la tête est tout ce qu'une messagerie voit.
        for l in Langue::TOUTES {
            let t = tete(l.contenu(), l, "/f.css", "/logo.webp", "/i.png");
            for balise in [
                "og:title",
                "og:description",
                "og:image",
                "og:url",
                "twitter:card",
                "canonical",
                "application/ld+json",
            ] {
                assert!(t.contains(balise), "{balise} manque en {:?}", l);
            }
        }
    }

    #[test]
    fn le_repli_dit_la_substance_de_la_page() {
        for l in Langue::TOUTES {
            let c = l.contenu();
            let r = repli(c, l);
            assert!(r.starts_with("<noscript>") && r.ends_with("</noscript>"));
            assert!(r.contains(&texte(c.chapeau)), "le chapeau doit y être");
            assert!(
                r.contains(&texte(c.tarifs.plus.prix)),
                "le prix doit y être"
            );
            assert!(
                r.contains(c.mentions.chemin),
                "le légal doit rester joignable"
            );
            // Les deux autres langues sont atteignables sans script.
            for autre in Langue::TOUTES.into_iter().filter(|a| *a != l) {
                assert!(r.contains(autre.chemin()), "{autre:?} doit être joignable");
            }
        }
    }

    #[test]
    fn le_repli_echappe_le_texte() {
        assert_eq!(texte("a & b <i>"), "a &amp; b &lt;i&gt;");
    }
}
