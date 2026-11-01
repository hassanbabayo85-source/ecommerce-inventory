use axum::{
    extract::{Path, Query, State},
    http::{header::HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::db;
use crate::errors::AppResult;
use crate::models::product::{CreateProductRequest, Product, UpdateProductRequest};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn create_product(
    State(state): State<AppState>,
    Json(req): Json<CreateProductRequest>,
) -> AppResult<(StatusCode, Json<Product>)> {
    let product = db::products::create_product(&state.pool, req).await?;
    Ok((StatusCode::CREATED, Json(product)))
}

pub async fn list_products(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> AppResult<Response> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);

    let (products, total) = db::products::list_products(&state.pool, limit, offset).await?;

    let mut resp = Json(products).into_response();
    resp.headers_mut().insert(
        HeaderName::from_static("x-total-count"),
        HeaderValue::from(total),
    );
    resp.headers_mut()
        .insert(HeaderName::from_static("x-limit"), HeaderValue::from(limit));
    resp.headers_mut().insert(
        HeaderName::from_static("x-offset"),
        HeaderValue::from(offset),
    );
    Ok(resp)
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
