use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

use codex2api_common::version::{CODEX_PACKAGE_VERSION, CODEX_REF_COMMIT, CODEX_REF_COMMIT_DATE};
use codex2api_storage::Storage;

#[tokio::main]
async fn main() -> Result<()> {
    let debug = std::env::args().skip(1).any(|arg| arg == "--debug");
    let mut filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("codex2api=info"));
    if debug {
        filter = filter.add_directive("codex2api=debug".parse()?);
    }
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let oauth_bind =
        std::env::var("CODEX2API_OAUTH_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let admin_bind =
        std::env::var("CODEX2API_ADMIN_BIND").unwrap_or_else(|_| "127.0.0.1:8081".into());
    let user_bind =
        std::env::var("CODEX2API_USER_BIND").unwrap_or_else(|_| "127.0.0.1:8082".into());
    let api_bind = std::env::var("CODEX2API_API_BIND").unwrap_or_else(|_| "127.0.0.1:8083".into());
    let database_url = std::env::var("CODEX2API_DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("CODEX2API_DATABASE_URL is required"))?;

    tracing::info!(
        commit = CODEX_REF_COMMIT,
        commit_date = CODEX_REF_COMMIT_DATE,
        package_version = CODEX_PACKAGE_VERSION,
        "starting Codex2API with the pinned official Codex protocol target"
    );

    let storage = Storage::open(&database_url).await?;
    storage
        .public_url_settings()
        .await
        .context("load public OAuth/API addresses")?;
    let registered = storage
        .sync_supported_models(&codex2api_upstream::supported_models())
        .await?;
    tracing::info!(registered, "synchronized supported model presets");
    storage.ensure_default_admin().await?;
    storage.recover_interrupted_usage().await?;

    let oauth_origin = std::env::var("CODEX2API_PUBLIC_OAUTH_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let user_origin = std::env::var("CODEX2API_PUBLIC_USER_URL").unwrap_or_else(|_| {
        if cfg!(feature = "dev-frontend") {
            "http://127.0.0.1:3001".into()
        } else {
            "http://127.0.0.1:8082".into()
        }
    });
    let admin_origin = std::env::var("CODEX2API_PUBLIC_ADMIN_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8081".into());
    let api_origin = std::env::var("CODEX2API_PUBLIC_API_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8083".into());
    let apps = codex2api::routers(
        storage.clone(),
        &oauth_origin,
        &user_origin,
        &admin_origin,
        &api_origin,
    )?;
    let oauth_listener = tokio::net::TcpListener::bind(&oauth_bind)
        .await
        .with_context(|| format!("bind OAuth clients {oauth_bind}"))?;
    let admin_listener = tokio::net::TcpListener::bind(&admin_bind)
        .await
        .with_context(|| format!("bind administration {admin_bind}"))?;
    let user_listener = tokio::net::TcpListener::bind(&user_bind)
        .await
        .with_context(|| format!("bind user website {user_bind}"))?;
    let api_listener = tokio::net::TcpListener::bind(&api_bind)
        .await
        .with_context(|| format!("bind standard AI API {api_bind}"))?;
    tracing::info!(%oauth_bind,%admin_bind,%user_bind,%api_bind,"listening on independent surfaces");
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let signal = tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });
    let admin_shutdown = shutdown_rx.clone();
    let user_shutdown = shutdown_rx.clone();
    let api_shutdown = shutdown_rx.clone();
    let quota_monitor = apps.spawn_quota_monitor();
    let result = tokio::try_join!(
        async {
            axum::serve(oauth_listener, apps.oauth)
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
        },
        async {
            axum::serve(api_listener, apps.api)
                .with_graceful_shutdown(wait_shutdown(api_shutdown))
                .await
        }
    );
    signal.abort();
    quota_monitor.abort();
    let _ = quota_monitor.await;
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
