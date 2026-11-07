mod common;

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
async fn metrics_endpoint_returns_prometheus_text() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    // Tura wata request kafin mu duba metrics don tabbatar da akwai bayanai
    client.get(format!("{base}/health")).send().await.unwrap();

    let resp = client.get(format!("{base}/metrics")).send().await.unwrap();

    assert_eq!(resp.status(), 200, "Metrics endpoint yakamata ya dawo 200");
    let body = resp.text().await.unwrap();

    assert!(
        body.contains("http_requests_total"),
        "Metrics yakamata su hada da http_requests_total"
    );
    assert!(
        body.contains("http_request_duration_seconds"),
        "Metrics yakamata su hada da http_request_duration_seconds"
    );
}
