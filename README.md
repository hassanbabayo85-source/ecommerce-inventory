# E-commerce Inventory and Order System

[![CI](https://github.com/hassanbabayo85-source/ecommerce-inventory/actions/workflows/ci.yml/badge.svg)](https://github.com/hassanbabayo85-source/ecommerce-inventory/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.83%2B-orange?logo=rust)](https://www.rust-lang.org)
[![Postgres](https://img.shields.io/badge/postgres-16-336791?logo=postgresql)](https://www.postgresql.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-46%20passing-brightgreen)](tests/)

A production-oriented Rust backend for **Products**, **Orders**, and
**Inventory**, built with **Axum + Tokio + PostgreSQL (sqlx)**.

Highlights:

- **Atomic orders** with `SELECT ... FOR UPDATE`, deterministic lock ordering,
  and per-row locking — 20 concurrent requests never oversell.
- **Per-order access tokens** on top of the UUID — IDOR/BOLA safe.
- **Idempotency keys** on `POST /api/orders` — retries are safe.
- **Inventory ledger** — every stock change is recorded with a reason,
  reconcilable against current stock (`SUM(movements) == stock`).
- **Background sweeper** — cancels stale pending orders and restores stock,
  guarded by a Postgres advisory lock (multi-instance safe).
- **OpenAPI / Swagger UI** — interactive documentation at `/api/docs`.
- **Structured JSON logging** — set `LOG_FORMAT=json` for machine-readable logs.
- **Prometheus metrics** — `/metrics` endpoint with HTTP and business metrics.
- **Production Docker stack** — Caddy auto-HTTPS, isolated networks,
  `read_only` app FS, resource limits, `/ready` healthchecks.

## Tech stack

| Concern              | Tool                                              |
|----------------------|---------------------------------------------------|
| HTTP framework       | Axum 0.7                                          |
| Async runtime        | Tokio (full features)                             |
| Database             | PostgreSQL 16 + sqlx (runtime-checked queries)    |
| Background jobs      | `tokio::time::interval` (60 s tick)               |
| Containerization     | Docker (multi-stage) + Compose (dev + prod)       |
| Reverse proxy        | Caddy 2 (auto-HTTPS, security headers)            |

## Quick start

### Option 1 — Docker Compose (dev)

    cp .env.example .env
    # edit .env (or accept defaults for a local stack)
    docker compose up --build -d
    docker compose logs -f app

App: <http://localhost:8080/health> · <http://localhost:8080/ready> · <http://localhost:8080/metrics> (Prometheus metrics)

**Interactive API Docs**: <http://localhost:8080/api/docs> (Swagger UI)

### Option 2 — Local (no Docker)

    cp .env.example .env
    # edit .env:
    #   DATABASE_URL=postgres://<user>:<pass>@localhost:5432/ecommerce
    #   ADMIN_API_KEY=<random 32+ chars>
    cargo run

Migrations run automatically on startup (`sqlx::migrate!`).

### Option 3 — Production

See [docs/DEPLOY.md](docs/DEPLOY.md).

## API

### Public

| Method | Path                          | Notes                                          |
|--------|-------------------------------|------------------------------------------------|
| GET    | `/health`                     | Liveness — plain `ok`.                         |
| GET    | `/ready`                      | Readiness — pings DB, returns 200 or 503.      |
| GET    | `/api/products`               | Paginated (`?limit=50&offset=0`, max 200).     |
| GET    | `/api/products/low-stock`     | `stock_quantity <= low_stock_threshold`.       |
| GET    | `/api/products/:id`           | Single product.                                |
| POST   | `/api/orders`                 | Create order. Supports `Idempotency-Key`.      |
| GET    | `/api/orders/:id`             | Requires `X-Order-Token` (from create response). |

### Admin (`X-API-Key: <ADMIN_API_KEY>`)

| Method | Path                                | Notes                                       |
|--------|-------------------------------------|---------------------------------------------|
| POST   | `/api/products`                     | Create product.                             |
| PUT    | `/api/products/:id`                 | Partial update. Stock delta is ledgered.    |
| DELETE | `/api/products/:id`                 | Refused (409) if any order or movement refs.|
| POST   | `/api/orders/:id/pay`               | Simulated payment (`pending → paid`).       |
| GET    | `/api/products/:id/movements`       | Per-product ledger history.                 |
| GET    | `/api/inventory/movements`          | All movements, paginated.                   |
| POST   | `/api/inventory/adjustments`        | Manual stock adjustment (with `note`).      |

## How order creation works

`create_order` in `src/db/orders.rs`:

1. Merges duplicate `product_id` line items into a `BTreeMap` (sorted by UUID)
   using `checked_add` (no `i32` overflow).
2. Opens a DB transaction.
3. For each product (in sorted UUID order — **deadlock-safe**):
   `SELECT price, stock_quantity FROM products WHERE id = $1 FOR UPDATE`.
4. If any line item lacks stock → return early → transaction rolls back.
5. `INSERT INTO orders`, `UPDATE products SET stock_quantity = stock_quantity - qty`,
   `INSERT INTO inventory_movements (change = -qty, reason = 'order_reservation')`,
   `INSERT INTO order_items`.
6. If `Idempotency-Key` was provided, insert the mapping in the same transaction.
7. `COMMIT`.

Guarantees:

- **Exactly 5 of 20** concurrent requests succeed against `stock = 5` — no
  negative stock, no double-sell, no partial writes.
- **Retries** with the same `Idempotency-Key` return the original order without
  decrementing stock again.
- **Ledger reconciliation**: for any product, `SUM(change)` equals current
  `stock_quantity`. If they ever diverge, you have a bug — the invariant is
  enforced by tests.

## Inventory ledger

`inventory_movements` is append-only. Every write to `products.stock_quantity`
happens in the same transaction as a corresponding `inventory_movements` row.

| Reason                | When                                              | `change`       |
|-----------------------|---------------------------------------------------|----------------|
| `initial`             | `create_product` (stock > 0)                      | `+stock`       |
| `order_reservation`   | `create_order`                                    | `-qty`         |
| `order_cancellation`  | stale-order sweeper                               | `+qty`         |
| `admin_adjustment`    | `update_product` (if delta ≠ 0) or `adjust_stock` | `delta`        |
| `stock_sync`          | reserved for future reconciliation jobs           | —              |

## Testing

Requires a reachable Postgres. Set `TEST_DATABASE_URL` (or `DATABASE_URL`):

    export TEST_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:5432/ecommerce_test"
    cargo test -- --test-threads=1

`--test-threads=1` is required: each test TRUNCATEs shared tables, so parallel
execution would cause flaky failures.

### Highlights

- **`stock_concurrency::concurrent_orders_never_oversell_stock`** — 20
  concurrent requests against stock=5, exactly 5 succeed, stock lands at 0.
- **`phase3_inventory::ledger_reconciles_with_current_stock`** — mixes orders,
  cancellations, and manual adjustments, then asserts `SUM(movements) == stock`.
- **`phase1_security::order_without_token_returns_not_found`** — UUID alone is
  not enough; `access_token` is mandatory.
- **`multi_worker::two_concurrent_sweeps_do_not_double_restock`** — two
  concurrent background sweeps restore stock exactly once.

46 tests total across 10 binaries.

## Security

See [SECURITY.md](SECURITY.md) for the vulnerability-reporting policy and the
full hardening checklist.

Key measures:

- Order ownership via per-order `access_token` (BOLA / IDOR fix).
- Idempotency keys on order creation.
- CORS fail-closed in production (`APP_ENV=production`).
- Admin routes gated by `X-API-Key` with constant-time comparison.
- Postgres unreachable from host in prod compose (`internal: true`).
- Caddy sets HSTS, X-Content-Type-Options, X-Frame-Options, Referrer-Policy,
  Permissions-Policy; caps request body at 1 MB.
- Containers run non-root with `read_only` root FS, `no-new-privileges`,
  memory/CPU limits.

## Deployment

See [docs/DEPLOY.md](docs/DEPLOY.md). TL;DR:

    cp .env.prod.example .env.prod
    # generate strong secrets:
    #   openssl rand -hex 32   (POSTGRES_PASSWORD)
    #   openssl rand -hex 32   (ADMIN_API_KEY)
    # edit DOMAIN and ACME_EMAIL in .env.prod
    docker compose -f docker-compose.prod.yml --env-file .env.prod up -d --build

First HTTPS request to `https://$DOMAIN/health` triggers Let's Encrypt
provisioning.

## Repository layout

    src/
      main.rs                   entrypoint: config, pool, migrations, jobs, HTTP server
      lib.rs                    crate root: AppState, build_app(), run_migrations()
      errors.rs                 AppError -> IntoResponse (JSON error mapping)
      models/
        product.rs              Product, Create/UpdateProductRequest
        order.rs                Order, OrderItem, OrderStatus, CreateOrderRequest
        inventory.rs            InventoryMovement, AdjustStockRequest
      db/
        pool.rs                 PgPool builder (min 2 connections)
        products.rs             Product CRUD, ledger on stock changes
        orders.rs               create_order (locked), pay_order, cancel_stale_pending_orders
        inventory.rs            ledger queries + adjust_stock
      handlers/
        products.rs             HTTP handlers for /api/products
        orders.rs               HTTP handlers for /api/orders
        inventory.rs            HTTP handlers for movements + adjustments
        health.rs               /health (liveness) and /ready (readiness)
      jobs/
        cancel_pending.rs       tokio loop that calls cancel_stale_pending_orders
    migrations/
      0001_init.sql             products, orders, order_items, order_status enum
      0002_order_access_token.sql   orders.access_token + unique index
      0003_idempotency.sql      idempotency_keys table
      0004_inventory_movements.sql  append-only inventory ledger
    tests/
      common/mod.rs             shared test helpers (pool, truncate, seed)
      stock_concurrency.rs      20-way oversell test — the most important one
      phase1_security.rs        C1/C3/C4/C6 regression tests
      phase3_inventory.rs       ledger + reconciliation tests
      background_job.rs         sweeper tests (cancel, restore, race with pay)
      multi_worker.rs           advisory lock (two sweeps, no double-restock)
      product_update_race.rs    update_product vs create_order, no stock resurrection
      http_api.rs               end-to-end /health, /ready, order flow
      orders_edge.rs            validation + pay/cancel race
      product_validation.rs     SKU uniqueness, non-negative thresholds, ...
      auth.rs                   admin API-key enforcement
    docs/
      DEPLOY.md                 production deployment guide
    Dockerfile                  multi-stage build, non-root runtime
    docker-compose.yml          local dev stack
    docker-compose.prod.yml     production stack (Caddy + isolated networks)
    Caddyfile                   reverse-proxy config for the prod stack
    .env.example                local-dev env template
    .env.prod.example           production env template (CHANGE_ME placeholders)

## License

MIT — see [LICENSE](LICENSE).
