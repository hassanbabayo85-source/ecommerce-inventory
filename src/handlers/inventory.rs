use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::db;
use crate::errors::AppResult;
use crate::models::inventory::{AdjustStockRequest, InventoryMovement, MovementsQuery};
use crate::AppState;

/// GET /api/products/:id/movements (admin)
pub async fn list_product_movements(
    State(state): State<AppState>,
    Path(product_id): Path<Uuid>,
    Query(params): Query<MovementsQuery>,
) -> AppResult<Json<Vec<InventoryMovement>>> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let rows =
        db::inventory::list_product_movements(&state.pool, product_id, limit, offset).await?;
    Ok(Json(rows))
}

/// GET /api/inventory/movements (admin)
pub async fn list_all_movements(
    State(state): State<AppState>,
    Query(params): Query<MovementsQuery>,
) -> AppResult<Json<Vec<InventoryMovement>>> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let rows = db::inventory::list_all_movements(&state.pool, limit, offset).await?;
    Ok(Json(rows))
}

/// POST /api/inventory/adjustments (admin)
pub async fn adjust_stock(
    State(state): State<AppState>,
    Json(req): Json<AdjustStockRequest>,
) -> AppResult<(StatusCode, Json<InventoryMovement>)> {
    let movement =
        db::inventory::adjust_stock(&state.pool, req.product_id, req.change, req.note).await?;
    Ok((StatusCode::CREATED, Json(movement)))
}
