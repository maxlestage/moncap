use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

/// Ajoute la table `searches` : les recherches de destination récentes,
/// enregistrées par utilisateur (synchronisées entre appareils).
///
/// DDL idempotent (`IF NOT EXISTS`) comme les migrations précédentes.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS searches (\
                id SERIAL PRIMARY KEY,\
                name TEXT NOT NULL,\
                subtitle TEXT NOT NULL DEFAULT '',\
                lat DOUBLE PRECISION NOT NULL,\
                lon DOUBLE PRECISION NOT NULL,\
                created_at BIGINT NOT NULL DEFAULT 0,\
                user_id INTEGER NOT NULL DEFAULT 0\
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS searches_user_id_idx ON searches (user_id)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS searches")
            .await?;
        Ok(())
    }
}
