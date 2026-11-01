use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

/// Builds the shared Postgres connection pool.
///
/// Reads `DB_MAX_CONNECTIONS` (default 10) so deployments can tune the pool
/// without recompiling. `acquire_timeout` is fixed at 10s to keep request
/// latency predictable under load.
pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let max_connections: u32 = std::env::var("DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
}
