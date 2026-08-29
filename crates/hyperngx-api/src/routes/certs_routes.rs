use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::supervisor_client::SupervisorResponse;
use axum::extract::{ConnectInfo, Path, State};
use axum::Json;
use serde::Deserialize;
use std::net::SocketAddr;

pub async fn list(State(state): State<AppState>, _u: CurrentUser) -> ApiResult<Json<serde_json::Value>> {
    let rows = sqlx::query!(
        "SELECT id, slug, domains, provider, not_after, last_error, renew_attempts
         FROM certificates ORDER BY not_after NULLS FIRST"
    ).fetch_all(&state.db).await?;

    let now = chrono::Utc::now();
    let out: Vec<_> = rows.into_iter().map(|r| serde_json::json!({
        "id": r.id, "slug": r.slug, "domains": r.domains, "provider": r.provider,
        "not_after": r.not_after,
        "days_left": r.not_after.map(|t| (t - now).num_days()),
        "last_error": r.last_error, "renew_attempts": r.renew_attempts,
    })).collect();
    Ok(Json(serde_json::json!(out)))
}

#[derive(Deserialize)]
pub struct CertInput {
    pub domains: Vec<String>,
    /// {"type":"http01"} atau {"type":"dns01","provider":{...}}
    pub challenge: hyperngx_acme::Challenge,
}

pub async fn request(
    State(state): State<AppState>,
    user: CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<CertInput>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require_write()?;
    let domains = crate::validate_input::domains(&body.domains)?;

    // Slug diturunkan dari domain pertama, bukan dari input bebas: ia jadi
    // nama direktori di /etc/hyperngx/tls/live.
    let slug = slugify(&domains[0]);

    let req = hyperngx_acme::CertRequest {
        slug: slug.clone(),
        domains: domains.clone(),
        challenge: body.challenge.clone(),
        key_type: hyperngx_acme::KeyType::Ecdsa256,
        must_staple: false,
    };
    // Wildcard + HTTP-01 ditolak di sini, sebelum menyentuh CA.
    req.validate().map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let cmd = serde_json::json!({
        "op": "request_cert", "slug": slug,
        "domains": domains, "challenge": body.challenge,
    });
    let resp: SupervisorResponse = state.supervisor.send(&cmd).await.map_err(ApiError::Other)?;
    let (_, detail) = resp.into_result()?;

    let not_after = detail.get("not_after").and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&chrono::Utc));

    let id: i64 = sqlx::query_scalar!(
        "INSERT INTO certificates (slug, domains, provider, challenge, not_after)
         VALUES ($1, $2, 'letsencrypt', $3, $4)
         ON CONFLICT (slug) DO UPDATE
           SET domains = EXCLUDED.domains, not_after = EXCLUDED.not_after,
               last_error = NULL, renew_attempts = 0
         RETURNING id",
        slug, serde_json::to_value(&domains)?, serde_json::to_value(&body.challenge)?, not_after
    ).fetch_one(&state.db).await?;

    crate::routes::auth_routes::audit(
        &state, Some(user.id), peer, "cert_issue", Some(&slug), None
    ).await;
    Ok(Json(serde_json::json!({ "id": id, "slug": slug, "not_after": not_after })))
}

pub async fn renew(
    State(state): State<AppState>,
    user: CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(slug): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require_write()?;
    let row = sqlx::query!(
        "SELECT domains, challenge FROM certificates WHERE slug = $1", slug
    ).fetch_optional(&state.db).await?.ok_or(ApiError::NotFound)?;

    let cmd = serde_json::json!({
        "op": "request_cert", "slug": slug,
        "domains": row.domains, "challenge": row.challenge,
    });
    let resp: SupervisorResponse = state.supervisor.send(&cmd).await.map_err(ApiError::Other)?;
    let (_, detail) = resp.into_result()?;

    crate::routes::auth_routes::audit(
        &state, Some(user.id), peer, "cert_renew", Some(&slug), None
    ).await;
    Ok(Json(detail))
}

pub async fn revoke(
    State(state): State<AppState>,
    user: CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(slug): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require_write()?;

    // Host yang masih memakainya akan kehilangan TLS dan `nginx -t` gagal.
    // Lebih baik ditolak sekarang dengan pesan jelas.
    let in_use: i64 = sqlx::query_scalar!(
        "SELECT count(*) FROM proxy_hosts h JOIN certificates c ON c.id = h.certificate_id
         WHERE c.slug = $1", slug
    ).fetch_one(&state.db).await?.unwrap_or(0);
    if in_use > 0 {
        return Err(ApiError::BadRequest(format!(
            "Sertifikat masih dipakai {in_use} host. Lepaskan dulu dari host tersebut."
        )));
    }

    let cmd = serde_json::json!({ "op": "revoke_cert", "slug": slug });
    let resp: SupervisorResponse = state.supervisor.send(&cmd).await.map_err(ApiError::Other)?;
    resp.into_result()?;

    sqlx::query!("DELETE FROM certificates WHERE slug = $1", slug)
        .execute(&state.db).await?;
    crate::routes::auth_routes::audit(
        &state, Some(user.id), peer, "cert_revoke", Some(&slug), None
    ).await;
    Ok(Json(serde_json::json!({ "removed": slug })))
}

fn slugify(domain: &str) -> String {
    domain.trim_start_matches("*.")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '-' })
        .collect()
}
