use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

/// Builds the shared Postgres connection pool used across the app (handlers + background job).
pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
}
