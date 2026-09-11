pub mod db;
pub mod errors;
pub mod handlers;
pub mod jobs;
pub mod models;

use axum::Router;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// Shared application state, injected into every handler via `axum::extract::State`.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}

/// Builds the full Axum `Router` (routes + shared state + middleware). Used by `main.rs` to
/// serve the app, and by integration tests to exercise the app in-process without a real
/// TCP listener.
pub fn build_app(pool: PgPool) -> Router {
    let state = AppState { pool };

    handlers::router()
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Runs pending SQL migrations embedded in the binary at compile time.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
