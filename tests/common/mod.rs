use ecommerce_inventory::{build_app, db, models::product::CreateProductRequest, run_migrations};
use rust_decimal::Decimal;
use sqlx::PgPool;

/// The API key used by all tests hitting admin endpoints.
pub const TEST_API_KEY: &str = "test-admin-key-1234567890";

/// Connects to the test database (set TEST_DATABASE_URL, falls back to DATABASE_URL) and
/// makes sure migrations are applied. Each test truncates the tables it touches at the start
/// so tests stay independent even though they share one database/pool.
pub async fn setup_pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("set TEST_DATABASE_URL or DATABASE_URL to run integration tests");

    let pool = db::create_pool(&url).await.expect("failed to connect to test database");
    run_migrations(&pool).await.expect("failed to run migrations");

    sqlx::query("TRUNCATE TABLE order_items, orders, products RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await
        .expect("failed to truncate tables");

    pool
}

pub fn app(pool: PgPool) -> axum::Router {
    build_app(pool, TEST_API_KEY.to_string(), vec![])
}

pub async fn seed_product(pool: &PgPool, name: &str, sku: &str, price: &str, stock: i32) -> uuid::Uuid {
    let product = db::products::create_product(
        pool,
        CreateProductRequest {
            name: name.to_string(),
            sku: sku.to_string(),
            price: price.parse::<Decimal>().unwrap(),
            stock_quantity: stock,
            low_stock_threshold: Some(2),
        },
    )
    .await
    .expect("failed to seed product");

    product.id
}
