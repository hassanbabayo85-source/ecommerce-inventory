mod common;

use ecommerce_inventory::db;
use ecommerce_inventory::models::order::{CreateOrderRequest, OrderItemRequest};
use std::sync::Arc;

#[tokio::test]
#[serial_test::serial]
async fn concurrent_requests_with_same_key_create_one_order() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Idem Race", "IDEM-RACE", "10.00", 100).await;

    let key = "concurrent-idem-key".to_string();
    let concurrent_requests = 20;
    let mut handles = Vec::new();
    let order_ids = Arc::new(tokio::sync::Mutex::new(Vec::new()));

    for _ in 0..concurrent_requests {
        let pool = pool.clone();
        let key = key.clone();
        let order_ids = order_ids.clone();

        handles.push(tokio::spawn(async move {
            let req = CreateOrderRequest {
                customer_email: "concurrent@example.com".into(),
                items: vec![OrderItemRequest {
                    product_id,
                    quantity: 2,
                }],
            };

            // Duk suna amfani da key guda daya.
            let res = db::orders::create_order(&pool, req, Some(key)).await;
            if let Ok(order) = res {
                order_ids.lock().await.push(order.order.id);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let ids = order_ids.lock().await;

    // Dole ne duk requests 20 su samu order guda daya.
    assert_eq!(ids.len(), 20, "Duk requests 20 yakamata su sami order");

    // Tabbatar duk ID din order guda daya ne.
    let first_id = ids[0];
    for id in ids.iter() {
        assert_eq!(*id, first_id, "Duk requests suna samun order iri daya");
    }

    // Tabbatar an kirkira order guda daya a DB.
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM orders")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1, "Order guda daya kawai yakamata a kirkira a DB");

    // Tabbatar stock ya ragu sau daya kawai (100 - 2 = 98).
    let product = db::products::get_product(&pool, product_id).await.unwrap();
    assert_eq!(
        product.stock_quantity, 98,
        "Stock yakamata ya ragu sau daya kawai"
    );
}
