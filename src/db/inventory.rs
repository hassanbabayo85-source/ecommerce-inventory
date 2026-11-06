use crate::errors::{AppError, AppResult};
use crate::models::inventory::{InventoryMovement, REASON_ADMIN_ADJUSTMENT};
use sqlx::PgPool;
use uuid::Uuid;

/// Inserts a movement row. MUST be called inside the same transaction as the
/// stock update so the ledger and stock cannot drift apart.
pub(crate) async fn log_movement(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    product_id: Uuid,
    change: i32,
    reason: &str,
    reference_id: Option<Uuid>,
    note: Option<&str>,
) -> AppResult<()> {
    if change == 0 {
        return Ok(());
    }
    sqlx::query(
        r#"
        INSERT INTO inventory_movements (product_id, change, reason, reference_id, note)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(product_id)
    .bind(change)
    .bind(reason)
    .bind(reference_id)
    .bind(note)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn list_product_movements(
    pool: &PgPool,
    product_id: Uuid,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<InventoryMovement>> {
    let rows = sqlx::query_as::<_, InventoryMovement>(
        r#"
        SELECT id, product_id, change, reason, reference_id, note, created_at
        FROM inventory_movements
        WHERE product_id = $1
        ORDER BY created_at DESC, id
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(product_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_all_movements(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<InventoryMovement>> {
    let rows = sqlx::query_as::<_, InventoryMovement>(
        r#"
        SELECT id, product_id, change, reason, reference_id, note, created_at
        FROM inventory_movements
        ORDER BY created_at DESC, id
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Admin-driven manual stock change. Locks the product row, validates the
/// resulting stock is non-negative, updates it, and logs one movement.
pub async fn adjust_stock(
    pool: &PgPool,
    product_id: Uuid,
    change: i32,
    note: Option<String>,
) -> AppResult<InventoryMovement> {
    if change == 0 {
        return Err(AppError::Validation("change must be non-zero".into()));
    }
    if let Some(ref n) = note {
        if n.len() > 1000 {
            return Err(AppError::Validation(
                "note must be ≤ 1000 characters".into(),
            ));
        }
    }

    let mut tx = pool.begin().await?;

    let row: Option<(i32,)> =
        sqlx::query_as("SELECT stock_quantity FROM products WHERE id = $1 FOR UPDATE")
            .bind(product_id)
            .fetch_optional(&mut *tx)
            .await?;

    let (current,) =
        row.ok_or_else(|| AppError::NotFound(format!("product {product_id} not found")))?;

    let new_stock = current.checked_add(change).ok_or_else(|| {
        AppError::Validation(format!(
            "stock adjustment overflows i32: {current} + {change}"
        ))
    })?;

    if new_stock < 0 {
        return Err(AppError::InvalidState(format!(
            "adjustment would make stock negative: {current} + {change} = {new_stock}"
        )));
    }

    sqlx::query("UPDATE products SET stock_quantity = $1, updated_at = now() WHERE id = $2")
        .bind(new_stock)
        .bind(product_id)
        .execute(&mut *tx)
        .await?;

    let movement = sqlx::query_as::<_, InventoryMovement>(
        r#"
        INSERT INTO inventory_movements (product_id, change, reason, note)
        VALUES ($1, $2, $3, $4)
        RETURNING id, product_id, change, reason, reference_id, note, created_at
        "#,
    )
    .bind(product_id)
    .bind(change)
    .bind(REASON_ADMIN_ADJUSTMENT)
    .bind(note.as_deref())
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(movement)
}
