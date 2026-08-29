//! Rotasi TLS session ticket key.
//!
//! Tanpa rotasi, ticket key statis membatalkan forward secrecy: siapa pun
//! yang mencuri berkas key bisa mendekripsi rekaman trafik lama. Kita
//! menggeser current -> previous tiap 12 jam dan membuat current baru
//! (80 byte acak untuk AES-256), lalu reload nginx. Klien dengan ticket
//! lama tetap bisa resume karena nginx menerima key kedua.

use crate::state::SupervisorState;
use anyhow::Result;
use rand::RngCore;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const KEY_LEN: usize = 80;

pub fn ensure_keys(tls_dir: &Path) -> Result<()> {
    let dir = tls_dir.join("ticket");
    std::fs::create_dir_all(&dir)?;
    for name in ["current.key", "previous.key"] {
        if !dir.join(name).exists() { write_random(&dir.join(name))?; }
    }
    Ok(())
}

fn write_random(path: &Path) -> Result<()> {
    let mut key = [0u8; KEY_LEN];
    rand::thread_rng().fill_bytes(&mut key);
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, key)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub async fn rotate_loop(state: SupervisorState) -> Result<()> {
    let dir = state.cfg.tls_dir.join("ticket");
    ensure_keys(&state.cfg.tls_dir)?;
    let period = std::time::Duration::from_secs(state.cfg.acme.ticket_rotate_hours * 3600);

    loop {
        tokio::time::sleep(period).await;
        // current -> previous, lalu current baru. Urutan ini penting:
        // kalau terbalik, ticket yang baru saja diterbitkan langsung
        // tidak bisa di-resume.
        std::fs::rename(dir.join("current.key"), dir.join("previous.key"))?;
        write_random(&dir.join("current.key"))?;

        match state.reload_only().await {
            Ok(()) => tracing::info!("ticket key dirotasi"),
            Err(e) => tracing::error!(error = %e, "reload setelah rotasi ticket gagal"),
        }
    }
}
