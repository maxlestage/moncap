use sea_orm::entity::prelude::*;

/// Table `reset_tokens` : les jetons de réinitialisation de mot de passe.
///
/// Le jeton envoyé par courriel n'est jamais stocké — seulement son
/// empreinte SHA-256 : une base compromise ne permet pas de rejouer les
/// liens déjà envoyés. Un jeton vaut une heure, et une seule fois.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "reset_tokens")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub user_id: i32,
    #[sea_orm(unique)]
    pub token_hash: String,
    /// Expiration en secondes Unix — une heure après l'envoi.
    pub expires_at: i64,
    pub used: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
