mod common;

use ecommerce_inventory::db;
use ecommerce_inventory::models::order::{CreateOrderRequest, OrderItemRequest};
use ecommerce_inventory::models::product::UpdateProductRequest;
use uuid::Uuid;

/// Regression test for C1 (audit fix).
///
/// Before the fix, `update_product` did a bare read-then-write with no lock:
///   1. update reads stock=10 at T1
///   2. order runs, decrements stock to 7, commits
///   3. update writes stock=10 back — the sale is "forgotten"
///
/// After the fix, both operations take `SELECT ... FOR UPDATE`, so they serialize
/// on the product row and the final stock is always 7 regardless of interleaving.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn concurrent_order_and_update_never_resurrect_stock() {
    let pool = common::setup_pool().await;

    let product_id: Uuid =
        common::seed_product(&pool, "Update Race Widget", "UPD-RACE-001", "5.00", 10).await;

    let rounds: usize = 15;

    for round in 0..rounds {
        // Reset state for this round: stock=10, no orders.
        sqlx::query("DELETE FROM order_items")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM orders")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE products SET stock_quantity = 10 WHERE id = $1")
            .bind(product_id)
            .execute(&pool)
            .await
            .unwrap();

        // Fire an order and an update at the same time on separate tasks.
        let pool_order = pool.clone();
        let order_handle = tokio::spawn(async move {
            db::orders::create_order(
                &pool_order,
                CreateOrderRequest {
                    customer_email: format!("racer{round}@example.com"),
                    items: vec![OrderItemRequest {
                        product_id,
                        quantity: 3,
                    }],
                },
            )
            .await
        });

        let pool_update = pool.clone();
        let update_handle = tokio::spawn(async move {
            db::products::update_product(
                &pool_update,
                product_id,
                UpdateProductRequest {
                    name: Some(format!("Renamed r{round}")),
                    sku: None,
                    price: None,
                    stock_quantity: None,
                    low_stock_threshold: None,
                },
            )
            .await
        });

        let order_result = order_handle.await.unwrap();
        let update_result = update_handle.await.unwrap();

        assert!(
            update_result.is_ok(),
            "round {round}: update failed unexpectedly: {:?}",
            update_result.err()
        );
        assert!(
            order_result.is_ok(),
            "round {round}: order failed unexpectedly: {:?}",
            order_result.err()
        );

        let product = db::products::get_product(&pool, product_id).await.unwrap();
        assert_eq!(
            product.stock_quantity, 7,
            "round {round}: stock was clobbered! Expected 7 (10-3), got {}. \
             A concurrent update overwrote the order's decrement.",
            product.stock_quantity
        );
    }
}
