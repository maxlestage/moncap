#!/usr/bin/env python3
"""Transforme le contenu TypeScript en source Rust, une fois pour toutes.

Ce script est un **outil de migration**, pas une pièce du build : il tourne
une seule fois, son résultat est relu puis versionné, et le TypeScript
disparaît. Porter 1 432 lignes de texte trilingue à la main aurait introduit
des fautes qu'aucun compilateur n'aurait vues — une accentuation perdue, une
phrase espagnole recopiée dans le bloc anglais.

Le Rust qu'il produit est fait pour être lu et modifié ensuite : des
structures nommées et des constantes, pas du JSON analysé à l'exécution. Le
texte entre donc dans le binaire tel quel, sans analyseur à embarquer ni coût
au démarrage.
"""

import json
import sys

RUST = []


def ecris(ligne=""):
    RUST.append(ligne)


def chaine(s: str) -> str:
    """Un littéral Rust. Les guillemets crus évitent d'échapper les
    apostrophes, omniprésentes en français — mais pas si le texte contient
    déjà la séquence fermante."""
    if '"' not in s and "\\" not in s:
        return f'r#"{s}"#' if "#" in s else f'"{s}"'
    return json.dumps(s, ensure_ascii=False)


def liste(items, rendu) -> str:
    return "&[" + ", ".join(rendu(i) for i in items) + "]"


def multi(items, rendu, indent: int) -> str:
    pad = " " * indent
    corps = "".join(f"\n{pad}    {rendu(i)}," for i in items)
    return f"&[{corps}\n{pad}]"


def main():
    d = json.load(open(sys.argv[1], encoding="utf-8"))
    langues, k = d["langues"], d["k"]

    ecris("//! Le contenu de la vitrine : trois langues, et rien d'autre.")
    ecris("//!")
    ecris("//! Écrit une fois en Rust et compilé dans les deux cibles — le WebAssembly")
    ecris("//! qui rend la page, et le binaire natif qui fabrique les coquilles HTML.")
    ecris("//! C'est ce que le passage à Rust achète : le texte n'existe qu'ici, et le")
    ecris("//! compilateur refuse qu'une langue oublie un champ que les autres ont.")
    ecris("//!")
    ecris("//! Tout est `&'static str` : aucune allocation, aucun analyseur embarqué,")
    ecris("//! rien à faire au démarrage. Le texte est dans le binaire, point.")
    ecris("//!")
    ecris("//! Issu d'une conversion mécanique depuis l'ancien `langues.ts`")
    ecris("//! (`outils/ts_vers_rust.py`), puis relu. Le TypeScript n'existe plus.")
    ecris()

    # --- Les types -------------------------------------------------------
    ecris(
        """
/// Une section de la page : un titre, un paragraphe d'introduction.
#[derive(Clone, Copy, PartialEq)]
pub struct Section {
    pub titre: &'static str,
    pub intro: &'static str,
}

/// Le comparateur a un paragraphe de plus, après la maquette.
#[derive(Clone, Copy, PartialEq)]
pub struct SectionApres {
    pub titre: &'static str,
    pub intro: &'static str,
    pub apres: &'static str,
}

/// Un chiffre du bandeau. `valeur` est une chaîne et non un nombre : il y a
/// « 3,99 € » parmi eux, et la virgule décimale change selon la langue.
#[derive(Clone, Copy, PartialEq)]
pub struct Chiffre {
    pub valeur: &'static str,
    pub legende: &'static str,
}

/// Une carte d'atout : un emoji, un titre, un texte.
#[derive(Clone, Copy, PartialEq)]
pub struct Atout {
    pub emoji: &'static str,
    pub titre: &'static str,
    pub texte: &'static str,
}

/// Une formule tarifaire.
#[derive(Clone, Copy, PartialEq)]
pub struct Formule {
    pub nom: &'static str,
    pub prix: &'static str,
    pub periode: &'static str,
    pub points: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq)]
pub struct Tarifs {
    pub titre: &'static str,
    pub intro: &'static str,
    pub plus: Formule,
    pub pro: Formule,
    pub note: &'static str,
}

/// La mesure mise en avant, versée au milieu d'une phrase.
#[derive(Clone, Copy, PartialEq)]
pub struct Preuve {
    pub titre: &'static str,
    pub texte: &'static str,
    pub mesure: &'static str,
    pub note: &'static str,
}

/// Un domaine technique et ses outils.
#[derive(Clone, Copy, PartialEq)]
pub struct Pile {
    pub domaine: &'static str,
    pub outils: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq)]
pub struct Legendes {
    pub itineraires: &'static str,
    pub guidage: &'static str,
    pub montre: &'static str,
    pub lieux: &'static str,
    pub regions: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Chevrons {
    pub descendre: &'static str,
    pub remonter: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Theme {
    pub libelle: &'static str,
    pub noms: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq)]
pub struct Maquette {
    pub installee: &'static str,
}

/// Une ancre du pied de page.
#[derive(Clone, Copy, PartialEq)]
pub struct Ancre {
    pub id: &'static str,
    pub nom: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Pied {
    pub slogan: &'static str,
    pub decouvrir: &'static str,
    pub ancres: &'static [Ancre],
    pub donnees: &'static str,
    pub legal_titre: &'static str,
    pub copyright: &'static str,
}

/// Une section d'une page légale : un titre, des paragraphes.
#[derive(Clone, Copy, PartialEq)]
pub struct SectionLegale {
    pub titre: &'static str,
    pub corps: &'static [&'static str],
}

/// Une page légale entière.
#[derive(Clone, Copy, PartialEq)]
pub struct PageLegale {
    pub chemin: &'static str,
    pub titre: &'static str,
    pub description: &'static str,
    pub retour: &'static str,
    pub sections: &'static [SectionLegale],
}

/// Tout le texte d'une langue.
#[derive(Clone, Copy, PartialEq)]
pub struct Contenu {
    pub code: &'static str,
    pub chemin: &'static str,
    pub nom: &'static str,
    pub titre: &'static str,
    pub description: &'static str,
    pub h1: &'static [&'static str],
    pub chapeau: &'static str,
    pub se_connecter: &'static str,
    pub chevrons: Chevrons,
    pub theme: Theme,
    pub langue_libelle: &'static str,
    pub chiffres: &'static [Chiffre],
    pub comparateur: SectionApres,
    pub guidage: Section,
    pub montre: Section,
    pub lieux: Section,
    pub hors_ligne: Section,
    pub atouts_titre: &'static str,
    pub atouts: &'static [Atout],
    pub hors_ligne_cartes: &'static [Atout],
    pub tarifs: Tarifs,
    pub preuve: Preuve,
    pub pile_titre: &'static str,
    pub pile: &'static [Pile],
    pub franchise_titre: &'static str,
    pub franchise: &'static [&'static str],
    pub legendes: Legendes,
    pub reste: &'static str,
    pub maquette: Maquette,
    pub signature: &'static str,
    pub credits: &'static str,
    pub pied: Pied,
    pub mentions: PageLegale,
    pub confidentialite: PageLegale,
}

/// Les trois langues de la vitrine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Langue {
    Fr,
    Es,
    En,
}

impl Langue {
    pub const TOUTES: [Langue; 3] = [Langue::Fr, Langue::Es, Langue::En];

    /// Le contenu de cette langue.
    pub const fn contenu(self) -> &'static Contenu {
        match self {
            Langue::Fr => &FR,
            Langue::Es => &ES,
            Langue::En => &EN,
        }
    }

    /// Le code BCP 47, tel qu'il part dans `lang` et `hreflang`.
    pub const fn code(self) -> &'static str {
        self.contenu().code
    }

    /// Le chemin de la page d'accueil de cette langue.
    pub const fn chemin(self) -> &'static str {
        self.contenu().chemin
    }

    /// Reconnaît une langue à son code, pour la lire depuis la page.
    pub fn depuis_code(code: &str) -> Option<Langue> {
        Langue::TOUTES.into_iter().find(|l| l.code() == code)
    }
}
""".strip()
    )
    ecris()

    # --- Les constantes partagées ----------------------------------------
    ecris("// --- Ce qui ne se traduit pas ---------------------------------------")
    ecris()
    ecris("/// Le plafond annoncé partout : le bandeau de chiffres le cite, et la")
    ecris("/// maquette ne doit jamais en montrer davantage.")
    ecris(f"pub const ITINERAIRES_MAX: u32 = {k['ITINERAIRES_MAX']};")
    ecris()
    ecris(f"pub const AUTEUR: &str = {chaine(k['AUTEUR'])};")
    ecris(f"pub const CONTACT: &str = {chaine(k['CONTACT'])};")
    ecris(f"pub const CONNEXION: &str = {chaine(k['CONNEXION'])};")
    ecris(f"pub const MODE_ACTIF: &str = {chaine(k['MODE_ACTIF'])};")
    ecris()
    ecris("/// Les quatre modes de déplacement, dans l'ordre du sélecteur.")
    ecris(
        "pub const MODES: &[&str] = "
        + liste(k["MODES"], lambda s: chaine(s))
        + ";"
    )
    ecris()

    ecris("/// La teinte d'une étiquette d'itinéraire.")
    ecris("///")
    ecris("/// Une énumération et non une chaîne : le TypeScript écrivait")
    ecris('/// `\"verte\" | \"bleue\" | \"orange\"`, ce que Rust sait dire pour de bon.')
    ecris("/// Une quatrième couleur devient une erreur de compilation, plus une")
    ecris("/// classe CSS absente qu'on découvre à l'écran.")
    ecris("#[derive(Clone, Copy, PartialEq, Eq)]")
    ecris("pub enum Teinte {")
    ecris("    Verte,")
    ecris("    Bleue,")
    ecris("    Orange,")
    ecris("}")
    ecris()
    ecris("impl Teinte {")
    ecris("    /// Le nom de la classe CSS, inchangé depuis la feuille existante.")
    ecris("    pub const fn classe(self) -> &'static str {")
    ecris("        match self {")
    ecris('            Teinte::Verte => "verte",')
    ecris('            Teinte::Bleue => "bleue",')
    ecris('            Teinte::Orange => "orange",')
    ecris("        }")
    ecris("    }")
    ecris("}")
    ecris()
    ecris("/// Un itinéraire d'exemple pour la maquette du comparateur.")
    ecris("///")
    ecris("/// Le TypeScript rangeait ces valeurs dans des tuples — `trajet` mêlait")
    ecris("/// deux nombres et deux chaînes, `cout` avait une ou trois cases selon")
    ecris("/// qu'il y a des péages. Des champs nommés disent la même chose sans")
    ecris("/// qu'il faille se souvenir d'un ordre, et `Option` dit mieux")
    ecris("/// « il n'y en a pas » qu'une longueur de tableau.")
    ecris("///")
    ecris("/// Le total des coûts n'est pas stocké : il valait exactement la somme")
    ecris("/// du carburant et des péages, et deux façons d'écrire le même nombre")
    ecris("/// finissent toujours par diverger. Voir `total()`.")
    ecris("#[derive(Clone, Copy, PartialEq)]")
    ecris("pub struct Itineraire {")
    ecris("    pub etiquette: &'static str,")
    ecris("    pub teinte: Teinte,")
    ecris("    pub minutes: u32,")
    ecris("    /// Déjà mise en forme : l'application écrit des mètres sous le")
    ecris("    /// kilomètre, et « 0.8 km » n'est pas ce qu'elle dit.")
    ecris("    pub distance: &'static str,")
    ecris("    pub virages: u32,")
    ecris("    pub arrivee: &'static str,")
    ecris("    /// Côte puis descente, en mètres.")
    ecris("    pub denivele: Option<(u32, u32)>,")
    ecris("    pub carburant: f64,")
    ecris("    /// Seulement quand l'itinéraire en emprunte.")
    ecris("    pub peages: Option<f64>,")
    ecris("    /// Mètres de côte, quand elle mérite un avertissement.")
    ecris("    pub avertissement: Option<u32>,")
    ecris("}")
    ecris()
    ecris("impl Itineraire {")
    ecris("    /// Ce que coûte le trajet, carburant et péages compris.")
    ecris("    pub fn total(&self) -> f64 {")
    ecris("        self.carburant + self.peages.unwrap_or(0.0)")
    ecris("    }")
    ecris("}")
    ecris()

    def nombre(x):
        v = float(x)
        return f"{v:.1f}" if v == int(v) else repr(v)

    def option_paire(v):
        return f"Some(({v[0]}, {v[1]}))" if v else "None"

    ecris("pub const MAQUETTE_ITINERAIRES: &[Itineraire] = &[")
    for i in k["MAQUETTE_ITINERAIRES"]:
        minutes, distance, virages, arrivee = i["trajet"]
        cout = i["cout"]
        # `[carburant]`, ou `[carburant, péages, total]` — le total est la
        # somme exacte des deux premiers, on ne le recopie donc pas.
        if len(cout) == 3:
            assert abs((cout[0] + cout[1]) - cout[2]) < 1e-9, (
                f"le total {cout[2]} ne vaut pas {cout[0]} + {cout[1]}"
            )
            peages = f"Some({nombre(cout[1])})"
        else:
            peages = "None"
        den = i.get("denivele")
        avert = i.get("avertissement")
        ecris("    Itineraire {")
        ecris(f"        etiquette: {chaine(i['etiquette'])},")
        ecris(f"        teinte: Teinte::{i['teinte'].capitalize()},")
        ecris(f"        minutes: {minutes},")
        ecris(f"        distance: {chaine(distance)},")
        ecris(f"        virages: {virages},")
        ecris(f"        arrivee: {chaine(arrivee)},")
        ecris(f"        denivele: {option_paire(den)},")
        ecris(f"        carburant: {nombre(cout[0])},")
        ecris(f"        peages: {peages},")
        ecris(
            f"        avertissement: {f'Some({avert[0]})' if avert else 'None'},"
        )
        ecris("    },")
    ecris("];")
    ecris()

    ecris("/// Un lieu d'une catégorie, pour la maquette.")
    ecris("#[derive(Clone, Copy, PartialEq)]")
    ecris("pub struct Lieu {")
    ecris("    pub emoji: &'static str,")
    ecris("    pub nom: &'static str,")
    ecris("}")
    ecris()
    ecris("/// Les 42 catégories annoncées par le bandeau de chiffres.")
    ecris("pub const LIEUX: &[Lieu] = &[")
    for l in k["LIEUX"]:
        ecris(
            f"    Lieu {{ emoji: {chaine(l['emoji'])}, nom: {chaine(l['nom'])} }},"
        )
    ecris("];")
    ecris()

    ecris("/// Une région téléchargeable, avec son poids.")
    ecris("#[derive(Clone, Copy, PartialEq)]")
    ecris("pub struct Region {")
    ecris("    pub nom: &'static str,")
    ecris("    pub poids: &'static str,")
    ecris("    /// Pourcentage téléchargé. `None` quand elle est déjà installée —")
    ecris("    /// ce que le TypeScript disait par l'absence du champ.")
    ecris("    pub avancement: Option<u32>,")
    ecris("}")
    ecris()
    ecris("pub const MAQUETTE_REGIONS: &[Region] = &[")
    for r in k["MAQUETTE_REGIONS"]:
        av = f"Some({r['avancement']})" if r.get("avancement") is not None else "None"
        ecris(
            f"    Region {{ nom: {chaine(r['nom'])}, poids: {chaine(r['poids'])}, "
            f"avancement: {av} }},"
        )
    ecris("];")
    ecris()

    ecris("/// Les outils, par domaine — affichés dans « Ce qu'il y a dessous ».")
    ecris("pub const OUTILS: &[&[&str]] = &[")
    for groupe in k["OUTILS"]:
        ecris("    " + liste(groupe, chaine) + ",")
    ecris("];")
    ecris()

    ecris("/// Une source de données, créditée en pied de page.")
    ecris("#[derive(Clone, Copy, PartialEq)]")
    ecris("pub struct Source {")
    ecris("    pub nom: &'static str,")
    ecris("    pub url: &'static str,")
    ecris("}")
    ecris()
    ecris("pub const SOURCES: &[Source] = &[")
    for s in k["SOURCES"]:
        ecris(f"    Source {{ nom: {chaine(s['nom'])}, url: {chaine(s['url'])} }},")
    ecris("];")
    ecris()

    # --- Les trois langues -----------------------------------------------
    for nom, c in zip(("FR", "ES", "EN"), langues):
        ecris(f"// --- {c['nom']} {'-' * (64 - len(c['nom']))}")
        ecris()
        ecris(f"pub const {nom}: Contenu = Contenu {{")
        ecris(f"    code: {chaine(c['code'])},")
        ecris(f"    chemin: {chaine(c['chemin'])},")
        ecris(f"    nom: {chaine(c['nom'])},")
        ecris(f"    titre: {chaine(c['titre'])},")
        ecris(f"    description: {chaine(c['description'])},")
        ecris(f"    h1: {liste(c['h1'], chaine)},")
        ecris(f"    chapeau: {chaine(c['chapeau'])},")
        ecris(f"    se_connecter: {chaine(c['seConnecter'])},")
        ecris(
            "    chevrons: Chevrons { descendre: "
            + chaine(c["chevrons"]["descendre"])
            + ", remonter: "
            + chaine(c["chevrons"]["remonter"])
            + " },"
        )
        ecris(
            "    theme: Theme { libelle: "
            + chaine(c["theme"]["libelle"])
            + ", noms: "
            + liste(c["theme"]["noms"], chaine)
            + " },"
        )
        ecris(f"    langue_libelle: {chaine(c['langueLibelle'])},")
        ecris(
            "    chiffres: "
            + multi(
                c["chiffres"],
                lambda n: f"Chiffre {{ valeur: {chaine(n['valeur'])}, legende: {chaine(n['legende'])} }}",
                4,
            )
            + ","
        )
        cp = c["comparateur"]
        ecris("    comparateur: SectionApres {")
        ecris(f"        titre: {chaine(cp['titre'])},")
        ecris(f"        intro: {chaine(cp['intro'])},")
        ecris(f"        apres: {chaine(cp['apres'])},")
        ecris("    },")
        for champ, cle in (
            ("guidage", "guidage"),
            ("montre", "montre"),
            ("lieux", "lieux"),
            ("hors_ligne", "horsLigne"),
        ):
            s = c[cle]
            ecris(f"    {champ}: Section {{")
            ecris(f"        titre: {chaine(s['titre'])},")
            ecris(f"        intro: {chaine(s['intro'])},")
            ecris("    },")
        ecris(f"    atouts_titre: {chaine(c['atoutsTitre'])},")

        def atout(a):
            return (
                "Atout {\n            emoji: "
                + chaine(a["emoji"])
                + ",\n            titre: "
                + chaine(a["titre"])
                + ",\n            texte: "
                + chaine(a["texte"])
                + ",\n        }"
            )

        ecris("    atouts: &[")
        for a in c["atouts"]:
            ecris("        " + atout(a) + ",")
        ecris("    ],")
        ecris("    hors_ligne_cartes: &[")
        for a in c["horsLigneCartes"]:
            ecris("        " + atout(a) + ",")
        ecris("    ],")

        t = c["tarifs"]
        ecris("    tarifs: Tarifs {")
        ecris(f"        titre: {chaine(t['titre'])},")
        ecris(f"        intro: {chaine(t['intro'])},")
        for f in ("plus", "pro"):
            o = t[f]
            ecris(f"        {f}: Formule {{")
            ecris(f"            nom: {chaine(o['nom'])},")
            ecris(f"            prix: {chaine(o['prix'])},")
            ecris(f"            periode: {chaine(o['periode'])},")
            ecris(f"            points: {multi(o['points'], chaine, 12)},")
            ecris("        },")
        ecris(f"        note: {chaine(t['note'])},")
        ecris("    },")

        p = c["preuve"]
        ecris("    preuve: Preuve {")
        ecris(f"        titre: {chaine(p['titre'])},")
        ecris(f"        texte: {chaine(p['texte'])},")
        ecris(f"        mesure: {chaine(p['mesure'])},")
        ecris(f"        note: {chaine(p['note'])},")
        ecris("    },")
        ecris(f"    pile_titre: {chaine(c['pileTitre'])},")
        ecris("    pile: &[")
        for pi in c["pile"]:
            ecris(
                "        Pile { domaine: "
                + chaine(pi["domaine"])
                + ", outils: "
                + liste(pi["outils"], chaine)
                + " },"
            )
        ecris("    ],")
        ecris(f"    franchise_titre: {chaine(c['franchiseTitre'])},")
        ecris(f"    franchise: {multi(c['franchise'], chaine, 4)},")
        lg = c["legendes"]
        ecris("    legendes: Legendes {")
        for ch in ("itineraires", "guidage", "montre", "lieux", "regions"):
            ecris(f"        {ch}: {chaine(lg[ch])},")
        ecris("    },")
        ecris(f"    reste: {chaine(c['reste'])},")
        ecris(
            "    maquette: Maquette { installee: "
            + chaine(c["maquette"]["installee"])
            + " },"
        )
        ecris(f"    signature: {chaine(c['signature'])},")
        ecris(f"    credits: {chaine(c['credits'])},")
        pd = c["pied"]
        ecris("    pied: Pied {")
        ecris(f"        slogan: {chaine(pd['slogan'])},")
        ecris(f"        decouvrir: {chaine(pd['decouvrir'])},")
        ecris(
            "        ancres: "
            + multi(
                pd["ancres"],
                lambda a: f"Ancre {{ id: {chaine(a['id'])}, nom: {chaine(a['nom'])} }}",
                8,
            )
            + ","
        )
        ecris(f"        donnees: {chaine(pd['donnees'])},")
        ecris(f"        legal_titre: {chaine(pd['legalTitre'])},")
        ecris(f"        copyright: {chaine(pd['copyright'])},")
        ecris("    },")
        for champ, cle in (("mentions", "mentions"), ("confidentialite", "confidentialite")):
            pg = c[cle]
            ecris(f"    {champ}: PageLegale {{")
            ecris(f"        chemin: {chaine(pg['chemin'])},")
            ecris(f"        titre: {chaine(pg['titre'])},")
            ecris(f"        description: {chaine(pg['description'])},")
            ecris(f"        retour: {chaine(pg['retour'])},")
            ecris("        sections: &[")
            for s in pg["sections"]:
                ecris("            SectionLegale {")
                ecris(f"                titre: {chaine(s['titre'])},")
                ecris(f"                corps: {multi(s['corps'], chaine, 16)},")
                ecris("            },")
            ecris("        ],")
            ecris("    },")
        ecris("};")
        ecris()

    sys.stdout.write("\n".join(RUST) + "\n")


if __name__ == "__main__":
    main()
