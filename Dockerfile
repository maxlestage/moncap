# Image complète et autonome pour l'API moncap-gps (Rust + Axum).
# Contexte de build attendu : la racine du dépôt.

# --- Étape de build ---
FROM rust:1-bookworm AS builder
WORKDIR /app

# 1) Compile uniquement les dépendances (mises en cache) avec un main factice.
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src

# 2) Compile le vrai code.
COPY backend/src ./src
RUN touch src/main.rs && cargo build --release

# --- Image finale (légère) ---
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/moncap-gps /usr/local/bin/moncap-gps

# Heroku (ou tout hébergeur) fournit PORT et DATABASE_URL au runtime ;
# le schéma Postgres est créé automatiquement au démarrage.
EXPOSE 3000
CMD ["moncap-gps"]
