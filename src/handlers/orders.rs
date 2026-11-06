use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use uuid::Uuid;

use crate::db;
use crate::errors::{AppError, AppResult};
use crate::models::order::{CreateOrderRequest, CreateOrderResponse, Order, OrderWithItems};
use crate::AppState;

/// POST /api/orders
/// Optional header: `Idempotency-Key` (C6).
/// Response includes `access_token` — clients MUST store it (C1).
pub async fn create_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateOrderRequest>,
) -> AppResult<(StatusCode, Json<CreateOrderResponse>)> {
    let idempotency_key = parse_idempotency_key(&headers)?;
    let resp = db::orders::create_order(&state.pool, req, idempotency_key).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/orders/:id
/// Required header: `X-Order-Token` matching the token from creation (C1).
pub async fn get_order(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> AppResult<Json<OrderWithItems>> {
    let token = parse_order_token(&headers)?;
    let order = db::orders::get_order_with_items(&state.pool, id, token).await?;
    Ok(Json(order))
}

/// POST /api/orders/:id/pay
pub async fn pay_order(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Order>> {
    let order = db::orders::pay_order(&state.pool, id).await?;
    Ok(Json(order))
}

fn parse_idempotency_key(headers: &HeaderMap) -> AppResult<Option<String>> {
    let Some(value) = headers.get("idempotency-key") else {
        return Ok(None);
    };
    let raw = value
        .to_str()
        .map_err(|_| AppError::Validation("Idempotency-Key must be valid ASCII".into()))?;
    let key = raw.trim().to_string();
    if key.is_empty() {
        return Err(AppError::Validation(
            "Idempotency-Key must not be empty".into(),
        ));
    }
    if key.len() > 255 {
        return Err(AppError::Validation(
            "Idempotency-Key must be ≤ 255 characters".into(),
        ));
    }
    Ok(Some(key))
}

fn parse_order_token(headers: &HeaderMap) -> AppResult<Option<Uuid>> {
    let Some(value) = headers.get("x-order-token") else {
        return Ok(None);
    };
    let raw = value
        .to_str()
        .map_err(|_| AppError::Unauthorized("X-Order-Token must be valid ASCII".into()))?;
    Uuid::parse_str(raw.trim())
        .map(Some)
        .map_err(|_| AppError::Unauthorized("X-Order-Token must be a valid UUID".into()))
}
