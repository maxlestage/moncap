use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

/// Ajoute la table `trips` : les trajets parcourus, enregistrés par utilisateur.
///
/// DDL idempotent (`IF NOT EXISTS`) comme la migration initiale.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS trips (\
                id SERIAL PRIMARY KEY,\
                label TEXT NOT NULL,\
                distance_km DOUBLE PRECISION NOT NULL DEFAULT 0,\
                duration_min DOUBLE PRECISION NOT NULL DEFAULT 0,\
                polyline TEXT NOT NULL DEFAULT '',\
                created_at BIGINT NOT NULL DEFAULT 0,\
                user_id INTEGER NOT NULL DEFAULT 0\
            )",
        )
        .await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS trips_user_id_idx ON trips (user_id)")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS trips").await?;
        Ok(())
    }
}
