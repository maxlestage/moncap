use sea_orm::entity::prelude::*;

/// Table `trips` : un trajet parcouru et enregistré par l'utilisateur.
///
/// Le tracé est stocké sous forme d'une suite de points `lat,lon` séparés par
/// des `;` (`polyline`), pour rester simple à (dé)coder côté client.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize)]
#[sea_orm(table_name = "trips")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub label: String,
    /// Distance parcourue en kilomètres.
    pub distance_km: f64,
    /// Durée du trajet en minutes.
    pub duration_min: f64,
    /// Tracé : `lat,lon;lat,lon;…`.
    pub polyline: String,
    /// Date d'enregistrement (secondes Unix).
    pub created_at: i64,
    /// Trajet **professionnel** : seuls ceux-là entrent dans le barème
    /// fiscal et dans la facturation. Faux par défaut — un trajet que
    /// personne n'a qualifié ne doit pas gonfler une déclaration.
    pub pro: bool,
    /// Le motif, libre : « livraison Dupont », « chantier Mérignac ». Un
    /// journal kilométrique se défend ligne à ligne, et c'est le motif qui
    /// le permet — pas la distance.
    pub motif: String,
    /// Propriétaire du trajet (non exposé dans le JSON).
    #[serde(skip)]
    pub user_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
