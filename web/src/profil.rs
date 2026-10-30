//! Ce que le navigateur retient sans compte : le nom choisi, et le partage
//! de position en cours.
//!
//! Gardé par le magasin de Mémoire (`paquets::orm::web::Store`), dans
//! `localStorage` sous `memoire:store:moncap.profil` : relu à la visite
//! suivante, tenu à jour entre les onglets. Rien n'en part au serveur, sauf
//! le nom, quand on partage — c'est lui que voit la page de suivi.

use paquets::orm::web::Store;
use serde::{Deserialize, Serialize};

/// La longueur d'un nom : de quoi tenir un prénom et un nom, comme l'app.
pub const NOM_MAX: usize = 40;

/// Le profil du navigateur.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Profil {
    /// Le nom montré à ceux qui suivent la position. Vide : « Un ami ».
    #[serde(default)]
    pub nom: String,
    /// Le partage ouvert, s'il y en a un : il reprend après un rechargement.
    #[serde(default)]
    pub partage: Option<PartageEnCours>,
}

/// Un partage ouvert auprès du serveur.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartageEnCours {
    /// Le jeton du lien de suivi (`/live/<jeton>`).
    pub jeton: String,
    /// Le secret qui seul permet de déplacer ou d'arrêter le partage.
    #[serde(default)]
    pub secret: String,
    /// Le début du partage, en millisecondes depuis l'époque Unix.
    pub depuis_ms: f64,
}

impl Profil {
    /// Le nom tel qu'il part au serveur : sans espaces autour, borné, et
    /// « Un ami » quand il n'y en a pas.
    pub fn nom_affiche(&self) -> String {
        let nom = nettoie_nom(&self.nom);
        if nom.is_empty() {
            "Un ami".into()
        } else {
            nom
        }
    }
}

/// Un nom saisi : sans espaces autour, ni caractères de contrôle, borné à
/// [`NOM_MAX`] caractères.
pub fn nettoie_nom(saisi: &str) -> String {
    saisi
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(NOM_MAX)
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Le magasin du profil — le même partout où on l'appelle.
pub fn profil() -> Store<Profil> {
    Store::new("moncap.profil")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_nom_propre_et_borne() {
        assert_eq!(nettoie_nom("  Max \n"), "Max");
        assert_eq!(nettoie_nom("a\u{7}b"), "ab");
        assert_eq!(nettoie_nom(&"x".repeat(80)).chars().count(), NOM_MAX);
    }

    #[test]
    fn sans_nom_un_ami() {
        assert_eq!(Profil::default().nom_affiche(), "Un ami");
        let p = Profil {
            nom: " Léa ".into(),
            partage: None,
        };
        assert_eq!(p.nom_affiche(), "Léa");
    }

    #[test]
    fn un_profil_ancien_se_relit() {
        // Un champ ajouté plus tard ne fait pas perdre le reste.
        let p: Profil = serde_json::from_str(r#"{"nom":"Max"}"#).unwrap();
        assert_eq!(p.nom, "Max");
        assert_eq!(p.partage, None);
    }
}
