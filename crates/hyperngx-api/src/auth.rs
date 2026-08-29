//! Autentikasi admin.
//!
//! Keputusan desain:
//!   * Argon2id (m=64MiB, t=3, p=4) untuk hash password.
//!   * Sesi opaque 256-bit di DB, bukan JWT. Panel admin harus bisa
//!     mencabut sesi seketika (misalnya saat laptop admin hilang);
//!     JWT stateless tidak bisa dicabut tanpa blacklist yang justru
//!     mengembalikan state.
//!   * Cookie: HttpOnly, Secure, SameSite=Strict, Path=/.
//!   * CSRF: double-submit token pada semua metode tulis.
//!   * TOTP (RFC 6238) wajib untuk role owner.
//!   * Rate limit login: 5 percobaan / 15 menit / (IP + username).

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const SESSION_COOKIE: &str = "hngx_session";
pub const CSRF_COOKIE: &str = "hngx_csrf";
pub const SESSION_TTL_HOURS: i64 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Hanya baca: dashboard, log, status sertifikat.
    Viewer,
    /// Kelola proxy host & sertifikat, tidak bisa ubah user/sistem.
    Operator,
    /// Akses penuh termasuk pengaturan sistem & manajemen user.
    Owner,
}

impl Role {
    pub fn can_write(self) -> bool { self >= Role::Operator }
    pub fn is_owner(self) -> bool { self == Role::Owner }
}

pub fn hasher() -> Argon2<'static> {
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(65536, 3, 4, None).expect("param argon2 valid"),
    )
}

pub fn hash_password(plain: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(hasher()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .to_string())
}

pub fn verify_password(plain: &str, stored: &str) -> bool {
    match PasswordHash::new(stored) {
        Ok(parsed) => hasher().verify_password(plain.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

/// Token acak 256-bit. Yang disimpan di database adalah SHA-256-nya:
/// dump database yang bocor tidak memberi penyerang sesi yang bisa dipakai.
pub fn new_token() -> (String, Vec<u8>) {
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = hex::encode(raw);
    let digest = Sha256::digest(token.as_bytes()).to_vec();
    (token, digest)
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

// ---------------------------------------------------------------------
// Rate limit login
// ---------------------------------------------------------------------

#[derive(Default)]
pub struct AttemptTracker {
    entries: HashMap<String, (u32, Instant)>,
}

impl AttemptTracker {
    const WINDOW: Duration = Duration::from_secs(15 * 60);
    const MAX: u32 = 5;

    pub fn check(&mut self, key: &str) -> bool {
        self.gc();
        match self.entries.get(key) {
            Some((count, _)) => *count < Self::MAX,
            None => true,
        }
    }

    pub fn record_failure(&mut self, key: &str) {
        let entry = self.entries.entry(key.to_string()).or_insert((0, Instant::now()));
        entry.0 += 1;
        entry.1 = Instant::now();
    }

    pub fn reset(&mut self, key: &str) { self.entries.remove(key); }

    fn gc(&mut self) {
        self.entries.retain(|_, (_, at)| at.elapsed() < Self::WINDOW);
    }
}

// ---------------------------------------------------------------------
// Extractor sesi
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub role: Role,
}

impl CurrentUser {
    pub fn require_write(&self) -> ApiResult<()> {
        if self.role.can_write() { Ok(()) } else { Err(ApiError::Forbidden) }
    }
    pub fn require_owner(&self) -> ApiResult<()> {
        if self.role.is_owner() { Ok(()) } else { Err(ApiError::Forbidden) }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = cookie(parts, SESSION_COOKIE).ok_or(ApiError::Unauthorized)?;

        let row = sqlx::query!(
            r#"SELECT u.id, u.username, u.role as "role: Role"
               FROM sessions s JOIN users u ON u.id = s.user_id
               WHERE s.token_hash = $1
                 AND s.revoked_at IS NULL
                 AND s.expires_at > now()
                 AND u.disabled = FALSE"#,
            hash_token(&token)
        )
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::Unauthorized)?;

        // CSRF double-submit: header harus cocok dengan cookie pada setiap
        // metode yang mengubah state. Cookie SameSite=Strict sudah menutup
        // sebagian besar kasus; ini lapisan kedua untuk browser lama.
        if parts.method != axum::http::Method::GET && parts.method != axum::http::Method::HEAD {
            let cookie_csrf = cookie(parts, CSRF_COOKIE).unwrap_or_default();
            let header_csrf = parts.headers.get("x-csrf-token")
                .and_then(|v| v.to_str().ok()).unwrap_or_default();
            if cookie_csrf.is_empty() || cookie_csrf != header_csrf {
                return Err(ApiError::Forbidden);
            }
        }

        Ok(CurrentUser { id: row.id, username: row.username, role: row.role })
    }
}

fn cookie(parts: &Parts, name: &str) -> Option<String> {
    parts.headers.get(axum::http::header::COOKIE)?
        .to_str().ok()?
        .split(';')
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

/// Membuat header Set-Cookie untuk sesi baru.
pub fn session_cookies(token: &str, csrf: &str) -> [(axum::http::HeaderName, String); 2] {
    let max_age = SESSION_TTL_HOURS * 3600;
    [
        (axum::http::header::SET_COOKIE,
         format!("{SESSION_COOKIE}={token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={max_age}")),
        // CSRF token sengaja TIDAK HttpOnly: JavaScript harus bisa
        // membacanya untuk dikirim balik sebagai header.
        (axum::http::header::SET_COOKIE,
         format!("{CSRF_COOKIE}={csrf}; Secure; SameSite=Strict; Path=/; Max-Age={max_age}")),
    ]
}

pub fn clear_cookies() -> [(axum::http::HeaderName, String); 2] {
    [
        (axum::http::header::SET_COOKIE,
         format!("{SESSION_COOKIE}=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0")),
        (axum::http::header::SET_COOKIE,
         format!("{CSRF_COOKIE}=; Secure; SameSite=Strict; Path=/; Max-Age=0")),
    ]
}

/// Membuat akun owner pertama bila tabel users masih kosong.
/// Password acak dicetak ke /etc/hyperngx/bootstrap.txt (0600) — tidak
/// pernah ke log, karena log dikirim ke journald yang lebih luas aksesnya.
pub async fn bootstrap_owner(state: &AppState) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar!("SELECT count(*) FROM users")
        .fetch_one(&state.db).await?.unwrap_or(0);
    if count > 0 { return Ok(()); }

    let mut raw = [0u8; 18];
    rand::thread_rng().fill_bytes(&mut raw);
    let password = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, raw);

    sqlx::query!(
        "INSERT INTO users (username, email, password_hash, role)
         VALUES ($1, $2, $3, 'owner')",
        "admin", "admin@localhost", hash_password(&password)?
    ).execute(&state.db).await?;

    hyperngx_acme::store::write_secret(
        std::path::Path::new("/etc/hyperngx/bootstrap.txt"),
        format!("username: admin\npassword: {password}\n\nGanti password ini setelah login pertama, lalu hapus berkas ini.\n").as_bytes(),
    )?;
    tracing::warn!("akun owner awal dibuat — kredensial di /etc/hyperngx/bootstrap.txt");
    Ok(())
}
