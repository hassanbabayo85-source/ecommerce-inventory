use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::db;
use crate::errors::AppResult;
use crate::models::product::{CreateProductRequest, Product, UpdateProductRequest};
use crate::AppState;

pub async fn create_product(
    State(state): State<AppState>,
    Json(req): Json<CreateProductRequest>,
) -> AppResult<(StatusCode, Json<Product>)> {
    let product = db::products::create_product(&state.pool, req).await?;
    Ok((StatusCode::CREATED, Json(product)))
}

pub async fn list_products(State(state): State<AppState>) -> AppResult<Json<Vec<Product>>> {
    let products = db::products::list_products(&state.pool).await?;
    Ok(Json(products))
}

pub async fn get_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Product>> {
    let product = db::products::get_product(&state.pool, id).await?;
    Ok(Json(product))
}

pub async fn update_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProductRequest>,
) -> AppResult<Json<Product>> {
    let product = db::products::update_product(&state.pool, id, req).await?;
    Ok(Json(product))
}

pub async fn delete_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    db::products::delete_product(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn low_stock_report(State(state): State<AppState>) -> AppResult<Json<Vec<Product>>> {
    let products = db::products::low_stock_products(&state.pool).await?;
    Ok(Json(products))
}
