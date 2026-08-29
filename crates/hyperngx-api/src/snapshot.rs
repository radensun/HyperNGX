//! Membangun snapshot state untuk dikirim ke supervisor.
//!
//! Ini satu-satunya jalan konfigurasi mengalir ke nginx: API tidak pernah
//! menulis berkas, ia mengirim seluruh state dan supervisor yang merender.
//! Konsekuensinya, setiap perubahan menghasilkan konfigurasi utuh — bukan
//! tambalan — sehingga tidak ada drift antara database dan berkas di disk.

use crate::error::ApiResult;
use crate::state::AppState;
use hyperngx_core::model::{LoadBalance, LocationRule, ProxyHost, Target};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub hosts: Vec<ProxyHost>,
    pub globals: serde_json::Map<String, serde_json::Value>,
}

pub async fn build(state: &AppState) -> ApiResult<Snapshot> {
    let rows = sqlx::query!(
        r#"SELECT h.id, h.domains, h.scheme, h.targets, h.load_balance, h.locations,
                  h.ssl_enabled, h.force_ssl, h.http2, h.http3, h.hsts_disabled,
                  h.hardening, h.block_common_exploits, h.access_list_id,
                  h.max_conn, h.client_max_body_size, h.advanced_config, h.enabled,
                  c.slug as "cert_slug?"
           FROM proxy_hosts h
           LEFT JOIN certificates c ON c.id = h.certificate_id
           ORDER BY h.id"#
    ).fetch_all(&state.db).await?;

    let mut hosts = Vec::with_capacity(rows.len());
    for r in rows {
        hosts.push(ProxyHost {
            id: r.id,
            domains: serde_json::from_value(r.domains)?,
            scheme: r.scheme,
            targets: serde_json::from_value::<Vec<Target>>(r.targets)?,
            load_balance: match r.load_balance.as_str() {
                "least_conn" => LoadBalance::LeastConn,
                "ip_hash" => LoadBalance::IpHash,
                _ => LoadBalance::RoundRobin,
            },
            locations: serde_json::from_value::<Vec<LocationRule>>(r.locations)?,
            ssl_enabled: r.ssl_enabled,
            force_ssl: r.force_ssl,
            http2: r.http2,
            http3: r.http3,
            hsts_disabled: r.hsts_disabled,
            // Host tanpa sertifikat memakai sertifikat default; nginx tetap
            // butuh berkas yang ada, kalau tidak `nginx -t` gagal.
            cert_slug: r.cert_slug.unwrap_or_else(|| "default".into()),
            hardening: r.hardening,
            block_common_exploits: r.block_common_exploits,
            access_list_id: r.access_list_id,
            max_conn: r.max_conn as u32,
            client_max_body_size: r.client_max_body_size,
            advanced_config: r.advanced_config,
            enabled: r.enabled,
        });
    }

    let settings = sqlx::query!("SELECT key, value FROM settings")
        .fetch_all(&state.db).await?;
    let globals = settings.into_iter()
        .map(|s| (s.key, serde_json::Value::String(s.value)))
        .collect();

    Ok(Snapshot { hosts, globals })
}

/// Menerapkan snapshot saat ini, mencatat hasilnya sebagai generation.
pub async fn apply(state: &AppState, user_id: i64) -> ApiResult<String> {
    let snap = build(state).await?;
    let generation_id = ulid::Ulid::new().to_string();

    sqlx::query!(
        "INSERT INTO generations (id, applied_by, status, snapshot)
         VALUES ($1, $2, 'staged', $3)",
        generation_id, user_id, serde_json::to_value(&snap)?
    ).execute(&state.db).await?;

    let cmd = serde_json::json!({
        "op": "apply_config",
        "generation_id": generation_id,
        "snapshot": snap,
    });
    let response: crate::supervisor_client::SupervisorResponse =
        state.supervisor.send(&cmd).await.map_err(crate::error::ApiError::Other)?;

    match response.into_result() {
        Ok(_) => {
            // Hanya satu generation boleh berstatus active (dijaga index unik).
            let mut tx = state.db.begin().await?;
            sqlx::query!("UPDATE generations SET status = 'rolled_back' WHERE status = 'active'")
                .execute(&mut *tx).await?;
            sqlx::query!("UPDATE generations SET status = 'active' WHERE id = $1", generation_id)
                .execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(generation_id)
        }
        Err(e) => {
            sqlx::query!(
                "UPDATE generations SET status = 'failed', nginx_test = $2 WHERE id = $1",
                generation_id, e.to_string()
            ).execute(&state.db).await?;
            Err(e)
        }
    }
}
