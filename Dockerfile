# Image Docker optionnelle (usage local). Le déploiement Heroku se fait
# désormais via buildpack (voir Procfile + app.json), pas via ce Dockerfile.
# Contexte de build attendu : la racine du dépôt.

# --- Étape de build ---
FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src

COPY src ./src
RUN touch src/main.rs && cargo build --release

# --- Image finale (légère) ---
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/moncap-gps /usr/local/bin/moncap-gps

EXPOSE 3000
CMD ["moncap-gps"]
