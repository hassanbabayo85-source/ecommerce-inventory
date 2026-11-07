use crate::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

#[utoipa::path(get, path = "/health", responses((status = 200, description = "Liveness probe")))]
pub async fn health() -> &'static str {
    "ok"
}

#[utoipa::path(get, path = "/ready", responses((status = 200, description = "Readiness probe"), (status = 503, description = "DB unreachable")))]
pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ready"}))),
        Err(e) => {
            tracing::error!(error = ?e, "readiness check failed: database unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "not_ready", "reason": "database"})),
            )
        }
    }
}

#[utoipa::path(get, path = "/metrics", responses((status = 200, description = "Prometheus metrics"), (status = 503, description = "Metrics not initialized")))]
pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    match state.prometheus_handle {
        Some(ref handle) => (StatusCode::OK, handle.render()).into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Prometheus metrics not initialized",
        )
            .into_response(),
    }
}
