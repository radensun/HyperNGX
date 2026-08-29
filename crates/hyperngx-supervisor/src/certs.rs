//! Operasi sertifikat di sisi privileged: penerbitan, pemasangan berkas,
//! inventaris, dan siklus perpanjangan.

use crate::state::SupervisorState;
use anyhow::{Context, Result};
use hyperngx_acme::renewal::{backoff_secs, RenewalPolicy};
use hyperngx_acme::{CertRequest, IssuedCert};
use serde::Serialize;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct CertInfo {
    pub slug: String,
    pub not_after: Option<chrono::DateTime<chrono::Utc>>,
    pub days_left: Option<i64>,
}

pub fn inventory(tls_dir: &Path) -> Result<Vec<CertInfo>> {
    let live = tls_dir.join("live");
    if !live.is_dir() { return Ok(vec![]); }

    let mut out = Vec::new();
    for entry in std::fs::read_dir(&live)? {
        let entry = entry?;
        let slug = entry.file_name().to_string_lossy().into_owned();
        let not_after = read_not_after(&entry.path().join("fullchain.pem")).ok();
        let days_left = not_after.map(|t| (t - chrono::Utc::now()).num_days());
        out.push(CertInfo { slug, not_after, days_left });
    }
    out.sort_by_key(|c| c.days_left.unwrap_or(i64::MAX));
    Ok(out)
}

/// Membaca notAfter lewat `openssl x509`. Sengaja memakai biner sistem
/// alih-alih menarik crate parser X.509: satu dependensi lebih sedikit
/// pada proses yang berjalan sebagai root.
fn read_not_after(pem: &Path) -> Result<chrono::DateTime<chrono::Utc>> {
    let out = std::process::Command::new("openssl")
        .args(["x509", "-noout", "-enddate", "-in"])
        .arg(pem)
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let value = text.trim().strip_prefix("notAfter=")
        .context("format keluaran openssl tidak dikenali")?;
    Ok(chrono::DateTime::parse_from_str(value, "%b %e %H:%M:%S %Y GMT")?.into())
}

/// Menulis sertifikat ke disk dengan izin yang benar.
///
/// privkey.pem 0600 root: hanya master process nginx (yang start sebagai
/// root) yang membacanya. Worker sudah drop privilege sebelum melayani
/// trafik, dan proses API tidak pernah bisa membukanya.
pub fn install(tls_dir: &Path, cert: &IssuedCert) -> Result<()> {
    let live = tls_dir.join("live").join(&cert.slug);
    let archive = tls_dir.join("archive").join(&cert.slug)
        .join(chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string());
    std::fs::create_dir_all(&live)?;
    std::fs::create_dir_all(&archive)?;

    for (name, body, mode) in [
        ("fullchain.pem", &cert.fullchain_pem, 0o644),
        ("privkey.pem", &cert.private_key_pem, 0o600),
    ] {
        // Arsip dulu (untuk rollback), lalu tulis atomik ke live.
        std::fs::write(archive.join(name), body)?;
        std::fs::set_permissions(archive.join(name), std::fs::Permissions::from_mode(mode))?;

        let tmp = live.join(format!("{name}.tmp"));
        std::fs::write(&tmp, body)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
        std::fs::rename(&tmp, live.join(name))?;
    }
    tracing::info!(slug = %cert.slug, "sertifikat dipasang");
    Ok(())
}

pub fn remove(tls_dir: &Path, slug: &str) -> Result<()> {
    let live = tls_dir.join("live").join(slug);
    if live.is_dir() { std::fs::remove_dir_all(&live)?; }
    Ok(())
}

pub async fn issue(state: &SupervisorState, req: &CertRequest) -> Result<CertInfo> {
    let cert = hyperngx_acme::order::issue(
        &state.cfg.acme.directory,
        state.cfg.acme.contact_email.as_deref(),
        &state.cfg.tls_dir.join("accounts"),
        &state.cfg.acme_webroot,
        req,
    ).await?;

    install(&state.cfg.tls_dir, &cert)?;
    let days_left = Some((cert.not_after - chrono::Utc::now()).num_days());
    Ok(CertInfo { slug: cert.slug, not_after: Some(cert.not_after), days_left })
}

/// Memproses semua sertifikat yang jatuh tempo. Mengembalikan jumlah yang
/// berhasil diperbarui — pemanggil yang memutuskan kapan reload.
pub async fn renew_due(state: &SupervisorState) -> Result<usize> {
    let policy = RenewalPolicy::default();
    let now = chrono::Utc::now();
    let mut renewed = 0usize;

    for info in inventory(&state.cfg.tls_dir)? {
        let Some(not_after) = info.not_after else { continue };
        if !policy.due(not_after, now) { continue; }

        // Jitter: ribuan instance HyperNGX tidak boleh menyerbu CA
        // pada jam yang sama.
        let jitter = rand::random::<u64>() % (policy.max_jitter.num_seconds() as u64).max(1);
        tokio::time::sleep(std::time::Duration::from_secs(jitter)).await;

        let req = load_request(&state.cfg.tls_dir, &info.slug)?;
        let mut attempt = 0u32;
        loop {
            match issue(state, &req).await {
                Ok(_) => { renewed += 1; break; }
                Err(e) if attempt < 3 => {
                    let wait = backoff_secs(attempt);
                    tracing::warn!(slug = %info.slug, error = %e, wait, "perpanjangan gagal, mencoba lagi");
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    attempt += 1;
                }
                Err(e) => {
                    tracing::error!(slug = %info.slug, error = %e, "perpanjangan menyerah");
                    break;
                }
            }
        }
    }
    Ok(renewed)
}

/// Parameter penerbitan disimpan berdampingan dengan sertifikatnya
/// (`live/<slug>/request.json`) supaya perpanjangan tidak bergantung pada
/// database — sertifikat tetap bisa diperpanjang walau API sedang mati.
fn load_request(tls_dir: &Path, slug: &str) -> Result<CertRequest> {
    let path = tls_dir.join("live").join(slug).join("request.json");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("{} tidak ada", path.display()))?;
    Ok(serde_json::from_str(&raw)?)
}
