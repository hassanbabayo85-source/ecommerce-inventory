mod common;

use ecommerce_inventory::db;
use ecommerce_inventory::models::order::{CreateOrderRequest, OrderItemRequest, OrderStatus};

#[tokio::test]
#[serial_test::serial]
async fn stale_pending_order_is_cancelled_and_stock_restored() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Slow Payer Item", "SLOW-001", "12.00", 10).await;

    let created = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "slow@example.com".to_string(),
            items: vec![OrderItemRequest {
                product_id,
                quantity: 3,
            }],
        },
        None,
    )
    .await
    .unwrap();

    let product = db::products::get_product(&pool, product_id).await.unwrap();
    assert_eq!(product.stock_quantity, 7);

    sqlx::query("UPDATE orders SET created_at = now() - interval '30 minutes' WHERE id = $1")
        .bind(created.order.id)
        .execute(&pool)
        .await
        .unwrap();

    let cancelled_ids = db::orders::cancel_stale_pending_orders(&pool, 15)
        .await
        .unwrap();
    assert_eq!(cancelled_ids, vec![created.order.id]);

    let order =
        db::orders::get_order_with_items(&pool, created.order.id, Some(created.access_token))
            .await
            .unwrap();
    assert_eq!(order.order.status, OrderStatus::Cancelled);

    let product = db::products::get_product(&pool, product_id).await.unwrap();
    assert_eq!(product.stock_quantity, 10);
}

#[tokio::test]
#[serial_test::serial]
async fn recent_pending_order_is_not_cancelled() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Fresh Order Item", "FRESH-001", "8.00", 5).await;

    let created = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "fresh@example.com".to_string(),
            items: vec![OrderItemRequest {
                product_id,
                quantity: 1,
            }],
        },
        None,
    )
    .await
    .unwrap();

    let cancelled_ids = db::orders::cancel_stale_pending_orders(&pool, 15)
        .await
        .unwrap();
    assert!(cancelled_ids.is_empty());

    let order =
        db::orders::get_order_with_items(&pool, created.order.id, Some(created.access_token))
            .await
            .unwrap();
    assert_eq!(order.order.status, OrderStatus::Pending);
}

#[tokio::test]
#[serial_test::serial]
async fn paid_order_is_never_cancelled_even_if_old() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Paid Old Item", "PAID-001", "30.00", 5).await;

    let created = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "paid@example.com".to_string(),
            items: vec![OrderItemRequest {
                product_id,
                quantity: 2,
            }],
        },
        None,
    )
    .await
    .unwrap();

    db::orders::pay_order(&pool, created.order.id)
        .await
        .unwrap();

    sqlx::query("UPDATE orders SET created_at = now() - interval '1 hour' WHERE id = $1")
        .bind(created.order.id)
        .execute(&pool)
        .await
        .unwrap();

    let cancelled_ids = db::orders::cancel_stale_pending_orders(&pool, 15)
        .await
        .unwrap();
    assert!(cancelled_ids.is_empty());

    let order =
        db::orders::get_order_with_items(&pool, created.order.id, Some(created.access_token))
            .await
            .unwrap();
    assert_eq!(order.order.status, OrderStatus::Paid);

    let product = db::products::get_product(&pool, product_id).await.unwrap();
    assert_eq!(product.stock_quantity, 3);
}
