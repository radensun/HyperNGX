use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::supervisor_client::SupervisorResponse;
use crate::snapshot;
use axum::extract::{ConnectInfo, Query, State};
use axum::Json;
use serde::Deserialize;
use std::net::SocketAddr;

pub async fn dry_run(
    State(state): State<AppState>, user: CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    user.require_write()?;
    let snap = snapshot::build(&state).await?;
    let cmd = serde_json::json!({ "op": "dry_run", "snapshot": snap });
    let resp: SupervisorResponse = state.supervisor.send(&cmd).await.map_err(ApiError::Other)?;
    let (_, detail) = resp.into_result()?;
    Ok(Json(detail))
}

#[derive(Deserialize)]
pub struct RollbackBody { generation_id: Option<String> }

pub async fn rollback(
    State(state): State<AppState>,
    user: CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<RollbackBody>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require_owner()?;
    let cmd = serde_json::json!({ "op": "rollback", "generation_id": body.generation_id });
    let resp: SupervisorResponse = state.supervisor.send(&cmd).await.map_err(ApiError::Other)?;
    let (generation_id, detail) = resp.into_result()?;

    if let Some(id) = &generation_id {
        let mut tx = state.db.begin().await?;
        sqlx::query!("UPDATE generations SET status = 'rolled_back' WHERE status = 'active'")
            .execute(&mut *tx).await?;
        sqlx::query!("UPDATE generations SET status = 'active' WHERE id = $1", id)
            .execute(&mut *tx).await?;
        tx.commit().await?;
    }
    crate::routes::auth_routes::audit(
        &state, Some(user.id), peer, "rollback", generation_id.as_deref(), None
    ).await;
    Ok(Json(detail))
}

pub async fn generations(
    State(state): State<AppState>, _u: CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    // snapshot sengaja tidak ikut: ukurannya bisa ratusan KB dan tidak
    // dibutuhkan untuk daftar riwayat.
    let rows = sqlx::query!(
        "SELECT g.id, g.applied_at, g.status, g.nginx_test, u.username
         FROM generations g LEFT JOIN users u ON u.id = g.applied_by
         ORDER BY g.applied_at DESC LIMIT 50"
    ).fetch_all(&state.db).await?;

    Ok(Json(serde_json::json!(rows.into_iter().map(|r| serde_json::json!({
        "id": r.id, "applied_at": r.applied_at, "status": r.status,
        "nginx_test": r.nginx_test, "by": r.username,
    })).collect::<Vec<_>>())))
}

#[derive(Deserialize)]
pub struct AuditQuery { #[serde(default = "d_limit")] limit: i64 }
fn d_limit() -> i64 { 100 }

pub async fn audit(
    State(state): State<AppState>, _u: CurrentUser, Query(q): Query<AuditQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = sqlx::query!(
        "SELECT a.at, a.action, a.entity_id, a.ip, a.diff, u.username
         FROM audit_log a LEFT JOIN users u ON u.id = a.user_id
         ORDER BY a.at DESC LIMIT $1",
        q.limit.clamp(1, 500)
    ).fetch_all(&state.db).await?;

    Ok(Json(serde_json::json!(rows.into_iter().map(|r| serde_json::json!({
        "at": r.at, "action": r.action, "entity_id": r.entity_id,
        "ip": r.ip.map(|i| i.to_string()), "by": r.username, "diff": r.diff,
    })).collect::<Vec<_>>())))
}

pub async fn status(
    State(state): State<AppState>, _u: CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    let cmd = serde_json::json!({ "op": "status" });
    let resp: SupervisorResponse = state.supervisor.send(&cmd).await.map_err(ApiError::Other)?;
    let (_, detail) = resp.into_result()?;
    Ok(Json(detail))
}
