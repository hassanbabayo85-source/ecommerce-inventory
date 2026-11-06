# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned

- OpenAPI / Swagger spec at `/api/docs`
- Structured JSON logging mode
- `/api/orders/:id/cancel` public endpoint (customer-initiated cancel)
- Multi-currency support
- Webhook dispatch on order status changes

## [0.1.0] — 2026-11-06

First tagged release. Ships Phases 1–4 of the security / production roadmap.

### Added

- `inventory_movements` ledger table — append-only audit trail of every stock
  change with reason (`initial`, `order_reservation`, `order_cancellation`,
  `admin_adjustment`, `stock_sync`), optional `reference_id` and `note`.
- `GET  /api/products/:id/movements` — per-product movement history (admin).
- `GET  /api/inventory/movements` — all movements, paginated (admin).
- `POST /api/inventory/adjustments` — manual stock adjustment (admin) with
  transaction-level validation and ledger entry.
- `GET  /ready` — readiness probe with DB ping (200 / 503).
- `Idempotency-Key` header support on `POST /api/orders`.
- Order `access_token` returned once at creation; required as `X-Order-Token`
  on subsequent `GET /api/orders/:id`.
- `docs/DEPLOY.md` — production deployment guide.
- Production Docker stack (`docker-compose.prod.yml`) with Caddy auto-HTTPS,
  isolated networks, resource limits, `read_only` app FS,
  `no-new-privileges`, `/ready` healthchecks.
- `SECURITY.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `LICENSE`.
- CI workflow: `cargo fmt`, `cargo clippy -D warnings`, `cargo test` against
  a Postgres service container.

### Changed

- **BREAKING**: `GET /api/orders/:id` now requires `X-Order-Token` header
  matching the token returned by `POST /api/orders`. Clients that stored only
  the order UUID will need to store the access token too.
- **BREAKING**: `DELETE /api/products/:id` returns **409 Conflict** instead of
  **500** when the product is referenced by any order line item *or* any
  inventory movement. Hard-deleting a product with audit history is refused.
- CORS: in production (`APP_ENV=production` or `CORS_STRICT=true`), the app
  **panics at startup** if `CORS_ALLOWED_ORIGINS` is empty, contains `*`, or
  contains any invalid origin. Dev mode still warns-and-falls-back.
- `DB_MAX_CONNECTIONS` is clamped to `>= 2` (background sweep holds one
  connection for its advisory lock and needs a second for the sweep).
- `Order` JSON shape is now flat via `#[serde(flatten)]` — `id`,
  `customer_email`, etc. sit at the top level, alongside `items`.
- Docker image now uses a 3-stage build with `cargo-chef`-style caching and
  honours `--locked` on every cargo invocation.

### Fixed

- **BOLA / IDOR**: `GET /api/orders/:id` previously returned any order given
  its UUID. UUIDs are not secrets. Now requires `access_token`.
- **CORS fail-open**: empty or malformed `CORS_ALLOWED_ORIGINS` silently fell
  back to `CorsLayer::permissive()` in every environment. Now fail-closed in
  production.
- **Integer overflow**: merging duplicate `product_id` line items in
  `create_order` could wrap `i32` silently. Now uses `checked_add` and returns
  `400 Bad Request` with the offending product id.
- **Product deletion 500**: deleting a product referenced by an existing order
  raised a Postgres FK violation → 500. Now pre-checked and returns 409.
- **DB pool starvation**: with `DB_MAX_CONNECTIONS=1` the sweep worker could
  deadlock waiting for a connection it was itself holding. Minimum is now 2.

### Tests

- 46 integration tests, all green (`cargo test -- --test-threads=1`).
- New suites: `phase1_security.rs` (8 tests), `phase3_inventory.rs` (11 tests).
- Concurrency: 20 concurrent orders never oversell; concurrent pay + cancel
  never double-restocks; two concurrent sweeps never double-restock.

[Unreleased]: https://github.com/hassanbabayo85-source/ecommerce-inventory/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/hassanbabayo85-source/ecommerce-inventory/releases/tag/v0.1.0
