use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

use codex2api_storage::Storage;
use codex2api_version::{CODEX_PACKAGE_VERSION, CODEX_REF_COMMIT, CODEX_REF_COMMIT_DATE};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("codex2api=info".parse()?))
        .init();

    let api_bind = std::env::var("CODEX2API_API_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let admin_bind =
        std::env::var("CODEX2API_ADMIN_BIND").unwrap_or_else(|_| "127.0.0.1:8081".into());
    let user_bind =
        std::env::var("CODEX2API_USER_BIND").unwrap_or_else(|_| "127.0.0.1:8082".into());
    let db_path = std::env::var("CODEX2API_DB")
        .unwrap_or_else(|_| codex2api_storage::DEFAULT_DB_PATH.to_string());

    tracing::info!(
        commit = CODEX_REF_COMMIT,
        commit_date = CODEX_REF_COMMIT_DATE,
        package_version = CODEX_PACKAGE_VERSION,
        "starting Codex2API with the pinned official Codex protocol target"
    );

    let storage = Storage::open(&db_path).await?;
    let registered = storage
        .sync_supported_models(&codex2api_upstream::supported_models())
        .await?;
    tracing::info!(registered, "synchronized supported model presets");
    codex2api_accounts::SupplierAccountStore::open(storage.clone())
        .align_user_agents()
        .await?;
    storage.ensure_default_admin().await?;
    storage.recover_interrupted_usage().await?;

    let api_origin = std::env::var("CODEX2API_PUBLIC_API_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let user_origin = std::env::var("CODEX2API_PUBLIC_USER_URL").unwrap_or_else(|_| {
        if cfg!(feature = "dev-frontend") {
            "http://127.0.0.1:3001".into()
        } else {
            "http://127.0.0.1:8082".into()
        }
    });
    let apps = codex2api::routers(storage.clone(), &api_origin, &user_origin)?;
    let api_listener = tokio::net::TcpListener::bind(&api_bind)
        .await
        .with_context(|| format!("bind AI API {api_bind}"))?;
    let admin_listener = tokio::net::TcpListener::bind(&admin_bind)
        .await
        .with_context(|| format!("bind administration {admin_bind}"))?;
    let user_listener = tokio::net::TcpListener::bind(&user_bind)
        .await
        .with_context(|| format!("bind user website {user_bind}"))?;
    tracing::info!(%api_bind,%admin_bind,%user_bind,db=%db_path,"listening on independent surfaces");
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let signal = tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });
    let admin_shutdown = shutdown_rx.clone();
    let user_shutdown = shutdown_rx.clone();
    let result = tokio::try_join!(
        async {
            axum::serve(api_listener, apps.api)
                .with_graceful_shutdown(wait_shutdown(shutdown_rx))
                .await
        },
        async {
            axum::serve(admin_listener, apps.admin)
                .with_graceful_shutdown(wait_shutdown(admin_shutdown))
                .await
        },
        async {
            axum::serve(user_listener, apps.user)
                .with_graceful_shutdown(wait_shutdown(user_shutdown))
                .await
        }
    );
    signal.abort();
    tracing::info!("shutting down");
    storage.close().await;
    result?;
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

async fn wait_shutdown(mut signal: tokio::sync::watch::Receiver<bool>) {
    let _ = signal.wait_for(|value| *value).await;
}
