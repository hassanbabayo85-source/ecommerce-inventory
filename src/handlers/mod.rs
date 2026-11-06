pub mod health;
pub mod inventory;
pub mod orders;
pub mod products;

use axum::{
    routing::{get, post, put},
    Router,
};

use crate::AppState;

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready))
        .route("/api/products", get(products::list_products))
        .route("/api/products/low-stock", get(products::low_stock_report))
        .route("/api/products/:id", get(products::get_product))
        .route("/api/orders", post(orders::create_order))
        .route("/api/orders/:id", get(orders::get_order))
}

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/products", post(products::create_product))
        .route(
            "/api/products/:id",
            put(products::update_product).delete(products::delete_product),
        )
        .route("/api/orders/:id/pay", post(orders::pay_order))
        .route(
            "/api/products/:id/movements",
            get(inventory::list_product_movements),
        )
        .route(
            "/api/inventory/movements",
            get(inventory::list_all_movements),
        )
        .route("/api/inventory/adjustments", post(inventory::adjust_stock))
}
