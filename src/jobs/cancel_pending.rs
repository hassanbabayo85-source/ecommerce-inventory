use sqlx::PgPool;
use std::time::Duration;
use tokio::time;

/// How often the sweep runs.
const TICK_INTERVAL: Duration = Duration::from_secs(60);
/// How long a pending order is allowed to stay unpaid before it's auto-cancelled.
const PENDING_TIMEOUT_MINUTES: i64 = 15;

/// Spawns a Tokio background task that, every `TICK_INTERVAL`, finds `pending` orders older
/// than `PENDING_TIMEOUT_MINUTES` and cancels them, restoring the stock that had been
/// reserved for them. Runs for the lifetime of the process.
pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = time::interval(TICK_INTERVAL);
        loop {
            interval.tick().await;
            run_once(&pool, PENDING_TIMEOUT_MINUTES).await;
        }
    });
}

async fn run_once(pool: &PgPool, timeout_minutes: i64) {
    match crate::db::orders::cancel_stale_pending_orders(pool, timeout_minutes).await {
        Ok(cancelled) if cancelled.is_empty() => {
            tracing::debug!("stale-order sweep: nothing to cancel");
        }
        Ok(cancelled) => {
            tracing::info!(
                count = cancelled.len(),
                order_ids = ?cancelled,
                "stale-order sweep: cancelled expired pending orders and restored stock"
            );
        }
        Err(err) => {
            tracing::error!(error = ?err, "stale-order sweep failed");
        }
    }
}
