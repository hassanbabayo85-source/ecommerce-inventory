use crate::errors::{AppError, AppResult};
use crate::models::product::{CreateProductRequest, Product, UpdateProductRequest};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn create_product(pool: &PgPool, req: CreateProductRequest) -> AppResult<Product> {
    if req.name.trim().is_empty() {
        return Err(AppError::Validation("name must not be empty".into()));
    }
    if req.sku.trim().is_empty() {
        return Err(AppError::Validation("sku must not be empty".into()));
    }
    if req.price < Decimal::ZERO {
        return Err(AppError::Validation("price must not be negative".into()));
    }
    if req.stock_quantity < 0 {
        return Err(AppError::Validation(
            "stock_quantity must not be negative".into(),
        ));
    }

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
    .bind(req.low_stock_threshold.unwrap_or(5))
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.constraint().is_some() => {
            AppError::Validation(format!("sku '{}' already exists", req.sku))
        }
        _ => AppError::Database(e),
    })?;

    Ok(product)
}

pub async fn list_products(pool: &PgPool) -> AppResult<Vec<Product>> {
    let products = sqlx::query_as::<_, Product>(
        r#"
        SELECT id, name, sku, price, stock_quantity, low_stock_threshold, created_at, updated_at
        FROM products
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(products)
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
    // Load current row first so we can apply partial updates and validate.
    let current = get_product(pool, id).await?;

    let name = req.name.unwrap_or(current.name);
    let sku = req.sku.unwrap_or(current.sku);
    let price = req.price.unwrap_or(current.price);
    let stock_quantity = req.stock_quantity.unwrap_or(current.stock_quantity);
    let low_stock_threshold = req
        .low_stock_threshold
        .unwrap_or(current.low_stock_threshold);

    if price < Decimal::ZERO {
        return Err(AppError::Validation("price must not be negative".into()));
    }
    if stock_quantity < 0 {
        return Err(AppError::Validation(
            "stock_quantity must not be negative".into(),
        ));
    }

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
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("product {id} not found")))?;

    Ok(product)
}

pub async fn delete_product(pool: &PgPool, id: Uuid) -> AppResult<()> {
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
