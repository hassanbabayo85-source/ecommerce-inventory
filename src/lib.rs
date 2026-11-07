pub mod auth;
pub mod db;
pub mod errors;
pub mod handlers;
pub mod jobs;
pub mod models;

use axum::{
    extract::{MatchedPath, Request},
    http::{HeaderValue, Method},
    middleware::Next,
    response::Response,
    Router,
};
use sqlx::PgPool;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub admin_api_key: String,
    pub prometheus_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::health::health,
        crate::handlers::health::ready,
        crate::handlers::health::metrics,
        crate::handlers::products::list_products,
        crate::handlers::products::get_product,
        crate::handlers::products::create_product,
        crate::handlers::products::update_product,
        crate::handlers::products::delete_product,
        crate::handlers::products::low_stock_report,
        crate::handlers::orders::create_order,
        crate::handlers::orders::get_order,
        crate::handlers::orders::pay_order,
        crate::handlers::inventory::list_product_movements,
        crate::handlers::inventory::list_all_movements,
        crate::handlers::inventory::adjust_stock,
    ),
    components(
        schemas(
            crate::models::product::Product,
            crate::models::product::CreateProductRequest,
            crate::models::product::UpdateProductRequest,
            crate::models::order::Order,
            crate::models::order::OrderItem,
            crate::models::order::OrderStatus,
            crate::models::order::CreateOrderRequest,
            crate::models::order::CreateOrderItem,
            crate::models::order::OrderWithItems,
            crate::models::order::CreateOrderResponse,
            crate::models::inventory::InventoryMovement,
            crate::models::inventory::AdjustStockRequest,
        )
    ),
    tags(
        (name = "ecommerce", description = "E-commerce Inventory API")
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "api_key",
                utoipa::openapi::security::SecurityScheme::ApiKey(
                    utoipa::openapi::security::ApiKey::Header(
                        utoipa::openapi::security::ApiKeyValue::new("X-API-Key"),
                    ),
                ),
            );
        }
    }
}

// Middleware don rikodin HTTP metrics (requests da duration)
async fn track_metrics(req: Request, next: Next) -> Response {
    let start = std::time::Instant::now();
    let path = if let Some(matched_path) = req.extensions().get::<MatchedPath>() {
        matched_path.as_str().to_owned()
    } else {
        req.uri().path().to_owned()
    };
    let method = req.method().clone();

    let response = next.run(req).await;

    let latency = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    let labels = [
        ("method", method.to_string()),
        ("path", path),
        ("status", status),
    ];

    metrics::counter!("http_requests_total", &labels).increment(1);
    metrics::histogram!("http_request_duration_seconds", &labels).record(latency);

    response
}

pub fn build_app(
    pool: PgPool,
    admin_api_key: String,
    cors_origins: Vec<String>,
    prometheus_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
) -> Router {
    let state = AppState {
        pool,
        admin_api_key,
        prometheus_handle,
    };

    let admin = handlers::admin_routes().route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        auth::require_admin,
    ));

    let governor_conf = Box::new(
        GovernorConfigBuilder::default()
            .per_second(50)
            .burst_size(100)
            .finish()
            .unwrap(),
    );

    Router::new()
        .merge(SwaggerUi::new("/api/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .merge(handlers::public_routes())
        .merge(admin)
        .layer(axum::middleware::from_fn(track_metrics))
        .layer(TraceLayer::new_for_http())
        .layer(GovernorLayer::new(governor_conf))
        .layer(build_cors_layer(&cors_origins))
        .with_state(state)
}

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
