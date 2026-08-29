//! HyperNGX supervisor — satu-satunya komponen yang berjalan privileged.
//!
//! Tanggung jawab (sengaja sempit):
//!   * menerima perintah bertipe dari API lewat unix socket 0660
//!   * render + uji + aktifkan konfigurasi nginx (hyperngx_core::apply)
//!   * menulis private key & sertifikat dengan mode 0600
//!   * menjalankan siklus perpanjangan ACME
//!   * merotasi TLS session ticket key
//!
//! Yang TIDAK dilakukan: menerima string konfigurasi mentah dari API,
//! menjalankan shell, atau membuka port publik. Permukaan serangannya
//! adalah satu enum `Command` yang tertutup.

mod certs;
mod config;
mod ipc;
mod peer;
mod state;
mod ticket;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let path = std::env::args()
        .skip_while(|a| a != "--config")
        .nth(1)
        .unwrap_or_else(|| "/etc/hyperngx/supervisor.toml".into());

    let cfg = config::Config::load(&path)?;
    tracing::info!(config = %path, "hyperngx-supervisor mulai");

    ticket::ensure_keys(&cfg.tls_dir)?;
    let state = state::SupervisorState::new(cfg);

    tokio::try_join!(
        ipc::serve(state.clone()),
        ticket::rotate_loop(state.clone()),
        state.acme_renewal_loop(),
    )?;
    Ok(())
}
