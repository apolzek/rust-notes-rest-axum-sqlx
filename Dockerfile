# syntax=docker/dockerfile:1.7
# ---- Build stage ----
FROM rust:1.94-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Dummy main to pre-build deps
RUN mkdir src && echo "fn main(){}" > src/main.rs \
    && cargo build --release \
    && rm -rf src target/release/deps/raditzlawliet_rust_notes_rest*

# Now copy real sources and build
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

# ---- Runtime stage ----
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 1001 app

WORKDIR /app

COPY --from=builder /app/target/release/raditzlawliet_rust-notes-rest-axum-sqlx /app/api
COPY migrations ./migrations

USER app
EXPOSE 8080

HEALTHCHECK --interval=10s --timeout=3s --start-period=20s --retries=5 \
  CMD curl -fsS http://127.0.0.1:8080/api/v1/healthcheck || exit 1

CMD ["/app/api"]
