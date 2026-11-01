mod common;

use serde_json::json;
use std::net::SocketAddr;

async fn spawn_app() -> String {
    let pool = common::setup_pool().await;
    let app = common::app(pool);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service()).await.unwrap();
    });

    format!("http://{addr}")
}

#[tokio::test]
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

    // List products includes it (public).
    let resp = client.get(format!("{base}/api/products")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let products: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(products.as_array().unwrap().len(), 1);

    // Create an order for 4 units (public).
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
    let order: serde_json::Value = resp.json().await.unwrap();
    let order_id = order["id"].as_str().unwrap().to_string();
    assert_eq!(order["status"], "pending");
    assert_eq!(order["total_amount"], "100.00");

    // Low-stock report (public).
    let resp = client.get(format!("{base}/api/products/low-stock")).send().await.unwrap();
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

    // Fetch full order detail with items (public).
    let resp = client.get(format!("{base}/api/orders/{order_id}")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let full_order: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(full_order["items"].as_array().unwrap().len(), 1);
    assert_eq!(full_order["items"][0]["quantity"], 4);

    // Fetching a non-existent order returns 404 (public).
    let resp = client
        .get(format!("{base}/api/orders/00000000-0000-0000-0000-000000000000"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
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
