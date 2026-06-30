use sea_orm::entity::prelude::*;

/// Table `positions` : une position GPS enregistrée.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize)]
#[sea_orm(table_name = "positions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub lat: f64,
    pub lon: f64,
    pub label: String,
    /// Propriétaire de la position (non exposé dans le JSON).
    #[serde(skip)]
    pub user_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
