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

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub admin_api_key: String,
}

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
/// Fail-safe behaviour (C2):
///   - `APP_ENV=production` (or `CORS_STRICT=true`) + empty/malformed
///     CORS_ALLOWED_ORIGINS → **panics at startup**.
///   - Otherwise (dev) → falls back to permissive with a loud warning.
fn build_cors_layer(origins: &[String]) -> CorsLayer {
    let strict = std::env::var("APP_ENV")
        .map(|v| v.eq_ignore_ascii_case("production") || v.eq_ignore_ascii_case("prod"))
        .unwrap_or(false)
        || std::env::var("CORS_STRICT")
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "True"))
            .unwrap_or(false);

    let empty_or_star = origins.is_empty() || origins.iter().any(|o| o == "*");
    if empty_or_star {
        if strict {
            panic!(
                "CORS misconfigured: CORS_ALLOWED_ORIGINS is empty or contains '*', \
                 but APP_ENV=production / CORS_STRICT is set. Set explicit origins."
            );
        }
        tracing::warn!(
            "CORS is permissive — set CORS_ALLOWED_ORIGINS in production to a comma-separated list"
        );
        return CorsLayer::permissive();
    }

    let mut parsed: Vec<HeaderValue> = Vec::with_capacity(origins.len());
    let mut invalid: Vec<String> = Vec::new();
    for o in origins {
        match HeaderValue::from_str(o) {
            Ok(v) => parsed.push(v),
            Err(e) => {
                tracing::error!(origin = %o, error = %e, "invalid CORS origin");
                invalid.push(o.clone());
            }
        }
    }

    if !invalid.is_empty() {
        if strict {
            panic!(
                "CORS misconfigured: invalid CORS_ALLOWED_ORIGINS entries: {}",
                invalid.join(", ")
            );
        }
        tracing::warn!(
            "skipping {} invalid CORS origin(s) in non-strict mode: {}",
            invalid.len(),
            invalid.join(", ")
        );
    }

    if parsed.is_empty() {
        if strict {
            panic!("CORS misconfigured: no valid CORS_ALLOWED_ORIGINS remain.");
        }
        tracing::warn!("all CORS origins were invalid — falling back to permissive (dev only)");
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

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
