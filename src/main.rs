//! MonCap GPS — le serveur, sur Paquets.

mod alertes;
mod app;
mod base;
mod cache;
mod commun;
mod compte;
mod connexion;
mod erreur;
mod externes;
mod partage;
mod pmtiles;
mod positions;
mod sauvegarde;
mod statique;
mod temps_reel;
mod trajets;
mod tuiles;

#[cfg(test)]
mod essais;

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let fin = app::application(app::Reglages::production()).run();
    // Heroku laisse trente secondes après SIGTERM : le dernier cliché de la
    // base a le temps de partir.
    sauvegarde::attends_dernier_envoi(std::time::Duration::from_secs(20));
    fin
}
