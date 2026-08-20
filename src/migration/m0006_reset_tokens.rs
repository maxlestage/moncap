use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

/// Ajoute la table `reset_tokens` : les jetons de réinitialisation de mot de
/// passe, envoyés par courriel. Seule l'empreinte SHA-256 du jeton est
/// stockée.
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
                "CREATE TABLE IF NOT EXISTS reset_tokens (\
                    id SERIAL PRIMARY KEY,\
                    user_id INTEGER NOT NULL,\
                    token_hash TEXT UNIQUE NOT NULL,\
                    expires_at BIGINT NOT NULL,\
                    used BOOLEAN NOT NULL DEFAULT FALSE\
                )",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS reset_tokens")
            .await?;
        Ok(())
    }
}
