mod common;

use ecommerce_inventory::db;
use ecommerce_inventory::models::order::{CreateOrderRequest, OrderItemRequest};
use ecommerce_inventory::models::product::UpdateProductRequest;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn concurrent_order_and_update_never_resurrect_stock() {
    let pool = common::setup_pool().await;

    let product_id: Uuid =
        common::seed_product(&pool, "Update Race Widget", "UPD-RACE-001", "5.00", 10).await;

    let rounds: usize = 15;

    for round in 0..rounds {
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
                None,
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

        assert!(update_result.is_ok(), "round {round}: update failed");
        assert!(order_result.is_ok(), "round {round}: order failed");

        let product = db::products::get_product(&pool, product_id).await.unwrap();
        assert_eq!(
            product.stock_quantity, 7,
            "round {round}: expected 7 (10-3), got {}",
            product.stock_quantity
        );
    }
}
