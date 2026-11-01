pub mod auth;
pub mod db;
pub mod errors;
pub mod handlers;
pub mod jobs;
pub mod models;

use axum::{
    http::{HeaderValue, Method},
    Router,
};
use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Shared application state, injected into every handler via `axum::extract::State`.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub admin_api_key: String,
}

/// Builds the full Axum `Router` (routes + shared state + middleware).
///
/// Admin routes (create/update/delete product, pay order) are guarded by
/// `auth::require_admin`, which checks `X-API-Key` against `admin_api_key`.
pub fn build_app(pool: PgPool, admin_api_key: String, cors_origins: Vec<String>) -> Router {
    let state = AppState {
        pool,
        admin_api_key,
    };

    let admin = handlers::admin_routes().route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        auth::require_admin,
    ));

    Router::new()
        .merge(handlers::public_routes())
        .merge(admin)
        .layer(TraceLayer::new_for_http())
        .layer(build_cors_layer(&cors_origins))
        .with_state(state)
}

/// Builds a CORS layer from a list of allowed origins.
///
/// - Empty list or containing `*` → permissive (dev default, logs a warning).
/// - Otherwise → strict allow-list. In production set
///   `CORS_ALLOWED_ORIGINS=https://app.example.com,https://admin.example.com`.
fn build_cors_layer(origins: &[String]) -> CorsLayer {
    if origins.is_empty() || origins.iter().any(|o| o == "*") {
        tracing::warn!(
            "CORS is permissive — set CORS_ALLOWED_ORIGINS in production to a comma-separated list"
        );
        return CorsLayer::permissive();
    }

    let mut parsed: Vec<HeaderValue> = Vec::with_capacity(origins.len());
    for o in origins {
        match HeaderValue::from_str(o) {
            Ok(v) => parsed.push(v),
            Err(e) => tracing::error!(origin = %o, error = %e, "skipping invalid CORS origin"),
        }
    }
    if parsed.is_empty() {
        tracing::warn!("all CORS origins were invalid — falling back to permissive");
        return CorsLayer::permissive();
    }

    CorsLayer::new()
        .allow_origin(parsed)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .allow_credentials(true)
}

/// Runs pending SQL migrations embedded in the binary at compile time.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
