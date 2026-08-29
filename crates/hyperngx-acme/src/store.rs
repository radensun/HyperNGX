//! Tata letak penyimpanan sertifikat.
//!
//! /etc/hyperngx/tls/
//!   accounts/<hash-directory>.json     0600 root  — kunci akun ACME
//!   live/<slug>/fullchain.pem          0644 root  — sertifikat + rantai
//!   live/<slug>/privkey.pem            0600 root  — private key
//!   live/<slug>/request.json           0600 root  — parameter perpanjangan
//!   archive/<slug>/<timestamp>/        riwayat untuk rollback
//!   ticket/{current,previous}.key      0600 root  — TLS session ticket
//!   ca-bundle.pem                      untuk verifikasi OCSP stapling
//!
//! nginx master process berjalan sebagai root saat startup sehingga bisa
//! membaca privkey.pem; worker sudah drop privilege ke user `hyperngx`
//! sebelum melayani trafik.

use anyhow::Result;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Menulis berkas rahasia: 0600 sejak detik pertama.
///
/// Berkas dibuat lebih dulu dengan izin ketat, baru diisi. Menulis dulu
/// lalu chmod meninggalkan jendela waktu di mana kunci privat bisa dibaca
/// proses lain.
pub fn write_secret(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }

    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true).create(true).truncate(true).mode(0o600)
        .open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Nama berkas akun diturunkan dari URL directory, supaya akun staging dan
/// produksi tidak pernah tertukar.
pub fn directory_slug(directory: &str) -> String {
    directory
        .replace("https://", "")
        .replace(['/', ':', '.'], "-")
        .trim_matches('-')
        .to_string()
}
