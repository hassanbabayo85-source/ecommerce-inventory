mod common;

use ecommerce_inventory::db;
use ecommerce_inventory::errors::AppError;
use ecommerce_inventory::models::order::{CreateOrderRequest, OrderItemRequest, OrderStatus};

#[tokio::test]
#[serial_test::serial]
async fn create_order_rejects_empty_email() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Email Test", "EMAIL-001", "5.00", 5).await;

    let err = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "   ".into(),
            items: vec![OrderItemRequest {
                product_id,
                quantity: 1,
            }],
        },
        None,
    )
    .await
    .expect_err("blank email must fail");

    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
#[serial_test::serial]
async fn create_order_rejects_empty_items() {
    let pool = common::setup_pool().await;

    let err = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "buyer@example.com".into(),
            items: vec![],
        },
        None,
    )
    .await
    .expect_err("empty items must fail");

    assert!(matches!(err, AppError::Validation(_)));
}

#[tokio::test]
#[serial_test::serial]
async fn create_order_rejects_zero_or_negative_quantity() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Qty Test", "QTY-001", "5.00", 5).await;

    for q in [0, -1] {
        let err = db::orders::create_order(
            &pool,
            CreateOrderRequest {
                customer_email: "buyer@example.com".into(),
                items: vec![OrderItemRequest {
                    product_id,
                    quantity: q,
                }],
            },
            None,
        )
        .await
        .expect_err("non-positive quantity must fail");

        assert!(matches!(err, AppError::Validation(_)), "q={q}");
    }
}

#[tokio::test]
#[serial_test::serial]
async fn payment_and_cancellation_race_does_not_double_restock() {
    let pool = common::setup_pool().await;
    let product_id = common::seed_product(&pool, "Race Paid", "RACE-PAID-001", "5.00", 5).await;

    let created = db::orders::create_order(
        &pool,
        CreateOrderRequest {
            customer_email: "racy@example.com".into(),
            items: vec![OrderItemRequest {
                product_id,
                quantity: 2,
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

    let pool_pay = pool.clone();
    let order_id = created.order.id;
    let pay = tokio::spawn(async move { db::orders::pay_order(&pool_pay, order_id).await });

    let pool_sweep = pool.clone();
    let sweep =
        tokio::spawn(async move { db::orders::cancel_stale_pending_orders(&pool_sweep, 15).await });

    let pay_result = pay.await.unwrap();
    let sweep_result = sweep.await.unwrap();

    assert!(
        pay_result.is_ok() || !sweep_result.unwrap_or_default().is_empty(),
        "either payment or cancellation must have happened"
    );

    let order =
        db::orders::get_order_with_items(&pool, created.order.id, Some(created.access_token))
            .await
            .unwrap();
    let product = db::products::get_product(&pool, product_id).await.unwrap();

    match order.order.status {
        OrderStatus::Paid => {
            assert_eq!(product.stock_quantity, 3, "paid → stock stays 3 (was 5-2)");
        }
        OrderStatus::Cancelled => {
            assert_eq!(product.stock_quantity, 5, "cancelled → stock restored to 5");
        }
        other => panic!("unexpected final status: {other}"),
    }
}
