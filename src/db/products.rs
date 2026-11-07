use crate::db::inventory::log_movement;
use crate::errors::{AppError, AppResult};
use crate::models::inventory::{REASON_ADMIN_ADJUSTMENT, REASON_INITIAL};
use crate::models::product::{CreateProductRequest, Product, UpdateProductRequest};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

fn map_product_db_error(e: sqlx::Error, sku: &str) -> AppError {
    if let sqlx::Error::Database(ref db_err) = e {
        match db_err.code().as_deref() {
            Some("23505") => {
                return AppError::Validation(format!("sku '{}' already exists", sku));
            }
            Some("23514") => {
                return AppError::Validation(format!(
                    "database check violation: {}",
                    db_err.message()
                ));
            }
            _ => {}
        }
    }
    AppError::Database(e)
}

fn validate_product_fields(
    name: &str,
    sku: &str,
    price: Decimal,
    stock_quantity: i32,
    low_stock_threshold: i32,
) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::Validation("name must not be empty".into()));
    }
    if sku.trim().is_empty() {
        return Err(AppError::Validation("sku must not be empty".into()));
    }
    if price < Decimal::ZERO {
        return Err(AppError::Validation("price must not be negative".into()));
    }
    if stock_quantity < 0 {
        return Err(AppError::Validation(
            "stock_quantity must not be negative".into(),
        ));
    }
    if low_stock_threshold < 0 {
        return Err(AppError::Validation(
            "low_stock_threshold must not be negative".into(),
        ));
    }
    Ok(())
}

pub async fn create_product(pool: &PgPool, req: CreateProductRequest) -> AppResult<Product> {
    let low_stock_threshold = req.low_stock_threshold.unwrap_or(5);

    validate_product_fields(
        &req.name,
        &req.sku,
        req.price,
        req.stock_quantity,
        low_stock_threshold,
    )?;

    let mut tx = pool.begin().await?;

    let product = sqlx::query_as::<_, Product>(
        r#"
        INSERT INTO products (name, sku, price, stock_quantity, low_stock_threshold)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, sku, price, stock_quantity, low_stock_threshold, created_at, updated_at
        "#,
    )
    .bind(&req.name)
    .bind(&req.sku)
    .bind(req.price)
    .bind(req.stock_quantity)
    .bind(low_stock_threshold)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| map_product_db_error(e, &req.sku))?;

    // Phase 3: log initial balance — only if stock > 0.
    if product.stock_quantity > 0 {
        log_movement(
            &mut tx,
            product.id,
            product.stock_quantity,
            REASON_INITIAL,
            None,
            None,
        )
        .await?;
    }

    tx.commit().await?;
    Ok(product)
}

pub async fn list_products(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> AppResult<(Vec<Product>, i64)> {
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM products")
        .fetch_one(pool)
        .await?;

    let products = sqlx::query_as::<_, Product>(
        r#"
        SELECT id, name, sku, price, stock_quantity, low_stock_threshold, created_at, updated_at
        FROM products
        ORDER BY created_at DESC, id ASC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((products, total.0))
}

pub async fn get_product(pool: &PgPool, id: Uuid) -> AppResult<Product> {
    let product = sqlx::query_as::<_, Product>(
        r#"
        SELECT id, name, sku, price, stock_quantity, low_stock_threshold, created_at, updated_at
        FROM products
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("product {id} not found")))?;

    Ok(product)
}

pub async fn update_product(
    pool: &PgPool,
    id: Uuid,
    req: UpdateProductRequest,
) -> AppResult<Product> {
    if let Some(ref n) = req.name {
        if n.trim().is_empty() {
            return Err(AppError::Validation("name must not be empty".into()));
        }
    }
    if let Some(ref s) = req.sku {
        if s.trim().is_empty() {
            return Err(AppError::Validation("sku must not be empty".into()));
        }
    }
    if let Some(p) = req.price {
        if p < Decimal::ZERO {
            return Err(AppError::Validation("price must not be negative".into()));
        }
    }
    if let Some(q) = req.stock_quantity {
        if q < 0 {
            return Err(AppError::Validation(
                "stock_quantity must not be negative".into(),
            ));
        }
    }
    if let Some(t) = req.low_stock_threshold {
        if t < 0 {
            return Err(AppError::Validation(
                "low_stock_threshold must not be negative".into(),
            ));
        }
    }

    let mut tx = pool.begin().await?;

    let current = sqlx::query_as::<_, Product>(
        r#"
        SELECT id, name, sku, price, stock_quantity, low_stock_threshold, created_at, updated_at
        FROM products
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("product {id} not found")))?;

    let name = req.name.unwrap_or(current.name);
    let sku = req.sku.unwrap_or(current.sku);
    let price = req.price.unwrap_or(current.price);
    let stock_quantity = req.stock_quantity.unwrap_or(current.stock_quantity);
    let low_stock_threshold = req
        .low_stock_threshold
        .unwrap_or(current.low_stock_threshold);

    validate_product_fields(&name, &sku, price, stock_quantity, low_stock_threshold)?;

    let stock_delta = stock_quantity
        .checked_sub(current.stock_quantity)
        .ok_or_else(|| AppError::Validation("stock calculation overflow".into()))?;

    let product = sqlx::query_as::<_, Product>(
        r#"
        UPDATE products
        SET name = $1, sku = $2, price = $3, stock_quantity = $4,
            low_stock_threshold = $5, updated_at = now()
        WHERE id = $6
        RETURNING id, name, sku, price, stock_quantity, low_stock_threshold, created_at, updated_at
        "#,
    )
    .bind(&name)
    .bind(&sku)
    .bind(price)
    .bind(stock_quantity)
    .bind(low_stock_threshold)
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| map_product_db_error(e, &sku))?;

    // Phase 3: log delta only if stock actually changed.
    if stock_delta != 0 {
        log_movement(
            &mut tx,
            id,
            stock_delta,
            REASON_ADMIN_ADJUSTMENT,
            None,
            None,
        )
        .await?;
    }

    tx.commit().await?;
    Ok(product)
}

pub async fn delete_product(pool: &PgPool, id: Uuid) -> AppResult<()> {
    // C4: refuse deletion if referenced by any order_item.
    let refs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM order_items WHERE product_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if refs.0 > 0 {
        return Err(AppError::InvalidState(format!(
            "product {id} cannot be deleted: referenced by {} order item(s); consider soft-delete or archive",
            refs.0
        )));
    }

    // Phase 3: refuse deletion if any audit history exists.
    let movements: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM inventory_movements WHERE product_id = $1")
            .bind(id)
            .fetch_one(pool)
            .await?;
    if movements.0 > 0 {
        return Err(AppError::InvalidState(format!(
            "product {id} cannot be deleted: has {} inventory movement(s); audit trail must be preserved",
            movements.0
        )));
    }

    let result = sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("product {id} not found")));
    }

    Ok(())
}

pub async fn low_stock_products(pool: &PgPool) -> AppResult<Vec<Product>> {
    let products = sqlx::query_as::<_, Product>(
        r#"
        SELECT id, name, sku, price, stock_quantity, low_stock_threshold, created_at, updated_at
        FROM products
        WHERE stock_quantity <= low_stock_threshold
        ORDER BY stock_quantity ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(products)
}
