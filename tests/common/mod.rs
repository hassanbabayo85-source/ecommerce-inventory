//! Shared helpers for integration tests.
#![allow(dead_code)]

use ecommerce_inventory::{build_app, db, models::product::CreateProductRequest, run_migrations};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::OnceLock;

pub const TEST_API_KEY: &str = "test-admin-key-1234567890";

static PROMETHEUS_HANDLE: OnceLock<metrics_exporter_prometheus::PrometheusHandle> = OnceLock::new();

pub async fn setup_pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("set TEST_DATABASE_URL or DATABASE_URL to run integration tests");

    let pool = db::create_pool(&url)
        .await
        .expect("failed to connect to test database");
    run_migrations(&pool)
        .await
        .expect("failed to run migrations");

    sqlx::query(
        "TRUNCATE TABLE inventory_movements, idempotency_keys, order_items, orders, products RESTART IDENTITY CASCADE",
    )
    .execute(&pool)
    .await
    .expect("failed to truncate tables");

    pool
}

pub fn app(pool: PgPool) -> axum::Router {
    // Amfani da OnceLock don kada mu yi ƙoƙarin girka Prometheus sau biyu (wanda zai haifar da kuskure).
    let handle = PROMETHEUS_HANDLE
        .get_or_init(|| {
            metrics_exporter_prometheus::PrometheusBuilder::new()
                .install_recorder()
                .unwrap()
        })
        .clone();

    build_app(pool, TEST_API_KEY.to_string(), vec![], Some(handle))
}

pub async fn seed_product(
    pool: &PgPool,
    name: &str,
    sku: &str,
    price: &str,
    stock: i32,
) -> uuid::Uuid {
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
