//! Klien unix socket ke supervisor.
//!
//! Satu perintah per koneksi, tanpa pipelining: jalur kontrol dipakai
//! beberapa kali per menit, jadi kesederhanaan lebih berharga daripada
//! throughput. Timeout wajib — `nginx -t` pada instalasi besar bisa
//! memakan detik, tapi tidak boleh menggantung request admin selamanya.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

const TIMEOUT: Duration = Duration::from_secs(60);

pub struct SupervisorClient {
    path: PathBuf,
}

impl SupervisorClient {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub async fn send<C: serde::Serialize, R: DeserializeOwned>(&self, cmd: &C) -> Result<R> {
        let fut = async {
            let stream = UnixStream::connect(&self.path)
                .await
                .with_context(|| format!("supervisor tidak merespons di {}", self.path.display()))?;
            let (rx, mut tx) = stream.into_split();

            let mut payload = serde_json::to_vec(cmd)?;
            payload.push(b'\n');
            tx.write_all(&payload).await?;
            tx.flush().await?;

            let mut line = String::new();
            BufReader::new(rx).read_line(&mut line).await?;
            anyhow::ensure!(!line.trim().is_empty(), "supervisor menutup koneksi tanpa jawaban");
            Ok::<R, anyhow::Error>(serde_json::from_str(&line)?)
        };

        tokio::time::timeout(TIMEOUT, fut)
            .await
            .context("supervisor tidak menjawab dalam 60 detik")?
    }
}

/// Bentuk balasan supervisor, dicerminkan di sisi API.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum SupervisorResponse {
    Ok { generation_id: Option<String>, detail: serde_json::Value },
    Err { code: String, message: String },
}

impl SupervisorResponse {
    pub fn into_result(self) -> Result<(Option<String>, serde_json::Value), crate::error::ApiError> {
        match self {
            SupervisorResponse::Ok { generation_id, detail } => Ok((generation_id, detail)),
            SupervisorResponse::Err { code, message } => {
                tracing::warn!(code, message, "supervisor menolak perintah");
                Err(crate::error::ApiError::ConfigRejected(message))
            }
        }
    }
}
