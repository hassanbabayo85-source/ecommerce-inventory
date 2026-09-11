mod common;

use ecommerce_inventory::db;
use ecommerce_inventory::models::order::{CreateOrderRequest, OrderItemRequest};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// The core correctness property of the whole system: if N concurrent requests race to buy
/// units of a product with only K in stock (N > K), exactly K requests must succeed and the
/// rest must fail with InsufficientStock — stock must never go negative and must never be
/// double-sold. This exercises `SELECT ... FOR UPDATE` row locking inside a DB transaction.
#[tokio::test]
async fn concurrent_orders_never_oversell_stock() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Race Widget", "RACE-001", "9.99", 5).await;

    let concurrent_requests = 20;
    let success_count = Arc::new(AtomicUsize::new(0));
    let failure_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for i in 0..concurrent_requests {
        let pool = pool.clone();
        let success_count = success_count.clone();
        let failure_count = failure_count.clone();
        handles.push(tokio::spawn(async move {
            let req = CreateOrderRequest {
                customer_email: format!("buyer{i}@example.com"),
                items: vec![OrderItemRequest {
                    product_id,
                    quantity: 1,
                }],
            };
            match db::orders::create_order(&pool, req).await {
                Ok(_) => {
                    success_count.fetch_add(1, Ordering::SeqCst);
                }
                Err(ecommerce_inventory::errors::AppError::InsufficientStock { .. }) => {
                    failure_count.fetch_add(1, Ordering::SeqCst);
                }
                Err(e) => panic!("unexpected error: {e:?}"),
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Exactly 5 of the 20 concurrent requests should have won the race.
    assert_eq!(success_count.load(Ordering::SeqCst), 5);
    assert_eq!(failure_count.load(Ordering::SeqCst), 15);

    let product = db::products::get_product(&pool, product_id).await.unwrap();
    assert_eq!(product.stock_quantity, 0, "stock must land exactly at zero, never negative");

    // Sanity check: exactly 5 orders exist for this product and each reserved 1 unit.
    let orders_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM order_items WHERE product_id = $1",
    )
    .bind(product_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(orders_count.0, 5);
}

/// A failed order (insufficient stock) must not leave any partial state behind: no order row,
/// no order_items rows, and stock unchanged. Verifies the transaction rollback path.
#[tokio::test]
async fn insufficient_stock_rolls_back_cleanly() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Scarce Item", "SCARCE-001", "20.00", 2).await;

    let req = CreateOrderRequest {
        customer_email: "buyer@example.com".to_string(),
        items: vec![OrderItemRequest {
            product_id,
            quantity: 10, // more than the 2 in stock
        }],
    };

    let result = db::orders::create_order(&pool, req).await;
    assert!(matches!(
        result,
        Err(ecommerce_inventory::errors::AppError::InsufficientStock { requested: 10, available: 2, .. })
    ));

    let product = db::products::get_product(&pool, product_id).await.unwrap();
    assert_eq!(product.stock_quantity, 2, "stock must be untouched after a rolled-back order");

    let orders_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM orders").fetch_one(&pool).await.unwrap();
    assert_eq!(orders_count.0, 0, "no order row should have been persisted");
}

/// A multi-item order that partially fails (one product has enough stock, another doesn't)
/// must roll back *both* products' stock, not just the one that failed.
#[tokio::test]
async fn multi_item_order_rolls_back_all_products_on_partial_failure() {
    let pool = common::setup_pool().await;
    let plenty_id = common::seed_product(&pool, "Plenty", "PLENTY-001", "5.00", 100).await;
    let scarce_id = common::seed_product(&pool, "Scarce", "SCARCE-002", "5.00", 1).await;

    let req = CreateOrderRequest {
        customer_email: "buyer@example.com".to_string(),
        items: vec![
            OrderItemRequest { product_id: plenty_id, quantity: 5 },
            OrderItemRequest { product_id: scarce_id, quantity: 5 }, // fails: only 1 in stock
        ],
    };

    let result = db::orders::create_order(&pool, req).await;
    assert!(result.is_err());

    let plenty = db::products::get_product(&pool, plenty_id).await.unwrap();
    let scarce = db::products::get_product(&pool, scarce_id).await.unwrap();
    assert_eq!(plenty.stock_quantity, 100, "unaffected product's stock must roll back too");
    assert_eq!(scarce.stock_quantity, 1);
}
