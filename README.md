# E-commerce Inventory and Order System

A Rust backend for managing **Products**, **Orders**, and **stock/inventory**, built with
Axum + Tokio + PostgreSQL (sqlx), including a background job that automatically cancels
unpaid orders and restores stock.

## Tech Stack

| Concern              | Tool                                                      |
|-----------------------|------------------------------------------------------------|
| HTTP framework          | [Axum](https://github.com/tokio-rs/axum) 0.7              |
| Async runtime             | [Tokio](https://tokio.rs) (full features)                  |
| Database                    | PostgreSQL + [sqlx](https://github.com/launchbadge/sqlx) (runtime-checked queries, not compile-time `query!` macros, so the image can be built without a live DB) |
| Background job                | `tokio::time::interval`, runs every 1 minute                |
| Containerization                | Docker (multi-stage) + docker-compose                    |

## Folder Structure

```
src/
  main.rs              # entrypoint: config, pool, migrations, background job, HTTP server
  lib.rs                # crate root: AppState, build_app(), run_migrations()
  errors.rs             # AppError -> IntoResponse (JSON error mapping)
  models/
    product.rs           # Product, CreateProductRequest, UpdateProductRequest
    order.rs              # Order, OrderItem, OrderStatus, CreateOrderRequest
  db/
    pool.rs                # PgPool builder
    products.rs             # Product CRUD queries
    orders.rs                # *Core logic*: atomic create_order, pay_order, cancel_stale_pending_orders
  handlers/
    products.rs               # HTTP handlers -> db::products
    orders.rs                  # HTTP handlers -> db::orders
  jobs/
    cancel_pending.rs            # tokio::spawn loop that calls db::orders::cancel_stale_pending_orders
migrations/
  0001_init.sql                  # products, orders, order_items, order_status enum
tests/
  common/mod.rs                   # test helpers (pool + seed)
  stock_concurrency.rs             # *Most important*: race-condition tests
  background_job.rs                 # sweeper tests (cancel + restore stock)
  http_api.rs                        # end-to-end HTTP tests
```

## How the Transaction/Locking Works (most important part)

### The problem being solved

If two people try to buy the **last unit** of a product at the same time, without protection,
both could read `stock_quantity = 1`, both decide there's enough stock, both decrement it, and
stock ends up at `-1` — **overselling**.

### The solution used

In `db/orders.rs`, the `create_order` function:

1. **Opens a database transaction** (`pool.begin()`). Everything below — checking, decrementing,
   creating the order and order_items — happens inside a single transaction. If any step fails,
   we return early (`return Err(...)`) before `tx.commit()`, and sqlx **auto-rolls-back** the
   transaction (dropping a `Transaction` without calling `commit()` = rollback) — no partial
   change is ever persisted to the database.

2. **`SELECT price, stock_quantity FROM products WHERE id = $1 FOR UPDATE`** — this is the key
   to race-condition safety. `FOR UPDATE` **locks the row** at the database level until the
   transaction either `COMMIT`s or `ROLLBACK`s. If a second concurrent request tries to
   `FOR UPDATE` the same row, PostgreSQL makes it **block/wait** until the first transaction
   finishes — so the second request's read of stock will reflect the already-decremented value,
   not the stale one.

3. **Deterministic (sorted) lock ordering** — if an order contains more than one item, we merge
   them into a `BTreeMap<Uuid, i32>` (which is sorted by `product_id`) before locking anything.
   This prevents **deadlocks**: if order A contains products 1 and 2, and order B contains 2 and
   1, and each locked them in a different order (A: 1→2, B: 2→1), they could deadlock waiting on
   each other. By guaranteeing every order locks products in the same order (by UUID), this can
   never happen.

4. **Duplicate product_ids within a single order are merged** before locking, so the same row is
   never locked/updated twice inside one transaction.

5. Once we've confirmed there's enough stock for **every** item (if even one fails, we return the
   error immediately — nothing has been written yet), we:
   - `INSERT` the order (status = `pending`)
   - For each item: `UPDATE products SET stock_quantity = stock_quantity - $1 ...` then
     `INSERT` the order_item
   - `tx.commit()` — now every change is persisted together, either all of it or none of it.

### Confirm Payment (`pay_order`) and the Background Job

`pay_order` and `cancel_stale_pending_orders` (the background job) also use
`SELECT status FROM orders WHERE id = $1 FOR UPDATE` before changing status. This prevents a
race between a user clicking "pay" at the exact moment the background job is trying to cancel
the same order because it's been pending for over 15 minutes — one of them locks the order row,
the other waits, then re-checks the status (which may have already changed) before proceeding.

### Background Job (`jobs/cancel_pending.rs`)

- Uses `tokio::time::interval` to run every **1 minute**.
- For every order with status `pending` that has been unpaid for over **15 minutes**:
  1. Locks the order row (`FOR UPDATE`) and re-checks its status — if it was already paid
     between the time it was flagged as "stale" and the time it got locked, it's left alone
     (not cancelled).
  2. Locks the products involved in the order (in the same deterministic order as #3 above) and
     **restores the stock** (`stock_quantity = stock_quantity + quantity`).
  3. Changes the order status to `cancelled`.
  4. Each order is processed in **its own transaction**, so one problematic order can't block
     the rest of the sweep.

## Endpoints

| Method | Path                          | Description                                            |
|--------|-------------------------------|----------------------------------------------------------|
| POST   | `/api/products`                | Create a product                                          |
| GET    | `/api/products`                 | List all products                                          |
| GET    | `/api/products/:id`              | Get a single product                                        |
| PUT    | `/api/products/:id`               | Update a product (partial update)                            |
| DELETE | `/api/products/:id`                | Delete a product                                              |
| GET    | `/api/products/low-stock`           | Products where stock ≤ low_stock_threshold                    |
| POST   | `/api/orders`                        | Create an order (atomic, with stock check + row locking)      |
| GET    | `/api/orders/:id`                     | Full order details + its items                                |
| POST   | `/api/orders/:id/pay`                  | Simulated payment confirmation (pending → paid)                |
| GET    | `/health`                                | Health check                                                   |

## Running It

### Option 1: `setup.sh` (easiest)

```bash
chmod +x setup.sh
./setup.sh
```

This will:
1. Create every project file/folder (if not already present).
2. Create `.env` from `.env.example`.
3. `docker-compose up --build -d` (Postgres + app).
4. Wait until the app is healthy (migrations run automatically on app startup via
   `sqlx::migrate!`).
5. Seed sample products via `curl` calls to `/api/products`.
6. Print a success message with example `curl` commands for trying the API.

### Option 2: Docker Compose directly

```bash
cp .env.example .env
docker compose up --build -d
docker compose logs -f app
```

### Option 3: Run locally (no Docker)

```bash
export DATABASE_URL=postgres://ecommerce:ecommerce@localhost:5432/ecommerce
cargo run
```

## Tests

You need a reachable PostgreSQL instance via `TEST_DATABASE_URL` (or `DATABASE_URL`):

```bash
export TEST_DATABASE_URL=postgres://ecommerce:ecommerce@localhost:5432/ecommerce
cargo test -- --test-threads=1
```

> **Important**: `--test-threads=1` matters because each integration test `TRUNCATE`s the
> shared tables (products/orders/order_items) at the start — running them in parallel would let
> tests interfere with each other.

The most important tests live in `tests/stock_concurrency.rs`:
- `concurrent_orders_never_oversell_stock` — fires **20 concurrent requests** at a product with
  stock=5, and asserts that **exactly 5** succeed, the other 15 get `InsufficientStock`, and the
  final stock lands exactly at **0** (never negative).
- `insufficient_stock_rolls_back_cleanly` — no order or order_item is written if stock isn't
  enough.
- `multi_item_order_rolls_back_all_products_on_partial_failure` — if one item in a multi-item
  order fails, EVERY product in that order rolls back (not just the failing one).

This project was built and verified against a real local PostgreSQL instance during
development: `cargo check`/`cargo build` compile cleanly, the app boots and applies migrations,
and `cargo test -- --test-threads=1` passes all 8 tests, including the 20-way concurrency test
above.

## Environment Variables

See `.env.example`:

```
POSTGRES_USER=ecommerce
POSTGRES_PASSWORD=ecommerce
POSTGRES_DB=ecommerce
POSTGRES_PORT=5432
APP_PORT=8080
DATABASE_URL=postgres://ecommerce:ecommerce@localhost:5432/ecommerce
RUST_LOG=ecommerce_inventory=info,tower_http=info
```

## License

MIT License.
