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

#[tokio::test]
#[serial_test::serial]
async fn openapi_spec_is_available() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/api-docs/openapi.json"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200, "OpenAPI JSON yakamata ya kasance");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["openapi"], "3.1.0");
    assert!(body["paths"]["/api/orders"].is_object());
    assert!(body["paths"]["/api/orders/{id}"].is_object());
}

#[tokio::test]
#[serial_test::serial]
async fn concurrent_order_and_product_delete_never_conflicts() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    // 1) Create a product
    let resp = client
        .post(format!("{base}/api/products"))
        .header("x-api-key", common::TEST_API_KEY)
        .json(&json!({
            "name": "Delete Race", "sku": "DEL-RACE-001", "price": "10.00", "stock_quantity": 50
        }))
        .send()
        .await
        .unwrap();
    let product: serde_json::Value = resp.json().await.unwrap();
    let product_id = product["id"].as_str().unwrap().to_string();

    // 2) Fire order creation and product deletion concurrently
    let c_order = client.clone();
    let p_id1 = product_id.clone();
    let base1 = base.clone();
    let order_task = tokio::spawn(async move {
        c_order
            .post(format!("{base1}/api/orders"))
            .json(&json!({
                "customer_email": "race@example.com",
                "items": [{ "product_id": p_id1, "quantity": 5 }]
            }))
            .send()
            .await
            .unwrap()
            .status()
    });

    let c_del = client.clone();
    let p_id2 = product_id.clone();
    let base2 = base.clone();
    let del_task = tokio::spawn(async move {
        c_del
            .delete(format!("{base2}/api/products/{p_id2}"))
            .header("x-api-key", common::TEST_API_KEY)
            .send()
            .await
            .unwrap()
            .status()
    });

    let order_status = order_task.await.unwrap();
    let del_status = del_task.await.unwrap();

    // Ko dai order ya yi nasara (201) sannan delete ya kasa (409),
    // Ko kuma delete ya yi nasara (204) sannan order ya kasa (404/409).
    let order_ok = order_status.is_success();
    let del_ok = del_status.is_success();

    assert!(
        order_ok || del_ok,
        "Ko daya daga cikinsu yakamata yayi nasara"
    );

    if order_ok {
        assert_eq!(
            del_status.as_u16(),
            409,
            "Idan order yayi nasara, delete yakamata ya dawo 409"
        );
    }
    if del_ok {
        assert!(
            order_status.as_u16() == 404 || order_status.as_u16() == 409,
            "Idan delete yayi nasara, order yakamata ya kasa da 404 ko 409"
        );
    }
}
