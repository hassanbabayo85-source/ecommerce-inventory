use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct InventoryMovement {
    pub id: Uuid,
    pub product_id: Uuid,
    pub change: i32,
    pub reason: String,
    pub reference_id: Option<Uuid>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AdjustStockRequest {
    pub product_id: Uuid,
    pub change: i32,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct MovementsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub const REASON_INITIAL: &str = "initial";
pub const REASON_ORDER_RESERVATION: &str = "order_reservation";
pub const REASON_ORDER_CANCELLATION: &str = "order_cancellation";
pub const REASON_ADMIN_ADJUSTMENT: &str = "admin_adjustment";
pub const REASON_STOCK_SYNC: &str = "stock_sync";
