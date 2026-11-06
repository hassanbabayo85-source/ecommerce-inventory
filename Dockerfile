# syntax=docker/dockerfile:1.7
#
# Multi-stage build:
#   chef    → installs system deps + pre-compiles the dependency graph
#   builder → compiles our actual crate
#   runtime → minimal debian + binary + non-root user
#
# Cargo.lock is committed and honoured via --locked on every cargo call,
# so image builds are reproducible.

# ---- Stage 1: dependency cache ----
FROM rust:1-slim-bookworm AS chef

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cargo.lock is committed → use exact name (no glob)
COPY Cargo.toml Cargo.lock ./

# Minimal stub so cargo can resolve and compile the dependency graph once.
RUN mkdir -p src && \
    echo 'fn main() {}' > src/main.rs && \
    echo '' > src/lib.rs

# `|| true` is intentional: the stub binary fails because lib.rs is empty,
# but the dependency artifacts are already compiled and cached. Only used
# to prime the cache layer — real errors surface in the next stage.
RUN cargo build --release --locked --bin ecommerce_inventory || true

# ---- Stage 2: build real binary ----
FROM chef AS builder

COPY src ./src
COPY migrations ./migrations

# touch forces cargo to recompile our crate (stub build cached an empty one)
RUN touch src/main.rs src/lib.rs && \
    cargo build --release --locked --bin ecommerce_inventory

# ---- Stage 3: runtime ----
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /bin/bash --uid 10001 appuser

WORKDIR /app
COPY --from=builder /app/target/release/ecommerce_inventory /app/ecommerce_inventory

USER appuser

ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost:8080/health || exit 1

ENTRYPOINT ["/app/ecommerce_inventory"]
