# ---- Stage 1: build ----
FROM rust:1-slim-bookworm AS builder

WORKDIR /app

# System deps needed to compile sqlx/tokio native bits.
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependency compilation: copy only manifests first, build a throwaway main.rs,
# then copy the real source. This means `docker build` only recompiles the app's own
# code (fast) when only src/ changes, not the whole dependency tree.
COPY Cargo.toml Cargo.lock* ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && \
    mkdir -p src/lib_stub && echo "" > src/lib.rs
RUN cargo build --release --bin ecommerce_inventory || true
RUN rm -rf src

# Now copy the real source and migrations (migrations are embedded into the binary at
# compile time via sqlx::migrate!, so they must be present at build time).
COPY src ./src
COPY migrations ./migrations

# Force a fresh build of our own crate (touch main.rs so cargo doesn't skip it because of
# the stub-build cache above).
RUN touch src/main.rs src/lib.rs && cargo build --release --bin ecommerce_inventory

# ---- Stage 2: runtime ----
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /bin/bash appuser

WORKDIR /app
COPY --from=builder /app/target/release/ecommerce_inventory /app/ecommerce_inventory

USER appuser

ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080

ENTRYPOINT ["/app/ecommerce_inventory"]
