use crate::errors::{AppError, AppResult};
use crate::models::order::{CreateOrderRequest, Order, OrderItem, OrderStatus, OrderWithItems};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

/// Creates an order and reduces stock atomically.
///
/// Concurrency safety:
/// 1. Everything runs inside a single DB transaction (`pool.begin()`), so either the whole
///    operation (stock check + stock decrement + order + order_items) commits, or none of it
///    does (automatic ROLLBACK on early return / drop of the transaction).
/// 2. Each product row involved is locked with `SELECT ... FOR UPDATE` *before* we read its
///    stock. Any other transaction trying to lock the same product row (e.g. a second,
///    concurrent request buying the last unit) blocks until this transaction commits or rolls
///    back — so two concurrent requests can never both "see" the same stock and both succeed
///    when only one unit is available.
/// 3. Product ids are locked in a fixed, sorted order (by UUID) across the whole order. This
///    prevents classic deadlocks that would occur if two concurrent orders each contain the
///    same two products but request their locks in opposite order.
/// 4. Duplicate product ids within a single order request are merged (quantities summed) before
///    locking, so we only ever lock/update each row once per transaction.
pub async fn create_order(pool: &PgPool, req: CreateOrderRequest) -> AppResult<OrderWithItems> {
    if req.customer_email.trim().is_empty() {
        return Err(AppError::Validation("customer_email is required".into()));
    }
    if req.items.is_empty() {
        return Err(AppError::Validation(
            "order must contain at least one item".into(),
        ));
    }
    for item in &req.items {
        if item.quantity <= 0 {
            return Err(AppError::Validation(format!(
                "quantity for product {} must be positive",
                item.product_id
            )));
        }
    }

    // Merge duplicate product ids and lock in a stable (sorted) order to avoid deadlocks.
    let mut merged: BTreeMap<Uuid, i32> = BTreeMap::new();
    for item in &req.items {
        *merged.entry(item.product_id).or_insert(0) += item.quantity;
    }

    let mut tx = pool.begin().await?;

    let mut total_amount = Decimal::ZERO;
    // (product_id, quantity, unit_price)
    let mut line_items: Vec<(Uuid, i32, Decimal)> = Vec::with_capacity(merged.len());

    for (product_id, quantity) in &merged {
        // Row-level lock: blocks concurrent transactions from reading/decrementing this
        // product's stock until we commit or roll back.
        let row: Option<(Decimal, i32)> =
            sqlx::query_as("SELECT price, stock_quantity FROM products WHERE id = $1 FOR UPDATE")
                .bind(product_id)
                .fetch_optional(&mut *tx)
                .await?;

        let (price, available) =
            row.ok_or_else(|| AppError::NotFound(format!("product {product_id} not found")))?;

        if available < *quantity {
            // Returning here drops `tx` without calling commit(), which rolls the
            // transaction back automatically — no partial stock decrement is ever persisted.
            return Err(AppError::InsufficientStock {
                product_id: *product_id,
                requested: *quantity,
                available,
            });
        }

        total_amount += price * Decimal::from(*quantity);
        line_items.push((*product_id, *quantity, price));
    }

    let order: Order = sqlx::query_as::<_, Order>(
        r#"
        INSERT INTO orders (customer_email, status, total_amount)
        VALUES ($1, 'pending', $2)
        RETURNING id, customer_email, status, total_amount, created_at, updated_at
        "#,
    )
    .bind(&req.customer_email)
    .bind(total_amount)
    .fetch_one(&mut *tx)
    .await?;

    let mut items: Vec<OrderItem> = Vec::with_capacity(line_items.len());
    for (product_id, quantity, unit_price) in line_items {
        sqlx::query("UPDATE products SET stock_quantity = stock_quantity - $1, updated_at = now() WHERE id = $2")
            .bind(quantity)
            .bind(product_id)
            .execute(&mut *tx)
            .await?;

        let item = sqlx::query_as::<_, OrderItem>(
            r#"
            INSERT INTO order_items (order_id, product_id, quantity, unit_price)
            VALUES ($1, $2, $3, $4)
            RETURNING id, order_id, product_id, quantity, unit_price
            "#,
        )
        .bind(order.id)
        .bind(product_id)
        .bind(quantity)
        .bind(unit_price)
        .fetch_one(&mut *tx)
        .await?;

        items.push(item);
    }

    tx.commit().await?;

    Ok(OrderWithItems { order, items })
}

pub async fn get_order_with_items(pool: &PgPool, id: Uuid) -> AppResult<OrderWithItems> {
    let order = sqlx::query_as::<_, Order>(
        r#"
        SELECT id, customer_email, status, total_amount, created_at, updated_at
        FROM orders
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("order {id} not found")))?;

    let items = sqlx::query_as::<_, OrderItem>(
        r#"
        SELECT id, order_id, product_id, quantity, unit_price
        FROM order_items
        WHERE order_id = $1
        ORDER BY id
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    Ok(OrderWithItems { order, items })
}

/// Confirms payment for a pending order (simulation — no real payment gateway).
/// Uses `SELECT ... FOR UPDATE` on the order row so this can't race with the background
/// cancellation job trying to cancel the same order at the same time.
pub async fn pay_order(pool: &PgPool, id: Uuid) -> AppResult<Order> {
    let mut tx = pool.begin().await?;

    let current: Option<(OrderStatus,)> =
        sqlx::query_as("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    let (status,) = current.ok_or_else(|| AppError::NotFound(format!("order {id} not found")))?;

    if status != OrderStatus::Pending {
        return Err(AppError::InvalidState(format!(
            "order {id} cannot be paid from status '{status}' (must be 'pending')"
        )));
    }

    let order = sqlx::query_as::<_, Order>(
        r#"
        UPDATE orders
        SET status = 'paid', updated_at = now()
        WHERE id = $1
        RETURNING id, customer_email, status, total_amount, created_at, updated_at
        "#,
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(order)
}

/// Background-job helper: finds pending orders older than `timeout_minutes` that have not
/// been paid, cancels them, and restores the stock that was reserved for them — all inside
/// one transaction per order so a crash mid-way never leaves stock permanently "lost".
///
/// Returns the ids of orders that were cancelled.
/// Advisory lock key — arbitrary fixed i64.
///
/// Prevents more than one instance from running the sweep at the same time.
/// `pg_try_advisory_lock` is non-blocking: if another worker holds the lock we
/// simply skip this tick and try again 60s later.
///
/// Acquired and released on the *same* pool connection (advisory locks are
/// per-session in Postgres), so we hold one connection from the pool during
/// the sweep. With max_connections >= 2 this leaves headroom for business
/// queries; we document the >= 2 requirement in README.
const SWEEP_ADVISORY_LOCK_ID: i64 = 0x4543_4F4D_4D45_5243;

/// Background-job entry point. Wraps `do_sweep` with a cross-instance advisory lock.
pub async fn cancel_stale_pending_orders(
    pool: &PgPool,
    timeout_minutes: i64,
) -> AppResult<Vec<Uuid>> {
    let mut conn = pool.acquire().await?;

    let got: (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock($1)")
        .bind(SWEEP_ADVISORY_LOCK_ID)
        .fetch_one(&mut *conn)
        .await?;

    if !got.0 {
        tracing::debug!("another instance holds the sweep advisory lock; skipping this tick");
        return Ok(Vec::new());
    }

    let result = do_sweep(pool, timeout_minutes).await;

    // Always release, even if do_sweep failed. Ignore unlock errors.
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SWEEP_ADVISORY_LOCK_ID)
        .execute(&mut *conn)
        .await;

    result
}

async fn do_sweep(pool: &PgPool, timeout_minutes: i64) -> AppResult<Vec<Uuid>> {
    // Find candidate order ids outside the per-order transaction (cheap, read-only).
    let stale_ids: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id FROM orders
        WHERE status = 'pending'
          AND created_at < now() - ($1 || ' minutes')::interval
        ORDER BY created_at ASC
        LIMIT 1000
        "#,
    )
    .bind(timeout_minutes.to_string())
    .fetch_all(pool)
    .await?;

    let mut cancelled = Vec::new();

    for (order_id,) in stale_ids {
        // Each order is cancelled + restocked in its own transaction so one bad order
        // can't block the rest, and so restore-stock + status-change stay atomic.
        let mut tx = pool.begin().await?;

        // Lock the order row and re-check status: it may have been paid concurrently
        // between our read above and now.
        let current: Option<(OrderStatus,)> =
            sqlx::query_as("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
                .bind(order_id)
                .fetch_optional(&mut *tx)
                .await?;

        let Some((status,)) = current else {
            continue;
        };
        if status != OrderStatus::Pending {
            // Got paid (or already cancelled) in the meantime — nothing to do.
            tx.rollback().await.ok();
            continue;
        }

        let items: Vec<(Uuid, i32)> =
            sqlx::query_as("SELECT product_id, quantity FROM order_items WHERE order_id = $1")
                .bind(order_id)
                .fetch_all(&mut *tx)
                .await?;

        // Lock product rows in sorted order (same deadlock-avoidance rule as create_order).
        let mut sorted_items = items;
        sorted_items.sort_by_key(|(pid, _)| *pid);

        for (product_id, quantity) in &sorted_items {
            sqlx::query("SELECT id FROM products WHERE id = $1 FOR UPDATE")
                .bind(product_id)
                .fetch_optional(&mut *tx)
                .await?;

            sqlx::query(
                "UPDATE products SET stock_quantity = stock_quantity + $1, updated_at = now() WHERE id = $2",
            )
            .bind(quantity)
            .bind(product_id)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query("UPDATE orders SET status = 'cancelled', updated_at = now() WHERE id = $1")
            .bind(order_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        cancelled.push(order_id);
    }

    Ok(cancelled)
}
