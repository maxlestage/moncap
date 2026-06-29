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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
