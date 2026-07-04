use sea_orm_migration::prelude::*;

mod m0001_init;
mod m0002_trips;
mod m0003_searches;
mod m0004_positions_index;

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
        ]
    }
}
