//! Les listes fixes de l'interface : les signalements et les avatars.
//!
//! Elles vivaient dans deux fichiers TypeScript, et la carte gardait sa
//! propre copie des émojis : deux tables qui devaient bouger ensemble et
//! que rien ne liait. Il n'y en a plus qu'une.

use std::collections::HashMap;

use serde::Deserialize;

/// Un type de signalement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Categorie {
    /// Le nom envoyé au serveur — celui de l'app iOS.
    pub id: &'static str,
    pub emoji: &'static str,
    pub nom: &'static str,
}

pub const CATEGORIES: [Categorie; 8] = [
    Categorie {
        id: "police",
        emoji: "🚓",
        nom: "Police",
    },
    Categorie {
        id: "accident",
        emoji: "💥",
        nom: "Accident",
    },
    Categorie {
        id: "bouchon",
        emoji: "🚧",
        nom: "Bouchon",
    },
    Categorie {
        id: "danger",
        emoji: "⚠️",
        nom: "Danger",
    },
    Categorie {
        id: "vehicule",
        emoji: "🚘",
        nom: "Véhicule arrêté",
    },
    Categorie {
        id: "objet",
        emoji: "📦",
        nom: "Objet sur la route",
    },
    Categorie {
        id: "travaux",
        emoji: "🏗️",
        nom: "Travaux",
    },
    Categorie {
        id: "brouillard",
        emoji: "🌫️",
        nom: "Brouillard",
    },
];

/// La catégorie d'un signalement reçu. Une catégorie inconnue — envoyée par
/// une version plus récente de l'app — s'affiche en danger générique plutôt
/// que de disparaître.
pub fn categorie(id: &str) -> Categorie {
    CATEGORIES
        .iter()
        .copied()
        .find(|c| c.id == id)
        .unwrap_or(Categorie {
            id: "danger",
            emoji: "⚠️",
            nom: "Signalement",
        })
}

/// Un avatar : un combi ou une voiture de course.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Avatar {
    pub id: &'static str,
    pub nom: &'static str,
}

/// Les avatars, dans l'ordre de l'ancien sélecteur. L'identifiant est celui
/// que l'app iOS envoie au temps réel : il doit rester tel quel.
pub const AVATARS: [Avatar; 14] = [
    Avatar {
        id: "green",
        nom: "Combi vert",
    },
    Avatar {
        id: "orange",
        nom: "Combi orange",
    },
    Avatar {
        id: "blue",
        nom: "Combi bleu",
    },
    Avatar {
        id: "mint",
        nom: "Combi menthe",
    },
    Avatar {
        id: "ferrari",
        nom: "Ferrari",
    },
    Avatar {
        id: "alpine",
        nom: "Alpine",
    },
    Avatar {
        id: "merc1",
        nom: "Mercedes argent",
    },
    Avatar {
        id: "merc2",
        nom: "Mercedes noire",
    },
    Avatar {
        id: "red1",
        nom: "F1 rouge",
    },
    Avatar {
        id: "yarisGrey",
        nom: "GR Yaris gris",
    },
    Avatar {
        id: "yarisWhite",
        nom: "GR Yaris blanc",
    },
    Avatar {
        id: "yarisBlue",
        nom: "GR Yaris bleu",
    },
    Avatar {
        id: "abarthWhite",
        nom: "Abarth 500 blanc",
    },
    Avatar {
        id: "abarthRed",
        nom: "Abarth 595 rouge",
    },
];

/// Les adresses des fichiers, que seule la construction connaît : elles
/// portent l'empreinte de leur contenu. La coquille HTML les transmet en
/// JSON sur le point de montage.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Ressources {
    #[serde(default)]
    pub logo: String,
    #[serde(default)]
    pub avatars: HashMap<String, String>,
}

impl Ressources {
    /// L'image d'un avatar ; le premier si l'identifiant est inconnu —
    /// venu d'une autre version de l'app, ou d'un stockage abîmé.
    pub fn avatar(&self, id: &str) -> String {
        self.avatars
            .get(id)
            .or_else(|| self.avatars.get(AVATARS[0].id))
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn les_identifiants_sont_uniques() {
        let c: HashSet<_> = CATEGORIES.iter().map(|c| c.id).collect();
        assert_eq!(c.len(), CATEGORIES.len());
        let a: HashSet<_> = AVATARS.iter().map(|a| a.id).collect();
        assert_eq!(a.len(), AVATARS.len());
    }

    #[test]
    fn une_categorie_inconnue_reste_visible() {
        assert_eq!(categorie("police").emoji, "🚓");
        assert_eq!(categorie("ovni").emoji, "⚠️");
    }

    #[test]
    fn un_avatar_inconnu_retombe_sur_le_premier() {
        let r: Ressources =
            serde_json::from_str(r#"{"logo":"/l.webp","avatars":{"green":"/g.webp"}}"#).unwrap();
        assert_eq!(r.avatar("green"), "/g.webp");
        assert_eq!(r.avatar("inconnu"), "/g.webp");
    }

    /// Chaque avatar listé doit avoir son image : un oubli ici se verrait
    /// sous la forme d'une image cassée, chez les autres, sur la carte.
    #[test]
    fn chaque_avatar_a_son_image() {
        let dossier = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/avatars");
        for a in AVATARS {
            let f = dossier.join(format!("{}.webp", a.id));
            assert!(f.exists(), "image manquante : {f:?}");
        }
    }
}
