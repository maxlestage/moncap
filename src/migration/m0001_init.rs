use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

/// Schéma initial : tables `users` et `positions` (+ colonne `user_id`).
///
/// DDL idempotent (`IF NOT EXISTS`) pour rester compatible avec une base
/// déjà créée par les anciennes versions de l'application.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS users (\
                id SERIAL PRIMARY KEY,\
                username TEXT UNIQUE NOT NULL,\
                password_hash TEXT NOT NULL\
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS positions (\
                id SERIAL PRIMARY KEY,\
                lat DOUBLE PRECISION NOT NULL,\
                lon DOUBLE PRECISION NOT NULL,\
                label TEXT NOT NULL\
            )",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE positions ADD COLUMN IF NOT EXISTS user_id INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS positions")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS users").await?;
        Ok(())
    }
}
