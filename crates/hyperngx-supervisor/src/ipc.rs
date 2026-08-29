use crate::state::SupervisorState;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// Protokol IPC: JSON-lines di atas unix socket.
/// Setiap varian sudah tervalidasi tipenya — API tidak bisa
/// menyuntik direktif nginx sembarangan.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Command {
    /// Render ulang seluruh konfigurasi dari snapshot state yang dikirim API.
    ApplyConfig { generation_id: String, snapshot: Snapshot },
    /// Uji konfigurasi tanpa mengaktifkan (tombol "Uji konfigurasi" di UI).
    DryRun { snapshot: Snapshot },
    /// Minta sertifikat baru / perpanjangan paksa.
    RequestCert { slug: String, domains: Vec<String>, challenge: hyperngx_acme::Challenge },
    /// Hapus sertifikat + berkas terkait.
    RevokeCert { slug: String },
    /// Kembali ke generation tertentu (atau yang sebelum aktif).
    Rollback { generation_id: Option<String> },
    /// Status runtime untuk dashboard.
    Status,
}

/// Bentuk snapshot sengaja eksplisit: supervisor tidak menerima
/// `serde_json::Value` bebas, sehingga field asing ditolak deserializer.
#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub hosts: Vec<hyperngx_core::model::ProxyHost>,
    #[serde(default)]
    pub globals: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Ok { generation_id: Option<String>, detail: serde_json::Value },
    Err { code: String, message: String },
}

impl Response {
    pub fn ok(detail: serde_json::Value) -> Self {
        Response::Ok { generation_id: None, detail }
    }
    pub fn ok_gen(id: impl Into<String>, detail: serde_json::Value) -> Self {
        Response::Ok { generation_id: Some(id.into()), detail }
    }
    pub fn err(code: &str, message: impl std::fmt::Display) -> Self {
        Response::Err { code: code.into(), message: message.to_string() }
    }
}

pub async fn serve(state: SupervisorState) -> Result<()> {
    let path = &state.cfg.socket_path;
    if path.exists() {
        std::fs::remove_file(path).context("socket lama tidak bisa dihapus")?;
    }
    let listener = UnixListener::bind(path)
        .with_context(|| format!("gagal bind {}", path.display()))?;

    // 0660 root:hyperngx-api. Batas akses ditegakkan kernel; tidak ada
    // token bersama yang bisa bocor lewat berkas konfigurasi.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))?;
    if let Some(gid) = crate::peer::gid_of_group(&state.cfg.allowed_peer_user) {
        let _ = nix::unistd::chown(path.as_path(), None, Some(gid));
    }

    tracing::info!(socket = %path.display(), "IPC siap");

    loop {
        let (stream, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, state).await {
                tracing::warn!(error = %e, "koneksi IPC berakhir dengan error");
            }
        });
    }
}

async fn handle(stream: UnixStream, state: SupervisorState) -> Result<()> {
    // Verifikasi identitas pemanggil lewat SO_PEERCRED. Izin berkas socket
    // sudah membatasi, tapi pemeriksaan ini menutup celah bila mode socket
    // pernah salah disetel oleh operator.
    crate::peer::verify(&stream, &state.cfg.allowed_peer_user)?;

    let (rx, mut tx) = stream.into_split();
    let mut lines = BufReader::new(rx).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() { continue; }
        let response = match serde_json::from_str::<Command>(&line) {
            Ok(cmd) => dispatch(cmd, &state).await,
            Err(e) => Response::err("bad_request", e),
        };
        let mut out = serde_json::to_vec(&response)?;
        out.push(b'\n');
        tx.write_all(&out).await?;
        tx.flush().await?;
    }
    Ok(())
}

async fn dispatch(cmd: Command, state: &SupervisorState) -> Response {
    let name = match &cmd {
        Command::ApplyConfig { .. } => "apply_config",
        Command::DryRun { .. } => "dry_run",
        Command::RequestCert { .. } => "request_cert",
        Command::RevokeCert { .. } => "revoke_cert",
        Command::Rollback { .. } => "rollback",
        Command::Status => "status",
    };
    tracing::info!(op = name, "perintah diterima");

    let result = match cmd {
        Command::ApplyConfig { generation_id, snapshot } =>
            state.apply_config(&generation_id, &snapshot).await,
        Command::DryRun { snapshot } => state.dry_run(&snapshot).await,
        Command::RequestCert { slug, domains, challenge } =>
            state.request_cert(&slug, &domains, &challenge).await,
        Command::RevokeCert { slug } => state.revoke_cert(&slug).await,
        Command::Rollback { generation_id } => state.rollback(generation_id.as_deref()).await,
        Command::Status => state.status().await,
    };

    match result {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(op = name, error = %e, "perintah gagal");
            Response::err("internal", e)
        }
    }
}
