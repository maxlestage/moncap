use sea_orm_migration::prelude::*;

mod m0001_init;
mod m0002_trips;
mod m0003_searches;
mod m0004_positions_index;
mod m0005_alerts;
mod m0006_reset_tokens;
mod m0007_users_created_at;
mod m0008_users_prefs;
mod m0009_trips_nature;

/// Liste ordonnée des migrations appliquées au démarrage.
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m0001_init::Migration),
            Box::new(m0002_trips::Migration),
            Box::new(m0003_searches::Migration),
            Box::new(m0004_positions_index::Migration),
            Box::new(m0005_alerts::Migration),
            Box::new(m0006_reset_tokens::Migration),
            Box::new(m0007_users_created_at::Migration),
            Box::new(m0008_users_prefs::Migration),
            Box::new(m0009_trips_nature::Migration),
        ]
    }
}
