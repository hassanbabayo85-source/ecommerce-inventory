//! Phase 1 security + correctness regression tests.
//!   C1 — Order ownership (access token)
//!   C3 — Integer overflow (checked_add)
//!   C4 — Product delete FK conflict
//!   C6 — Idempotency key

mod common;

use ecommerce_inventory::db;
use ecommerce_inventory::errors::AppError;
use ecommerce_inventory::models::order::{CreateOrderRequest, OrderItemRequest};
use uuid::Uuid;

fn req_for(product_id: Uuid, qty: i32) -> CreateOrderRequest {
    CreateOrderRequest {
        customer_email: "buyer@example.com".into(),
        items: vec![OrderItemRequest {
            product_id,
            quantity: qty,
        }],
    }
}

#[tokio::test]
#[serial_test::serial]
async fn order_without_token_returns_not_found() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "Sec1", "SEC1-001", "5.00", 5).await;
    let created = db::orders::create_order(&pool, req_for(pid, 1), None)
        .await
        .unwrap();

    let err = db::orders::get_order_with_items(&pool, created.order.id, None)
        .await
        .expect_err("missing token must fail");
    assert!(matches!(err, AppError::NotFound(_)), "got: {err:?}");
}

#[tokio::test]
#[serial_test::serial]
async fn order_with_wrong_token_returns_not_found() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "Sec2", "SEC2-001", "5.00", 5).await;
    let created = db::orders::create_order(&pool, req_for(pid, 1), None)
        .await
        .unwrap();

    let err = db::orders::get_order_with_items(&pool, created.order.id, Some(Uuid::new_v4()))
        .await
        .expect_err("wrong token must fail");
    assert!(matches!(err, AppError::NotFound(_)), "got: {err:?}");
}

#[tokio::test]
#[serial_test::serial]
async fn order_with_correct_token_succeeds() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "Sec3", "SEC3-001", "5.00", 5).await;
    let created = db::orders::create_order(&pool, req_for(pid, 2), None)
        .await
        .unwrap();

    let order =
        db::orders::get_order_with_items(&pool, created.order.id, Some(created.access_token))
            .await
            .expect("correct token must succeed");
    assert_eq!(order.order.id, created.order.id);
    assert_eq!(order.items.len(), 1);
    assert_eq!(order.items[0].quantity, 2);
}

#[tokio::test]
#[serial_test::serial]
async fn idempotency_key_returns_same_order() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "Idem", "IDEM-001", "5.00", 10).await;

    let key = "test-idem-key-001";
    let first = db::orders::create_order(&pool, req_for(pid, 2), Some(key.into()))
        .await
        .unwrap();
    let second = db::orders::create_order(&pool, req_for(pid, 2), Some(key.into()))
        .await
        .unwrap();

    assert_eq!(first.order.id, second.order.id, "same order id returned");

    let product = db::products::get_product(&pool, pid).await.unwrap();
    assert_eq!(product.stock_quantity, 8, "stock decremented exactly once");
}

#[tokio::test]
#[serial_test::serial]
async fn no_idempotency_key_creates_separate_orders() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "NoIdem", "NOIDEM-001", "5.00", 10).await;

    let first = db::orders::create_order(&pool, req_for(pid, 1), None)
        .await
        .unwrap();
    let second = db::orders::create_order(&pool, req_for(pid, 1), None)
        .await
        .unwrap();

    assert_ne!(first.order.id, second.order.id);
    let product = db::products::get_product(&pool, pid).await.unwrap();
    assert_eq!(product.stock_quantity, 8, "stock decremented twice");
}

#[tokio::test]
#[serial_test::serial]
async fn duplicate_product_ids_overflowing_i32_are_rejected() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "Ovf", "OVF-001", "1.00", 100).await;

    let err = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "buyer@example.com".into(),
            items: vec![
                OrderItemRequest {
                    product_id: pid,
                    quantity: i32::MAX,
                },
                OrderItemRequest {
                    product_id: pid,
                    quantity: 1,
                },
            ],
        },
        None,
    )
    .await
    .expect_err("overflow must be rejected");

    match err {
        AppError::Validation(msg) => {
            assert!(
                msg.contains("overflow"),
                "msg should mention overflow: {msg}"
            )
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
#[serial_test::serial]
async fn delete_product_with_referencing_order_returns_conflict() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "DelRef", "DELREF-001", "5.00", 5).await;

    db::orders::create_order(&pool, req_for(pid, 1), None)
        .await
        .unwrap();

    let err = db::products::delete_product(&pool, pid)
        .await
        .expect_err("cannot delete referenced product");
    assert!(matches!(err, AppError::InvalidState(_)), "got: {err:?}");
}

#[tokio::test]
#[serial_test::serial]
async fn delete_unreferenced_product_succeeds() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "DelOk", "DELOK-001", "5.00", 0).await;

    db::products::delete_product(&pool, pid)
        .await
        .expect("unreferenced delete must succeed");

    let err = db::products::get_product(&pool, pid)
        .await
        .expect_err("should be gone");
    assert!(matches!(err, AppError::NotFound(_)));
}
