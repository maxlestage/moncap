use sea_orm_migration::prelude::*;

mod m0001_init;

/// Liste ordonnée des migrations appliquées au démarrage.
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m0001_init::Migration)]
    }
}
