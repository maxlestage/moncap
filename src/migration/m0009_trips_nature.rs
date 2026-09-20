use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

/// La nature d'un trajet — professionnel ou personnel — et son motif.
///
/// Sans elle, le journal kilométrique n'a aucune valeur : le barème fiscal ne
/// s'applique qu'aux trajets **professionnels**, et un journal qui compte les
/// courses du samedi avec les tournées de la semaine sur-déclare. Ce n'est pas
/// une lacune de confort, c'est un risque pour qui s'en sert devant
/// l'administration.
///
/// `pro` est faux par défaut, et c'est délibéré : un trajet existant n'a
/// jamais été qualifié par personne, et le compter comme professionnel
/// gonflerait rétroactivement des déclarations. Mieux vaut sous-compter et
/// laisser l'utilisateur cocher que sur-compter dans son dos.
///
/// `motif` reste libre : « livraison Dupont », « chantier Mérignac ». Le
/// journal doit pouvoir se défendre ligne à ligne, et c'est le motif qui le
/// permet — pas la distance.
///
/// DDL idempotent (`IF NOT EXISTS`) comme les migrations précédentes.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE trips ADD COLUMN IF NOT EXISTS pro BOOLEAN NOT NULL DEFAULT FALSE",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE trips ADD COLUMN IF NOT EXISTS motif TEXT NOT NULL DEFAULT ''",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE trips DROP COLUMN IF EXISTS pro")
            .await?;
        db.execute_unprepared("ALTER TABLE trips DROP COLUMN IF EXISTS motif")
            .await?;
        Ok(())
    }
}
