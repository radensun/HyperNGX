use anyhow::Result;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;
use std::time::Duration;

/// PostgreSQL dipakai sejak versi pertama, termasuk pada instalasi satu
/// server. Alasannya bukan volume data (state HyperNGX kecil: ratusan host),
/// melainkan agar mode multi-node tidak menuntut migrasi skema — beberapa
/// instance tinggal diarahkan ke klaster yang sama.
///
/// Koneksi default lewat unix socket dengan `peer` authentication sehingga
/// tidak ada password di berkas konfigurasi: identitas ditegakkan kernel,
/// sama seperti socket ke supervisor.
///
/// Ukuran pool sengaja kecil (8). Ini jalur kontrol, bukan jalur data —
/// trafik pengguna tidak pernah menyentuh database. Pool besar hanya
/// memindahkan antrean dari aplikasi ke PostgreSQL.
pub async fn connect(url: &str) -> Result<PgPool> {
    let opts = PgConnectOptions::from_str(url)?
        .application_name("hyperngx-api")
        .statement_cache_capacity(256);

    Ok(PgPoolOptions::new()
        .max_connections(8)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .connect_with(opts)
        .await?)
}

/// URL default: unix socket, database dan role `hyperngx`.
pub const DEFAULT_URL: &str =
    "postgres:///hyperngx?host=/var/run/postgresql&application_name=hyperngx-api";
