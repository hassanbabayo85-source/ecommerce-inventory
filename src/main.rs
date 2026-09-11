use ecommerce_inventory::{build_app, db, jobs, run_migrations};
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ecommerce_inventory=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL environment variable must be set");
    let bind_addr =
        std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let pool = db::create_pool(&database_url).await?;
    tracing::info!("database pool created");

    run_migrations(&pool).await?;
    tracing::info!("database migrations applied");

    // Background sweeper: cancels pending orders older than 15 minutes and restores stock.
    jobs::cancel_pending::spawn(pool.clone());
    tracing::info!("background stale-order sweeper started (runs every 60s)");

    let app = build_app(pool);

    let addr: SocketAddr = bind_addr.parse().expect("invalid BIND_ADDR");
    tracing::info!(%addr, "starting server");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
