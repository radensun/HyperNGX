use crate::auth::{self, CurrentUser, Role};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::extract::{ConnectInfo, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Deserialize)]
pub struct LoginBody {
    username: String,
    password: String,
    totp: Option<String>,
}

#[derive(Serialize)]
pub struct MeResponse {
    id: i64,
    username: String,
    role: Role,
    totp_enabled: bool,
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<LoginBody>,
) -> ApiResult<impl IntoResponse> {
    let key = format!("{}|{}", peer.ip(), body.username);

    {
        let mut tracker = state.login_attempts.lock().await;
        if !tracker.check(&key) {
            return Err(ApiError::TooManyRequests);
        }
    }

    let user = sqlx::query!(
        r#"SELECT id, username, password_hash, role as "role: Role", totp_secret, totp_enabled
           FROM users WHERE username = $1 AND disabled = FALSE"#,
        body.username
    ).fetch_optional(&state.db).await?;

    // Verifikasi tetap dijalankan walau user tidak ada, memakai hash dummy,
    // supaya waktu respons tidak membocorkan username mana yang terdaftar.
    let ok = match &user {
        Some(u) => auth::verify_password(&body.password, &u.password_hash),
        None => {
            auth::verify_password(&body.password, DUMMY_HASH);
            false
        }
    };

    let Some(user) = user.filter(|_| ok) else {
        state.login_attempts.lock().await.record_failure(&key);
        audit(&state, None, peer, "login_failed", Some(&body.username), None).await;
        return Err(ApiError::Unauthorized);
    };

    if user.totp_enabled {
        let code = body.totp.as_deref().unwrap_or("");
        let secret = user.totp_secret.as_deref().unwrap_or_default();
        if !verify_totp(secret, code) {
            state.login_attempts.lock().await.record_failure(&key);
            audit(&state, Some(user.id), peer, "login_totp_failed", None, None).await;
            return Err(ApiError::Unauthorized);
        }
    }

    let (token, token_hash) = auth::new_token();
    let (csrf, _) = auth::new_token();

    sqlx::query!(
        "INSERT INTO sessions (token_hash, user_id, ip, expires_at)
         VALUES ($1, $2, $3, now() + ($4 || ' hours')::interval)",
        token_hash, user.id, ipnet(peer), auth::SESSION_TTL_HOURS.to_string()
    ).execute(&state.db).await?;

    sqlx::query!("UPDATE users SET last_login_at = now() WHERE id = $1", user.id)
        .execute(&state.db).await?;

    state.login_attempts.lock().await.reset(&key);
    audit(&state, Some(user.id), peer, "login", None, None).await;

    Ok((
        auth::session_cookies(&token, &csrf),
        Json(MeResponse {
            id: user.id, username: user.username, role: user.role,
            totp_enabled: user.totp_enabled,
        }),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<impl IntoResponse> {
    sqlx::query!(
        "UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
        user.id
    ).execute(&state.db).await?;
    Ok((auth::clear_cookies(), Json(serde_json::json!({ "ok": true }))))
}

pub async fn me(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<MeResponse>> {
    let row = sqlx::query!("SELECT totp_enabled FROM users WHERE id = $1", user.id)
        .fetch_one(&state.db).await?;
    Ok(Json(MeResponse {
        id: user.id, username: user.username, role: user.role,
        totp_enabled: row.totp_enabled,
    }))
}

/// Menyiapkan TOTP: mengembalikan URI otpauth untuk dipindai.
/// Secret belum aktif sampai dikonfirmasi dengan satu kode yang benar —
/// mencegah admin mengunci dirinya sendiri karena salah memindai.
pub async fn totp_enroll(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    let secret = totp_rs::Secret::generate_secret();
    let raw = secret.to_bytes().map_err(|e| ApiError::Other(anyhow::anyhow!("{e:?}")))?;

    sqlx::query!(
        "UPDATE users SET totp_secret = $1, totp_enabled = FALSE WHERE id = $2",
        raw, user.id
    ).execute(&state.db).await?;

    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1, 6, 1, 30, raw,
        Some("HyperNGX".into()), user.username.clone(),
    ).map_err(|e| ApiError::Other(anyhow::anyhow!("{e}")))?;

    Ok(Json(serde_json::json!({ "otpauth_url": totp.get_url() })))
}

#[derive(Deserialize)]
pub struct TotpConfirm { code: String }

pub async fn totp_confirm(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<TotpConfirm>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = sqlx::query!("SELECT totp_secret FROM users WHERE id = $1", user.id)
        .fetch_one(&state.db).await?;
    let secret = row.totp_secret.ok_or(ApiError::BadRequest("TOTP belum disiapkan".into()))?;

    if !verify_totp_bytes(&secret, &body.code) {
        return Err(ApiError::BadRequest("Kode tidak cocok. Periksa jam perangkat Anda.".into()));
    }
    sqlx::query!("UPDATE users SET totp_enabled = TRUE WHERE id = $1", user.id)
        .execute(&state.db).await?;
    Ok(Json(serde_json::json!({ "totp_enabled": true })))
}

fn verify_totp(secret: &[u8], code: &str) -> bool { verify_totp_bytes(secret, code) }

fn verify_totp_bytes(secret: &[u8], code: &str) -> bool {
    let Ok(totp) = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1, 6, 1, 30, secret.to_vec(), None, String::new()
    ) else { return false };
    totp.check_current(code).unwrap_or(false)
}

fn ipnet(peer: SocketAddr) -> sqlx::types::ipnetwork::IpNetwork {
    peer.ip().into()
}

/// Hash Argon2id atas string acak, dipakai untuk menyamakan waktu respons
/// ketika username tidak ditemukan.
const DUMMY_HASH: &str = "$argon2id$v=19$m=65536,t=3,p=4$c29tZXNhbHR2YWx1ZQ$kIVN0Zx2Q3vP2fZ0nQyq9rV1cO8lLwXQ9m2Yq3ZzR8k";

pub async fn audit(
    state: &AppState,
    user_id: Option<i64>,
    peer: SocketAddr,
    action: &str,
    entity_id: Option<&str>,
    diff: Option<serde_json::Value>,
) {
    let _ = sqlx::query!(
        "INSERT INTO audit_log (user_id, ip, action, entity_id, diff)
         VALUES ($1, $2, $3, $4, $5)",
        user_id, ipnet(peer), action, entity_id, diff
    ).execute(&state.db).await;
}
