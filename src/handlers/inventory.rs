use crate::{
    db,
    errors::AppResult,
    models::inventory::{AdjustStockRequest, InventoryMovement, MovementsQuery},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

#[utoipa::path(get, path = "/api/products/{id}/movements", params(("id" = Uuid, Path, description = "Product ID"), MovementsQuery), responses((status = 200, body = Vec<InventoryMovement>)), tag = "ecommerce", security(("api_key" = [])))]
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

#[utoipa::path(get, path = "/api/inventory/movements", params(MovementsQuery), responses((status = 200, body = Vec<InventoryMovement>)), tag = "ecommerce", security(("api_key" = [])))]
pub async fn list_all_movements(
    State(state): State<AppState>,
    Query(params): Query<MovementsQuery>,
) -> AppResult<Json<Vec<InventoryMovement>>> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let rows = db::inventory::list_all_movements(&state.pool, limit, offset).await?;
    Ok(Json(rows))
}

#[utoipa::path(post, path = "/api/inventory/adjustments", request_body = AdjustStockRequest, responses((status = 201, body = InventoryMovement)), tag = "ecommerce", security(("api_key" = [])))]
pub async fn adjust_stock(
    State(state): State<AppState>,
    Json(req): Json<AdjustStockRequest>,
) -> AppResult<(StatusCode, Json<InventoryMovement>)> {
    let movement =
        db::inventory::adjust_stock(&state.pool, req.product_id, req.change, req.note).await?;
    Ok((StatusCode::CREATED, Json(movement)))
}
