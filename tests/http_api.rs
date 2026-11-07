mod common;

use serde_json::json;
use std::net::SocketAddr;

async fn spawn_app() -> String {
    let pool = common::setup_pool().await;
    let app = common::app(pool);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    format!("http://{addr}")
}

// ============================================================
// Phase 4 — health & readiness
// ============================================================

#[tokio::test]
#[serial_test::serial]
async fn health_endpoint_is_plain_ok() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();
    let resp = client.get(format!("{base}/health")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "ok");
}

#[tokio::test]
#[serial_test::serial]
async fn ready_endpoint_returns_200_when_db_is_up() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();
    let resp = client.get(format!("{base}/ready")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ready");
}

// ============================================================
// End-to-end product + order flow
// ============================================================

#[tokio::test]
#[serial_test::serial]
async fn full_product_and_order_http_flow() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    // Create a product (admin — needs X-API-Key).
    let resp = client
        .post(format!("{base}/api/products"))
        .header("x-api-key", common::TEST_API_KEY)
        .json(&json!({
            "name": "HTTP Test Widget",
            "sku": "HTTP-001",
            "price": "25.00",
            "stock_quantity": 5,
            "low_stock_threshold": 2
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let product: serde_json::Value = resp.json().await.unwrap();
    let product_id = product["id"].as_str().unwrap().to_string();

    let resp = client
        .get(format!("{base}/api/products"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let products: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(products.as_array().unwrap().len(), 1);

    // Create an order — response includes access_token (Phase 1 / C1).
    let resp = client
        .post(format!("{base}/api/orders"))
        .json(&json!({
            "customer_email": "http-buyer@example.com",
            "items": [{ "product_id": product_id, "quantity": 4 }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let created: serde_json::Value = resp.json().await.unwrap();
    let order_id = created["id"].as_str().unwrap().to_string();
    let access_token = created["access_token"]
        .as_str()
        .expect("access_token must be present in create response")
        .to_string();
    assert_eq!(created["status"], "pending");
    assert_eq!(created["total_amount"], "100.00");

    let resp = client
        .get(format!("{base}/api/products/low-stock"))
        .send()
        .await
        .unwrap();
    let low_stock: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(low_stock.as_array().unwrap().len(), 1);
    assert_eq!(low_stock[0]["stock_quantity"], 1);

    // Ordering more than the remaining 1 unit fails with 409 (public).
    let resp = client
        .post(format!("{base}/api/orders"))
        .json(&json!({
            "customer_email": "greedy@example.com",
            "items": [{ "product_id": product_id, "quantity": 2 }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);

    // Pay the first order (admin).
    let resp = client
        .post(format!("{base}/api/orders/{order_id}/pay"))
        .header("x-api-key", common::TEST_API_KEY)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let paid: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(paid["status"], "paid");

    // GET order without token → 404 (C1: no information leak).
    let resp = client
        .get(format!("{base}/api/orders/{order_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "missing X-Order-Token must be rejected");

    // GET order with wrong token → 404.
    let resp = client
        .get(format!("{base}/api/orders/{order_id}"))
        .header("x-order-token", uuid::Uuid::new_v4().to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "wrong X-Order-Token must be rejected");

    // GET order with correct token → 200 + items.
    let resp = client
        .get(format!("{base}/api/orders/{order_id}"))
        .header("x-order-token", &access_token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let full_order: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(full_order["items"].as_array().unwrap().len(), 1);
    assert_eq!(full_order["items"][0]["quantity"], 4);

    // Non-existent order → 404.
    let resp = client
        .get(format!(
            "{base}/api/orders/00000000-0000-0000-0000-000000000000"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
#[serial_test::serial]
async fn creating_product_with_negative_price_is_rejected() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/products"))
        .header("x-api-key", common::TEST_API_KEY)
        .json(&json!({
            "name": "Bad Product",
            "sku": "BAD-001",
            "price": "-5.00",
            "stock_quantity": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
#[serial_test::serial]
async fn idempotency_key_header_returns_same_order() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    // Create a product.
    let resp = client
        .post(format!("{base}/api/products"))
        .header("x-api-key", common::TEST_API_KEY)
        .json(&json!({
            "name": "Idem HTTP",
            "sku": "IDEM-HTTP-001",
            "price": "10.00",
            "stock_quantity": 10
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let product: serde_json::Value = resp.json().await.unwrap();
    let product_id = product["id"].as_str().unwrap().to_string();

    let body = json!({
        "customer_email": "idem@example.com",
        "items": [{ "product_id": product_id, "quantity": 2 }]
    });

    // First POST with key.
    let r1 = client
        .post(format!("{base}/api/orders"))
        .header("idempotency-key", "http-idem-abc-123")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), 201);
    let first: serde_json::Value = r1.json().await.unwrap();
    let first_id = first["id"].as_str().unwrap().to_string();

    // Second POST with same key → same order id.
    let r2 = client
        .post(format!("{base}/api/orders"))
        .header("idempotency-key", "http-idem-abc-123")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), 201);
    let second: serde_json::Value = r2.json().await.unwrap();
    assert_eq!(
        second["id"].as_str().unwrap(),
        first_id,
        "same idempotency key must return same order"
    );

    // Stock only decremented once.
    let resp = client
        .get(format!("{base}/api/products/{product_id}"))
        .send()
        .await
        .unwrap();
    let prod: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(prod["stock_quantity"], 8);
}
