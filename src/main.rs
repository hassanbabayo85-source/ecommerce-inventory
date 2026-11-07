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
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let admin_api_key = std::env::var("ADMIN_API_KEY")
        .map_err(|_| anyhow::anyhow!("ADMIN_API_KEY environment variable must be set"))?;
    if admin_api_key.len() < 16 {
        anyhow::bail!(
            "ADMIN_API_KEY must be at least 16 characters (got {})",
            admin_api_key.len()
        );
    }

    let cors_origins: Vec<String> = std::env::var("CORS_ALLOWED_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let pool = db::create_pool(&database_url).await?;
    tracing::info!("database pool created");

    run_migrations(&pool).await?;
    tracing::info!("database migrations applied");

    jobs::cancel_pending::spawn(pool.clone());
    tracing::info!("background stale-order sweeper started (runs every 60s)");

    let app = build_app(pool, admin_api_key, cors_origins);

    let addr: SocketAddr = bind_addr.parse().expect("invalid BIND_ADDR");
    tracing::info!(%addr, "starting server");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    tracing::info!("server shut down cleanly");
    Ok(())
}

/// Resolves on SIGINT or SIGTERM so the server can drain in-flight requests
/// before exiting. Required for clean Docker / Kubernetes rollouts.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("SIGINT received"),
        _ = terminate => tracing::info!("SIGTERM received"),
    }
    tracing::info!("draining in-flight requests...");
}
