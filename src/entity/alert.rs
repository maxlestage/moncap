use sea_orm::entity::prelude::*;

/// Table `alerts` : un signalement communautaire (police, accident, bouchon…),
/// persisté avec expiration et votes de confirmation.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "alerts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub category: String,
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    /// Création (secondes Unix).
    pub created_at: i64,
    /// Expiration (secondes Unix) — prolongée par les confirmations.
    pub expires_at: i64,
    /// Votes « toujours là ».
    pub confirms: i32,
    /// Votes « plus là ».
    pub denies: i32,
    /// Auteur du signalement.
    pub user_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
