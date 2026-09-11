use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::db;
use crate::errors::AppResult;
use crate::models::order::{CreateOrderRequest, Order, OrderWithItems};
use crate::AppState;

/// POST /api/orders
/// Atomic: validates stock, reserves it, and creates the order + order_items in one
/// database transaction with row-level locking (see db::orders::create_order).
pub async fn create_order(
    State(state): State<AppState>,
    Json(req): Json<CreateOrderRequest>,
) -> AppResult<(StatusCode, Json<OrderWithItems>)> {
    let order = db::orders::create_order(&state.pool, req).await?;
    Ok((StatusCode::CREATED, Json(order)))
}

/// GET /api/orders/:id
pub async fn get_order(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<OrderWithItems>> {
    let order = db::orders::get_order_with_items(&state.pool, id).await?;
    Ok(Json(order))
}

/// POST /api/orders/:id/pay
/// Simulated payment confirmation: pending -> paid.
pub async fn pay_order(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Order>> {
    let order = db::orders::pay_order(&state.pool, id).await?;
    Ok(Json(order))
}
