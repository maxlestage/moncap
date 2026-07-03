use sea_orm::entity::prelude::*;

/// Table `searches` : une recherche de destination mémorisée par l'utilisateur.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize)]
#[sea_orm(table_name = "searches")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Nom du lieu (ex. « 124 Rue Billaudel »).
    pub name: String,
    /// Sous-titre / adresse complète.
    pub subtitle: String,
    pub lat: f64,
    pub lon: f64,
    /// Date de la recherche (secondes Unix).
    pub created_at: i64,
    /// Propriétaire (non exposé dans le JSON).
    #[serde(skip)]
    pub user_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
