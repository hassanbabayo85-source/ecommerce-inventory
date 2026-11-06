//! Liveness and readiness probes.
//!
//! - `/health` is a **liveness** check: the process is up and the router
//!   responds. Used by Docker's HEALTHCHECK for restart-on-crash.
//! - `/ready`  is a **readiness** check: the app can serve traffic right now.
//!   Currently it pings the DB with `SELECT 1`. Used by orchestrators
//!   (Kubernetes, load balancers) to decide when to send traffic.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::AppState;

pub async fn health() -> &'static str {
    "ok"
}

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
