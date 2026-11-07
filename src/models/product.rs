use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Product {
    pub id: Uuid,
    pub name: String,
    pub sku: String,
    pub price: Decimal,
    pub stock_quantity: i32,
    pub low_stock_threshold: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateProductRequest {
    pub name: String,
    pub sku: String,
    pub price: Decimal,
    pub stock_quantity: i32,
    pub low_stock_threshold: Option<i32>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateProductRequest {
    pub name: Option<String>,
    pub sku: Option<String>,
    pub price: Option<Decimal>,
    pub stock_quantity: Option<i32>,
    pub low_stock_threshold: Option<i32>,
}
