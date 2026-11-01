mod common;

use ecommerce_inventory::db;
use ecommerce_inventory::errors::AppError;
use ecommerce_inventory::models::product::{CreateProductRequest, UpdateProductRequest};
use rust_decimal::Decimal;

#[tokio::test]
#[serial_test::serial]
async fn create_product_rejects_duplicate_sku_with_specific_message() {
    let pool = common::setup_pool().await;
    common::seed_product(&pool, "Original", "DUP-001", "10.00", 5).await;

    let err = db::products::create_product(
        &pool,
        CreateProductRequest {
            name: "Dupe".into(),
            sku: "DUP-001".into(),
            price: Decimal::new(500, 2),
            stock_quantity: 1,
            low_stock_threshold: None,
        },
    )
    .await
    .expect_err("duplicate SKU must fail");

    match err {
        AppError::Validation(msg) => {
            assert!(msg.contains("already exists"), "msg was: {msg}");
            assert!(msg.contains("DUP-001"), "msg must name the SKU: {msg}");
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
#[serial_test::serial]
async fn create_product_rejects_negative_low_stock_threshold() {
    let pool = common::setup_pool().await;

    let err = db::products::create_product(
        &pool,
        CreateProductRequest {
            name: "Bad".into(),
            sku: "BAD-THRESH".into(),
            price: Decimal::new(1000, 2),
            stock_quantity: 1,
            low_stock_threshold: Some(-1),
        },
    )
    .await
    .expect_err("negative threshold must fail");

    match err {
        AppError::Validation(msg) => assert!(msg.contains("low_stock_threshold"), "msg: {msg}"),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
#[serial_test::serial]
async fn create_product_rejects_empty_name() {
    let pool = common::setup_pool().await;
    let err = db::products::create_product(
        &pool,
        CreateProductRequest {
            name: "   ".into(),
            sku: "WHITESPACE-001".into(),
            price: Decimal::new(1000, 2),
            stock_quantity: 1,
            low_stock_threshold: None,
        },
    )
    .await
    .expect_err("whitespace-only name must fail");

    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
#[serial_test::serial]
async fn update_product_rejects_negative_low_stock_threshold() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Thresh", "THRESH-001", "10.00", 5).await;

    let err = db::products::update_product(
        &pool,
        product_id,
        UpdateProductRequest {
            name: None,
            sku: None,
            price: None,
            stock_quantity: None,
            low_stock_threshold: Some(-5),
        },
    )
    .await
    .expect_err("negative threshold must fail on update");

    match err {
        AppError::Validation(msg) => assert!(msg.contains("low_stock_threshold"), "msg: {msg}"),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
#[serial_test::serial]
async fn update_product_rejects_empty_sku() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Sku Test", "SKU-ORIG", "10.00", 5).await;

    let err = db::products::update_product(
        &pool,
        product_id,
        UpdateProductRequest {
            name: None,
            sku: Some("   ".into()),
            price: None,
            stock_quantity: None,
            low_stock_threshold: None,
        },
    )
    .await
    .expect_err("blank sku must fail");

    assert!(matches!(err, AppError::Validation(_)));
}
