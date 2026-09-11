pub mod orders;
pub mod products;

use axum::{
    routing::{get, post},
    Router,
};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/products",
            post(products::create_product).get(products::list_products),
        )
        .route("/api/products/low-stock", get(products::low_stock_report))
        .route(
            "/api/products/:id",
            get(products::get_product)
                .put(products::update_product)
                .delete(products::delete_product),
        )
        .route("/api/orders", post(orders::create_order))
        .route("/api/orders/:id", get(orders::get_order))
        .route("/api/orders/:id/pay", post(orders::pay_order))
        .route("/health", get(health))
}

async fn health() -> &'static str {
    "ok"
}
