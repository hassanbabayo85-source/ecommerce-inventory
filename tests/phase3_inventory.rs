mod common;

use ecommerce_inventory::db;
use ecommerce_inventory::errors::AppError;
use ecommerce_inventory::models::inventory::{
    InventoryMovement, REASON_ADMIN_ADJUSTMENT, REASON_INITIAL, REASON_ORDER_CANCELLATION,
    REASON_ORDER_RESERVATION,
};
use ecommerce_inventory::models::order::{CreateOrderRequest, OrderItemRequest};
use ecommerce_inventory::models::product::UpdateProductRequest;

async fn movements_for(pool: &sqlx::PgPool, product_id: uuid::Uuid) -> Vec<InventoryMovement> {
    db::inventory::list_product_movements(pool, product_id, 1000, 0)
        .await
        .unwrap()
}

#[tokio::test]
#[serial_test::serial]
async fn create_product_with_stock_logs_initial_movement() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-1", "5.00", 10).await;
    let ms = movements_for(&pool, pid).await;
    assert_eq!(ms.len(), 1);
    assert_eq!(ms[0].change, 10);
    assert_eq!(ms[0].reason, REASON_INITIAL);
    assert!(ms[0].reference_id.is_none());
}

#[tokio::test]
#[serial_test::serial]
async fn create_product_with_zero_stock_logs_no_movement() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-2", "5.00", 0).await;
    let ms = movements_for(&pool, pid).await;
    assert_eq!(ms.len(), 0);
}

#[tokio::test]
#[serial_test::serial]
async fn create_order_logs_reservation_movement() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-3", "5.00", 10).await;
    let created = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "b@e.com".into(),
            items: vec![OrderItemRequest {
                product_id: pid,
                quantity: 3,
            }],
        },
        None,
    )
    .await
    .unwrap();

    let ms = movements_for(&pool, pid).await;
    assert_eq!(ms.len(), 2); // initial + reservation
    let reservation = ms
        .iter()
        .find(|m| m.reason == REASON_ORDER_RESERVATION)
        .unwrap();
    assert_eq!(reservation.change, -3);
    assert_eq!(reservation.reference_id, Some(created.order.id));
}

#[tokio::test]
#[serial_test::serial]
async fn cancel_order_logs_cancellation_movement() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-4", "5.00", 10).await;
    let created = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "b@e.com".into(),
            items: vec![OrderItemRequest {
                product_id: pid,
                quantity: 3,
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

    db::orders::cancel_stale_pending_orders(&pool, 15)
        .await
        .unwrap();

    let ms = movements_for(&pool, pid).await;
    let cancellation = ms
        .iter()
        .find(|m| m.reason == REASON_ORDER_CANCELLATION)
        .unwrap();
    assert_eq!(cancellation.change, 3);
    assert_eq!(cancellation.reference_id, Some(created.order.id));
}

#[tokio::test]
#[serial_test::serial]
async fn update_product_stock_logs_admin_adjustment() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-5", "5.00", 10).await;

    db::products::update_product(
        &pool,
        pid,
        UpdateProductRequest {
            name: None,
            sku: None,
            price: None,
            stock_quantity: Some(25),
            low_stock_threshold: None,
        },
    )
    .await
    .unwrap();

    let ms = movements_for(&pool, pid).await;
    let adj = ms
        .iter()
        .find(|m| m.reason == REASON_ADMIN_ADJUSTMENT)
        .unwrap();
    assert_eq!(adj.change, 15);
    assert!(adj.reference_id.is_none());
}

#[tokio::test]
#[serial_test::serial]
async fn update_product_metadata_only_logs_no_movement() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-6", "5.00", 10).await;

    db::products::update_product(
        &pool,
        pid,
        UpdateProductRequest {
            name: Some("Renamed".into()),
            sku: None,
            price: None,
            stock_quantity: None,
            low_stock_threshold: None,
        },
    )
    .await
    .unwrap();

    let ms = movements_for(&pool, pid).await;
    assert_eq!(ms.len(), 1, "only initial movement expected");
    assert_eq!(ms[0].reason, REASON_INITIAL);
}

#[tokio::test]
#[serial_test::serial]
async fn manual_adjustment_endpoint_logs_movement() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-7", "5.00", 10).await;

    let m = db::inventory::adjust_stock(&pool, pid, -3, Some("shrinkage".into()))
        .await
        .unwrap();
    assert_eq!(m.change, -3);
    assert_eq!(m.reason, REASON_ADMIN_ADJUSTMENT);
    assert_eq!(m.note.as_deref(), Some("shrinkage"));

    let product = db::products::get_product(&pool, pid).await.unwrap();
    assert_eq!(product.stock_quantity, 7);
}

#[tokio::test]
#[serial_test::serial]
async fn manual_adjustment_cannot_make_stock_negative() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-8", "5.00", 5).await;

    let err = db::inventory::adjust_stock(&pool, pid, -10, None)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::InvalidState(_)), "got: {err:?}");
}

#[tokio::test]
#[serial_test::serial]
async fn ledger_reconciles_with_current_stock() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-9", "5.00", 20).await;

    // Order 1: 5 units
    db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "a@b.c".into(),
            items: vec![OrderItemRequest {
                product_id: pid,
                quantity: 5,
            }],
        },
        None,
    )
    .await
    .unwrap();

    // Order 2: 3 units, then cancelled
    let o2 = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "a@b.c".into(),
            items: vec![OrderItemRequest {
                product_id: pid,
                quantity: 3,
            }],
        },
        None,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE orders SET created_at = now() - interval '30 minutes' WHERE id = $1")
        .bind(o2.order.id)
        .execute(&pool)
        .await
        .unwrap();
    db::orders::cancel_stale_pending_orders(&pool, 15)
        .await
        .unwrap();

    // Manual adjustment: +10
    db::inventory::adjust_stock(&pool, pid, 10, Some("restock".into()))
        .await
        .unwrap();

    // 20 (initial) - 5 (order1) - 3 (order2) + 3 (cancel) + 10 (adjust) = 25
    let product = db::products::get_product(&pool, pid).await.unwrap();
    assert_eq!(product.stock_quantity, 25);

    let ms = movements_for(&pool, pid).await;
    let sum: i32 = ms.iter().map(|m| m.change).sum();
    assert_eq!(
        sum, product.stock_quantity,
        "ledger sum must equal current stock"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn delete_product_with_movements_is_refused() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-10", "5.00", 5).await;

    // stock=5 → has initial movement → cannot delete
    let err = db::products::delete_product(&pool, pid).await.unwrap_err();
    assert!(matches!(err, AppError::InvalidState(_)), "got: {err:?}");
}

#[tokio::test]
#[serial_test::serial]
async fn concurrent_orders_log_movements_atomically() {
    let pool = common::setup_pool().await;
    let pid = common::seed_product(&pool, "P", "SKU-11", "5.00", 5).await;

    let mut handles = Vec::new();
    for i in 0..20 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            db::orders::create_order(
                &pool,
                CreateOrderRequest {
                    customer_email: format!("b{i}@e.com"),
                    items: vec![OrderItemRequest {
                        product_id: pid,
                        quantity: 1,
                    }],
                },
                None,
            )
            .await
        }));
    }
    for h in handles {
        let _ = h.await.unwrap();
    }

    let ms = movements_for(&pool, pid).await;
    let reservations: Vec<_> = ms
        .iter()
        .filter(|m| m.reason == REASON_ORDER_RESERVATION)
        .collect();
    assert_eq!(reservations.len(), 5, "exactly 5 successful reservations");
    let total_change: i32 = reservations.iter().map(|m| m.change).sum();
    assert_eq!(total_change, -5);
}
