use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::{snapshot, validate_input};
use axum::extract::{ConnectInfo, Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Serialize)]
pub struct HostSummary {
    id: i64,
    domains: serde_json::Value,
    targets: serde_json::Value,
    scheme: String,
    ssl_enabled: bool,
    http3: bool,
    enabled: bool,
    cert_state: &'static str,
    cert_days_left: Option<i64>,
    health: &'static str,
}

pub async fn list(State(state): State<AppState>, _u: CurrentUser) -> ApiResult<Json<Vec<HostSummary>>> {
    let rows = sqlx::query!(
        r#"SELECT h.id, h.domains, h.targets, h.scheme, h.ssl_enabled, h.http3, h.enabled,
                  c.not_after
           FROM proxy_hosts h
           LEFT JOIN certificates c ON c.id = h.certificate_id
           ORDER BY h.id"#
    ).fetch_all(&state.db).await?;

    let now = chrono::Utc::now();
    Ok(Json(rows.into_iter().map(|r| {
        let days = r.not_after.map(|t| (t - now).num_days());
        let cert_state = match days {
            None => "none",
            Some(d) if d < 0 => "expired",
            Some(d) if d < 15 => "expiring",
            Some(_) => "valid",
        };
        HostSummary {
            id: r.id, domains: r.domains, targets: r.targets, scheme: r.scheme,
            ssl_enabled: r.ssl_enabled, http3: r.http3, enabled: r.enabled,
            cert_state, cert_days_left: days,
            // Health check aktif ada di Fase 2; sampai saat itu status
            // dilaporkan apa adanya, bukan ditebak.
            health: "unknown",
        }
    }).collect()))
}

pub async fn detail(
    State(state): State<AppState>, _u: CurrentUser, Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = sqlx::query!("SELECT row_to_json(h) as data FROM proxy_hosts h WHERE id = $1", id)
        .fetch_optional(&state.db).await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(row.data.unwrap_or(serde_json::Value::Null)))
}

#[derive(Deserialize)]
pub struct HostInput {
    pub domains: Vec<String>,
    #[serde(default = "http")] pub scheme: String,
    pub targets: Vec<serde_json::Value>,
    #[serde(default = "rr")] pub load_balance: String,
    #[serde(default)] pub locations: Vec<serde_json::Value>,
    #[serde(default)] pub certificate_id: Option<i64>,
    #[serde(default)] pub ssl_enabled: bool,
    #[serde(default = "yes")] pub force_ssl: bool,
    #[serde(default = "yes")] pub http2: bool,
    #[serde(default)] pub http3: bool,
    #[serde(default)] pub hsts_disabled: bool,
    #[serde(default = "yes")] pub hardening: bool,
    #[serde(default = "yes")] pub block_common_exploits: bool,
    #[serde(default)] pub access_list_id: Option<i64>,
    #[serde(default = "max_conn")] pub max_conn: i32,
    #[serde(default = "body_size")] pub client_max_body_size: String,
    #[serde(default)] pub advanced_config: String,
    #[serde(default = "yes")] pub enabled: bool,
}
fn http() -> String { "http".into() }
fn rr() -> String { "round_robin".into() }
fn yes() -> bool { true }
fn max_conn() -> i32 { 4000 }
fn body_size() -> String { "64m".into() }

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<HostInput>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require_write()?;
    let domains = validate_input::domains(&body.domains)?;
    validate_input::advanced(&body.advanced_config)?;

    let id: i64 = sqlx::query_scalar!(
        "INSERT INTO proxy_hosts
           (domains, scheme, targets, load_balance, locations, certificate_id,
            ssl_enabled, force_ssl, http2, http3, hsts_disabled, hardening,
            block_common_exploits, access_list_id, max_conn, client_max_body_size,
            advanced_config, enabled)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
         RETURNING id",
        serde_json::to_value(&domains)?, body.scheme, serde_json::to_value(&body.targets)?,
        body.load_balance, serde_json::to_value(&body.locations)?, body.certificate_id,
        body.ssl_enabled, body.force_ssl, body.http2, body.http3, body.hsts_disabled,
        body.hardening, body.block_common_exploits, body.access_list_id, body.max_conn,
        body.client_max_body_size, body.advanced_config, body.enabled
    ).fetch_one(&state.db).await?;

    finish(&state, &user, peer, "host_create", id).await
}

pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    Json(body): Json<HostInput>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require_write()?;
    let domains = validate_input::domains(&body.domains)?;
    validate_input::advanced(&body.advanced_config)?;

    let affected = sqlx::query!(
        "UPDATE proxy_hosts SET
           domains=$2, scheme=$3, targets=$4, load_balance=$5, locations=$6,
           certificate_id=$7, ssl_enabled=$8, force_ssl=$9, http2=$10, http3=$11,
           hsts_disabled=$12, hardening=$13, block_common_exploits=$14,
           access_list_id=$15, max_conn=$16, client_max_body_size=$17,
           advanced_config=$18, enabled=$19
         WHERE id=$1",
        id, serde_json::to_value(&domains)?, body.scheme, serde_json::to_value(&body.targets)?,
        body.load_balance, serde_json::to_value(&body.locations)?, body.certificate_id,
        body.ssl_enabled, body.force_ssl, body.http2, body.http3, body.hsts_disabled,
        body.hardening, body.block_common_exploits, body.access_list_id, body.max_conn,
        body.client_max_body_size, body.advanced_config, body.enabled
    ).execute(&state.db).await?.rows_affected();

    if affected == 0 { return Err(ApiError::NotFound); }
    finish(&state, &user, peer, "host_update", id).await
}

pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require_write()?;
    sqlx::query!("DELETE FROM proxy_hosts WHERE id = $1", id).execute(&state.db).await?;
    finish(&state, &user, peer, "host_delete", id).await
}

pub async fn toggle(
    State(state): State<AppState>,
    user: CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require_write()?;
    sqlx::query!("UPDATE proxy_hosts SET enabled = NOT enabled WHERE id = $1", id)
        .execute(&state.db).await?;
    finish(&state, &user, peer, "host_toggle", id).await
}

/// Setiap perubahan host langsung diterapkan. Kalau supervisor menolak
/// (misalnya `nginx -t` gagal), perubahan database di-rollback juga —
/// database dan berkas di disk tidak boleh berbeda.
async fn finish(
    state: &AppState, user: &CurrentUser, peer: SocketAddr, action: &str, id: i64,
) -> ApiResult<Json<serde_json::Value>> {
    match snapshot::apply(state, user.id).await {
        Ok(generation_id) => {
            crate::routes::auth_routes::audit(
                state, Some(user.id), peer, action, Some(&id.to_string()), None
            ).await;
            Ok(Json(serde_json::json!({ "id": id, "generation_id": generation_id })))
        }
        Err(e) => {
            crate::routes::auth_routes::audit(
                state, Some(user.id), peer, &format!("{action}_rejected"),
                Some(&id.to_string()), Some(serde_json::json!({ "error": e.to_string() }))
            ).await;
            Err(e)
        }
    }
}
