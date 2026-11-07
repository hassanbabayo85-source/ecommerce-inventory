use crate::errors::{AppError, AppResult};
use crate::models::inventory::{REASON_ORDER_CANCELLATION, REASON_ORDER_RESERVATION};
use crate::models::order::{
    CreateOrderRequest, CreateOrderResponse, Order, OrderItem, OrderStatus, OrderWithItems,
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

fn fingerprint(req: &CreateOrderRequest) -> String {
    let mut items: Vec<_> = req
        .items
        .iter()
        .map(|i| (i.product_id.to_string(), i.quantity))
        .collect();
    items.sort();
    format!("{}|{:?}", req.customer_email.trim().to_lowercase(), items)
}

pub async fn create_order(
    pool: &PgPool,
    req: CreateOrderRequest,
    idempotency_key: Option<String>,
) -> AppResult<CreateOrderResponse> {
    let email = req.customer_email.trim();
    if email.is_empty() {
        return Err(AppError::Validation("customer_email is required".into()));
    }
    // Basic email validation: must have an '@' and a '.' after the '@'
    if !email.contains('@') || !email.split('@').nth(1).unwrap_or("").contains('.') {
        return Err(AppError::Validation("invalid customer_email format".into()));
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

    if let Some(ref key) = idempotency_key {
        if let Some(existing) = load_by_idempotency(pool, key, &req).await? {
            return Ok(existing);
        }
    }

    let mut merged: BTreeMap<Uuid, i32> = BTreeMap::new();
    for item in &req.items {
        let slot = merged.entry(item.product_id).or_insert(0);
        *slot = slot.checked_add(item.quantity).ok_or_else(|| {
            AppError::Validation(format!(
                "quantity for product {} overflows i32",
                item.product_id
            ))
        })?;
    }

    let mut tx = pool.begin().await?;

    let mut total_amount = Decimal::ZERO;
    let mut line_items: Vec<(Uuid, i32, Decimal)> = Vec::with_capacity(merged.len());

    for (product_id, quantity) in &merged {
        let row: Option<(Decimal, i32)> =
            sqlx::query_as("SELECT price, stock_quantity FROM products WHERE id = $1 FOR UPDATE")
                .bind(product_id)
                .fetch_optional(&mut *tx)
                .await?;

        let (price, available) =
            row.ok_or_else(|| AppError::NotFound(format!("product {product_id} not found")))?;

        if available < *quantity {
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
        RETURNING id, customer_email, status, total_amount, created_at, updated_at, access_token
        "#,
    )
    .bind(&req.customer_email)
    .bind(total_amount)
    .fetch_one(&mut *tx)
    .await?;

    let mut items: Vec<OrderItem> = Vec::with_capacity(line_items.len());
    for (product_id, quantity, unit_price) in line_items {
        sqlx::query(
            "UPDATE products SET stock_quantity = stock_quantity - $1, updated_at = now() WHERE id = $2",
        )
        .bind(quantity)
        .bind(product_id)
        .execute(&mut *tx)
        .await?;

        // Phase 3: ledger entry — same transaction as stock decrement.
        sqlx::query(
            r#"
            INSERT INTO inventory_movements (product_id, change, reason, reference_id)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(product_id)
        .bind(-quantity)
        .bind(REASON_ORDER_RESERVATION)
        .bind(order.id)
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

    if let Some(ref key) = idempotency_key {
        let fp = fingerprint(&req);
        let email_lower = req.customer_email.trim().to_lowercase();
        let res = sqlx::query("INSERT INTO idempotency_keys (key, order_id, customer_email, request_fingerprint) VALUES ($1, $2, $3, $4) ON CONFLICT (key) DO NOTHING")
            .bind(key)
            .bind(order.id)
            .bind(email_lower)
            .bind(fp)
            .execute(&mut *tx)
            .await?;

        if res.rows_affected() == 0 {
            // Wani request ne ya riga ya saka key din a lokaci guda.
            // Mu soke wannan order din da muka kirkira, sannan mu dawo da wanda ke akwai.
            tx.rollback().await?;
            return load_by_idempotency(pool, key, &req).await?.ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!(
                    "Idempotency conflict but original order not found"
                ))
            });
        }
    }

    tx.commit().await?;

    Ok(CreateOrderResponse {
        access_token: order.access_token,
        order,
        items,
    })
}

async fn load_by_idempotency(
    pool: &PgPool,
    key: &str,
    req: &CreateOrderRequest,
) -> AppResult<Option<CreateOrderResponse>> {
    let existing: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT order_id, customer_email, request_fingerprint FROM idempotency_keys WHERE key = $1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some((oid, stored_email, stored_fp)) = existing else {
        return Ok(None);
    };

    let current_fp = fingerprint(req);
    if stored_email != req.customer_email.trim().to_lowercase() || stored_fp != current_fp {
        return Err(AppError::InvalidState(
            "Idempotency-Key reused with a different payload or customer".into(),
        ));
    }

    let order = sqlx::query_as::<_, Order>(
        r#"
        SELECT id, customer_email, status, total_amount, created_at, updated_at, access_token
        FROM orders
        WHERE id = $1
        "#,
    )
    .bind(oid)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("order {oid} not found")))?;

    let items = sqlx::query_as::<_, OrderItem>(
        r#"
        SELECT id, order_id, product_id, quantity, unit_price
        FROM order_items
        WHERE order_id = $1
        ORDER BY id
        "#,
    )
    .bind(oid)
    .fetch_all(pool)
    .await?;

    Ok(Some(CreateOrderResponse {
        access_token: order.access_token,
        order,
        items,
    }))
}

pub async fn get_order_with_items(
    pool: &PgPool,
    id: Uuid,
    access_token: Option<Uuid>,
) -> AppResult<OrderWithItems> {
    let order = sqlx::query_as::<_, Order>(
        r#"
        SELECT id, customer_email, status, total_amount, created_at, updated_at, access_token
        FROM orders
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("order {id} not found")))?;

    let authorized = matches!(access_token, Some(tok) if tok == order.access_token);
    if !authorized {
        return Err(AppError::NotFound(format!("order {id} not found")));
    }

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
        RETURNING id, customer_email, status, total_amount, created_at, updated_at, access_token
        "#,
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(order)
}

const SWEEP_ADVISORY_LOCK_ID: i64 = 0x4543_4F4D_4D45_5243;

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

    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SWEEP_ADVISORY_LOCK_ID)
        .execute(&mut *conn)
        .await;

    result
}

async fn do_sweep(pool: &PgPool, timeout_minutes: i64) -> AppResult<Vec<Uuid>> {
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
        let mut tx = pool.begin().await?;

        let current: Option<(OrderStatus,)> =
            sqlx::query_as("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
                .bind(order_id)
                .fetch_optional(&mut *tx)
                .await?;

        let Some((status,)) = current else {
            continue;
        };
        if status != OrderStatus::Pending {
            tx.rollback().await.ok();
            continue;
        }

        let items: Vec<(Uuid, i32)> =
            sqlx::query_as("SELECT product_id, quantity FROM order_items WHERE order_id = $1")
                .bind(order_id)
                .fetch_all(&mut *tx)
                .await?;

        let mut sorted_items = items;
        sorted_items.sort_by_key(|(pid, _)| *pid);

        for (product_id, quantity) in &sorted_items {
            sqlx::query("SELECT id FROM products WHERE id = $1 FOR UPDATE")
                .bind(product_id)
                .fetch_optional(&mut *tx)
                .await?;

            let update_res = sqlx::query(
                "UPDATE products SET stock_quantity = stock_quantity + $1, updated_at = now() WHERE id = $2",
            )
            .bind(quantity)
            .bind(product_id)
            .execute(&mut *tx)
            .await;

            if let Err(e) = update_res {
                if let sqlx::Error::Database(ref db_err) = e {
                    if db_err.code().as_deref() == Some("22003") {
                        return Err(AppError::Validation(format!(
                            "stock restoration overflow for product {}",
                            product_id
                        )));
                    }
                }
                return Err(AppError::Database(e));
            }

            // Phase 3: ledger entry — same transaction as stock restoration.
            sqlx::query(
                r#"
                INSERT INTO inventory_movements (product_id, change, reason, reference_id)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(product_id)
            .bind(quantity)
            .bind(REASON_ORDER_CANCELLATION)
            .bind(order_id)
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
