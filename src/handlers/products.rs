use crate::{
    db,
    errors::AppResult,
    models::product::{CreateProductRequest, Product, UpdateProductRequest},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{header::HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct PaginationParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[utoipa::path(post, path = "/api/products", request_body = CreateProductRequest, responses((status = 201, body = Product)), tag = "ecommerce", security(("api_key" = [])))]
pub async fn create_product(
    State(state): State<AppState>,
    Json(req): Json<CreateProductRequest>,
) -> AppResult<(StatusCode, Json<Product>)> {
    let product = db::products::create_product(&state.pool, req).await?;
    Ok((StatusCode::CREATED, Json(product)))
}

#[utoipa::path(get, path = "/api/products", params(PaginationParams), responses((status = 200, body = Vec<Product>)), tag = "ecommerce")]
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

#[utoipa::path(get, path = "/api/products/{id}", params(("id" = Uuid, Path, description = "Product ID")), responses((status = 200, body = Product)), tag = "ecommerce")]
pub async fn get_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Product>> {
    let product = db::products::get_product(&state.pool, id).await?;
    Ok(Json(product))
}

#[utoipa::path(put, path = "/api/products/{id}", params(("id" = Uuid, Path, description = "Product ID")), request_body = UpdateProductRequest, responses((status = 200, body = Product)), tag = "ecommerce", security(("api_key" = [])))]
pub async fn update_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProductRequest>,
) -> AppResult<Json<Product>> {
    let product = db::products::update_product(&state.pool, id, req).await?;
    Ok(Json(product))
}

#[utoipa::path(delete, path = "/api/products/{id}", params(("id" = Uuid, Path, description = "Product ID")), responses((status = 204), (status = 409)), tag = "ecommerce", security(("api_key" = [])))]
pub async fn delete_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    db::products::delete_product(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/api/products/low-stock", responses((status = 200, body = Vec<Product>)), tag = "ecommerce")]
pub async fn low_stock_report(State(state): State<AppState>) -> AppResult<Json<Vec<Product>>> {
    let products = db::products::low_stock_products(&state.pool).await?;
    Ok(Json(products))
}
