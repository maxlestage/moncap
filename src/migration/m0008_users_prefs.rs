use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

/// Les préférences du compte — Domicile, Travail — en JSON sur l'utilisateur :
/// un nouvel iPhone retrouve ses favoris, ils appartiennent au compte, pas à
/// l'appareil. Un blob plutôt que des colonnes : les préférences grossiront,
/// le schéma n'a pas à courir après.
///
/// DDL idempotent (`IF NOT EXISTS`) comme les migrations précédentes.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE users ADD COLUMN IF NOT EXISTS prefs TEXT")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE users DROP COLUMN IF EXISTS prefs")
            .await?;
        Ok(())
    }
}
