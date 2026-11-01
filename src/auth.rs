use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

use crate::{errors::AppError, AppState};

/// Middleware that requires a valid `X-API-Key` header on admin routes.
///
/// Uses constant-time comparison (`subtle::ConstantTimeEq`) to prevent attackers from
/// learning the key one byte at a time via response-timing side channels.
///
/// The expected key lives in `AppState.admin_api_key`, loaded once from the
/// `ADMIN_API_KEY` env var at startup.
pub async fn require_admin(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let provided = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("missing X-API-Key header".into()))?;

    if !bool::from(provided.as_bytes().ct_eq(state.admin_api_key.as_bytes())) {
        return Err(AppError::Unauthorized("invalid API key".into()));
    }

    Ok(next.run(req).await)
}
