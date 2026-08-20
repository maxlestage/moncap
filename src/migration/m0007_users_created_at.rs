use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

/// Date de création sur les comptes : c'est elle qui ancre l'essai de
/// quatorze jours — côté serveur, parce que l'horloge d'un téléphone se
/// triche et pas la nôtre. Les comptes existants sont datés d'aujourd'hui :
/// leur essai démarre à la mise en service de la règle, pas rétroactivement.
///
/// DDL idempotent (`IF NOT EXISTS`) comme les migrations précédentes.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE users ADD COLUMN IF NOT EXISTS created_at BIGINT \
                 NOT NULL DEFAULT CAST(EXTRACT(EPOCH FROM NOW()) AS BIGINT)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE users DROP COLUMN IF EXISTS created_at")
            .await?;
        Ok(())
    }
}
