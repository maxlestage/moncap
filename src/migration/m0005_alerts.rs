use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

/// Ajoute la table `alerts` : signalements communautaires persistés
/// (avec expiration et votes), au lieu du stockage en mémoire.
///
/// DDL idempotent (`IF NOT EXISTS`) comme les migrations précédentes.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS alerts (\
                id SERIAL PRIMARY KEY,\
                category TEXT NOT NULL,\
                label TEXT NOT NULL DEFAULT '',\
                lat DOUBLE PRECISION NOT NULL,\
                lon DOUBLE PRECISION NOT NULL,\
                created_at BIGINT NOT NULL DEFAULT 0,\
                expires_at BIGINT NOT NULL DEFAULT 0,\
                confirms INTEGER NOT NULL DEFAULT 0,\
                denies INTEGER NOT NULL DEFAULT 0,\
                user_id INTEGER NOT NULL DEFAULT 0\
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS alerts_expires_at_idx ON alerts (expires_at)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS alerts").await?;
        Ok(())
    }
}
