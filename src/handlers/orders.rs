use crate::{
    db,
    errors::{AppError, AppResult},
    models::order::{CreateOrderRequest, CreateOrderResponse, Order, OrderWithItems},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use uuid::Uuid;

#[utoipa::path(post, path = "/api/orders", request_body = CreateOrderRequest, responses((status = 201, body = CreateOrderResponse)), tag = "ecommerce")]
pub async fn create_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateOrderRequest>,
) -> AppResult<(StatusCode, Json<CreateOrderResponse>)> {
    let idempotency_key = parse_idempotency_key(&headers)?;
    let resp = db::orders::create_order(&state.pool, req, idempotency_key).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

#[utoipa::path(get, path = "/api/orders/{id}", params(("id" = Uuid, Path, description = "Order ID")), responses((status = 200, body = OrderWithItems)), tag = "ecommerce")]
pub async fn get_order(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> AppResult<Json<OrderWithItems>> {
    let token = parse_order_token(&headers)?;
    let order = db::orders::get_order_with_items(&state.pool, id, token).await?;
    Ok(Json(order))
}

#[utoipa::path(post, path = "/api/orders/{id}/pay", params(("id" = Uuid, Path, description = "Order ID")), responses((status = 200, body = Order)), tag = "ecommerce", security(("api_key" = [])))]
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
