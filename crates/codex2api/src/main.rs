use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

use codex2api_storage::Storage;
use codex2api_version::{CODEX_PACKAGE_VERSION, CODEX_REF_COMMIT, CODEX_REF_COMMIT_DATE};

const DEFAULT_BIND: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("codex2api=info".parse()?))
        .init();

    let bind = std::env::var("CODEX2API_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_string());
    let db_path = std::env::var("CODEX2API_DB")
        .unwrap_or_else(|_| codex2api_storage::DEFAULT_DB_PATH.to_string());

    tracing::info!(
        commit = CODEX_REF_COMMIT,
        commit_date = CODEX_REF_COMMIT_DATE,
        package_version = CODEX_PACKAGE_VERSION,
        "starting Codex2API aligned with official Codex snapshot"
    );

    let storage = Storage::open(&db_path).await?;
    codex2api_accounts::SupplierAccountStore::open(storage.clone())
        .align_user_agents()
        .await?;
    storage.ensure_default_admin().await?;
    storage.recover_interrupted_usage().await?;

    let public_base_url = std::env::var("CODEX2API_PUBLIC_BASE_URL").ok();
    let app = codex2api::router_with_public_base_url(storage.clone(), public_base_url.as_deref())?;

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("bind {bind}"))?;
    tracing::info!(%bind, db = %db_path, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("shutting down");
    storage.close().await;
    Ok(())
}

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
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutdown signal received");
}
