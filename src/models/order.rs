use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "order_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    Pending,
    Paid,
    Shipped,
    Cancelled,
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OrderStatus::Pending => "pending",
            OrderStatus::Paid => "paid",
            OrderStatus::Shipped => "shipped",
            OrderStatus::Cancelled => "cancelled",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Order {
    pub id: Uuid,
    pub customer_email: String,
    pub status: OrderStatus,
    pub total_amount: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    /// Per-order opaque access token. NEVER serialized to JSON via this field —
    /// it is returned exactly once via `CreateOrderResponse::access_token`.
    #[serde(skip)]
    pub access_token: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OrderItem {
    pub id: Uuid,
    pub order_id: Uuid,
    pub product_id: Uuid,
    pub quantity: i32,
    pub unit_price: Decimal,
}

/// Full order + items. `#[serde(flatten)]` keeps the JSON shape flat:
/// `{ id, customer_email, status, total_amount, ..., items: [...] }`.
#[derive(Debug, Clone, Serialize)]
pub struct OrderWithItems {
    #[serde(flatten)]
    pub order: Order,
    pub items: Vec<OrderItem>,
}

#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    pub customer_email: String,
    pub items: Vec<CreateOrderItem>,
}

#[derive(Debug, Deserialize)]
pub struct CreateOrderItem {
    pub product_id: Uuid,
    pub quantity: i32,
}

/// Backwards-compatible alias — older code/tests used this name.
pub type OrderItemRequest = CreateOrderItem;

/// Response body for POST /api/orders.
///
/// `access_token` is returned exactly ONCE; clients must store it and send
/// it as `X-Order-Token` on subsequent GET /api/orders/:id requests.
#[derive(Debug, Serialize)]
pub struct CreateOrderResponse {
    #[serde(flatten)]
    pub order: Order,
    pub items: Vec<OrderItem>,
    pub access_token: Uuid,
}
