//! HyperNGX API — proses tak berprivilege yang melayani admin UI.
//!
//! Mendengar di 127.0.0.1:8081 (di-proxy oleh nginx ke :8443 dengan TLS
//! dan access list), menyimpan state di PostgreSQL, dan mendelegasikan
//! setiap operasi berbahaya ke supervisor lewat unix socket.

mod auth;
mod db;
mod error;
mod routes;
mod snapshot;
mod state;
mod supervisor_client;
mod validate_input;

use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let url = std::env::var("HYPERNGX_DATABASE_URL")
        .unwrap_or_else(|_| db::DEFAULT_URL.to_string());
    let pool = db::connect(&url).await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;

    let state = state::AppState {
        db: pool,
        supervisor: Arc::new(supervisor_client::SupervisorClient::new(
            std::env::var("HYPERNGX_SUPERVISOR_SOCKET")
                .unwrap_or_else(|_| "/run/hyperngx/supervisor.sock".into()),
        )),
        login_attempts: Arc::new(tokio::sync::Mutex::new(auth::AttemptTracker::default())),
    };

    auth::bootstrap_owner(&state).await?;

    // Konfigurasi diterapkan sekali saat start, supaya berkas nginx selalu
    // mencerminkan database walau server sempat mati saat perubahan terakhir.
    if let Err(e) = snapshot::apply(&state, 0).await {
        tracing::warn!(error = %e, "penerapan konfigurasi awal gagal");
    }

    let app = routes::build(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8081").await?;
    tracing::info!("API siap di {}", listener.local_addr()?);

    // ConnectInfo dibutuhkan untuk rate limit login dan audit log.
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}
