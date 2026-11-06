mod common;

use ecommerce_inventory::db;
use ecommerce_inventory::models::order::{CreateOrderRequest, OrderItemRequest, OrderStatus};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn two_concurrent_sweeps_do_not_double_restock() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "MW Item", "MW-001", "5.00", 10).await;

    let created = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "mw@example.com".into(),
            items: vec![OrderItemRequest {
                product_id,
                quantity: 4,
            }],
        },
        None,
    )
    .await
    .unwrap();

    sqlx::query("UPDATE orders SET created_at = now() - interval '30 minutes' WHERE id = $1")
        .bind(created.order.id)
        .execute(&pool)
        .await
        .unwrap();

    let pool_a = pool.clone();
    let pool_b = pool.clone();
    let a = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        db::orders::cancel_stale_pending_orders(&pool_a, 15).await
    });
    let b = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        db::orders::cancel_stale_pending_orders(&pool_b, 15).await
    });

    let a_ids = a.await.unwrap().unwrap();
    let b_ids = b.await.unwrap().unwrap();

    let total_cancelled = a_ids.len() + b_ids.len();
    assert_eq!(
        total_cancelled,
        1,
        "exactly one sweeper must have cancelled (a={}, b={})",
        a_ids.len(),
        b_ids.len()
    );

    let order =
        db::orders::get_order_with_items(&pool, created.order.id, Some(created.access_token))
            .await
            .unwrap();
    assert_eq!(order.order.status, OrderStatus::Cancelled);

    let product = db::products::get_product(&pool, product_id).await.unwrap();
    assert_eq!(product.stock_quantity, 10);
}
