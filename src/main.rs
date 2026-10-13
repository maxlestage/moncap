//! MonCap GPS — le serveur, sur Paquets.

mod alertes;
mod app;
mod base;
mod commun;
mod compte;
mod connexion;
mod erreur;
mod externes;
mod partage;
mod pmtiles;
mod positions;
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
    app::application(app::Reglages::production()).run()
}
