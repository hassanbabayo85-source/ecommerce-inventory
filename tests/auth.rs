mod common;

use serde_json::json;
use std::net::SocketAddr;

async fn spawn_app() -> String {
    let pool = common::setup_pool().await;
    let app = common::app(pool);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .await
            .unwrap();
    });

    format!("http://{addr}")
}

#[tokio::test]
#[serial_test::serial]
async fn admin_route_rejects_missing_api_key() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/products"))
        .json(&json!({
            "name": "No Auth",
            "sku": "NOAUTH-001",
            "price": "10.00",
            "stock_quantity": 5
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        401,
        "admin route must return 401 without X-API-Key"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn admin_route_rejects_invalid_api_key() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/products"))
        .header("x-api-key", "definitely-wrong-key-xxxxxxxx")
        .json(&json!({
            "name": "Bad Key",
            "sku": "BADKEY-001",
            "price": "10.00",
            "stock_quantity": 5
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        401,
        "admin route must reject wrong X-API-Key"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn admin_route_accepts_valid_api_key() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/products"))
        .header("x-api-key", common::TEST_API_KEY)
        .json(&json!({
            "name": "Good Key",
            "sku": "GOODKEY-001",
            "price": "10.00",
            "stock_quantity": 5
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        201,
        "admin route must accept valid X-API-Key"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn public_routes_do_not_require_api_key() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/api/products"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "GET /api/products must be public");

    let resp = client.get(format!("{base}/health")).send().await.unwrap();
    assert_eq!(resp.status(), 200, "GET /health must be public");
}

#[tokio::test]
#[serial_test::serial]
async fn pay_endpoint_requires_api_key() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    // Create product (admin).
    let resp = client
        .post(format!("{base}/api/products"))
        .header("x-api-key", common::TEST_API_KEY)
        .json(&json!({
            "name": "Pay Test",
            "sku": "PAY-001",
            "price": "10.00",
            "stock_quantity": 5
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let product: serde_json::Value = resp.json().await.unwrap();
    let product_id = product["id"].as_str().unwrap();

    // Create order (public).
    let resp = client
        .post(format!("{base}/api/orders"))
        .json(&json!({
            "customer_email": "buyer@example.com",
            "items": [{"product_id": product_id, "quantity": 1}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let order: serde_json::Value = resp.json().await.unwrap();
    let order_id = order["id"].as_str().unwrap();

    // Pay without key → 401.
    let resp = client
        .post(format!("{base}/api/orders/{order_id}/pay"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Pay with key → 200.
    let resp = client
        .post(format!("{base}/api/orders/{order_id}/pay"))
        .header("x-api-key", common::TEST_API_KEY)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}
