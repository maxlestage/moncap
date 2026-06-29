# --- Étape de build ---
FROM rust:1-bookworm AS builder
WORKDIR /app

# Compile d'abord les dépendances (cache Docker) avec un main factice.
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

# Puis le vrai code.
COPY backend/src ./src
RUN touch src/main.rs && cargo build --release

# --- Image finale ---
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/moncap-gps /usr/local/bin/moncap-gps

# Heroku fournit PORT et DATABASE_URL au runtime.
CMD ["moncap-gps"]
