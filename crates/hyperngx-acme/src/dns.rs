//! Plugin DNS-01. Hanya dipakai untuk sertifikat wildcard.

use crate::DnsProvider;
use anyhow::{Context, Result};

/// Membaca rahasia dari berkas, bukan dari argumen. Token API DNS tidak
/// pernah melewati IPC atau tersimpan di database — yang dikirim hanyalah
/// nama berkasnya (`api_token_ref`).
fn read_secret(reference: &str) -> Result<String> {
    anyhow::ensure!(
        !reference.contains('/') && !reference.contains(".."),
        "referensi rahasia tidak boleh mengandung path"
    );
    let path = std::path::Path::new("/etc/hyperngx/secrets").join(reference);
    Ok(std::fs::read_to_string(&path)
        .with_context(|| format!("rahasia {} tidak terbaca", path.display()))?
        .trim()
        .to_string())
}

fn record_name(domain: &str) -> String {
    format!("_acme-challenge.{}", domain.trim_start_matches("*."))
}

pub async fn publish(provider: &DnsProvider, domain: &str, digest: &str) -> Result<()> {
    match provider {
        DnsProvider::Cloudflare { api_token_ref } => {
            cloudflare_upsert(&read_secret(api_token_ref)?, &record_name(domain), digest).await
        }
        DnsProvider::Rfc2136 { .. } => {
            anyhow::bail!("provider RFC2136 belum diimplementasikan (Fase 1)")
        }
        DnsProvider::Manual => {
            tracing::warn!(
                record = %record_name(domain),
                value = %digest,
                "mode manual: buat TXT record ini lalu jalankan ulang penerbitan"
            );
            anyhow::bail!("mode manual memerlukan tindakan operator")
        }
    }
}

pub async fn cleanup(provider: &DnsProvider, domain: &str) -> Result<()> {
    if let DnsProvider::Cloudflare { api_token_ref } = provider {
        return cloudflare_delete(&read_secret(api_token_ref)?, &record_name(domain)).await;
    }
    Ok(())
}

async fn cloudflare_upsert(token: &str, name: &str, value: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let zone = cloudflare_zone_id(&client, token, name).await?;

    // TTL 60 detik: cukup pendek supaya percobaan ulang tidak menunggu
    // cache resolver lama kedaluwarsa.
    let res = client
        .post(format!("https://api.cloudflare.com/client/v4/zones/{zone}/dns_records"))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "type": "TXT", "name": name, "content": value, "ttl": 60
        }))
        .send()
        .await?;

    anyhow::ensure!(res.status().is_success(), "Cloudflare menolak TXT record: {}", res.status());
    Ok(())
}

async fn cloudflare_delete(token: &str, name: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let zone = cloudflare_zone_id(&client, token, name).await?;
    let list: serde_json::Value = client
        .get(format!("https://api.cloudflare.com/client/v4/zones/{zone}/dns_records"))
        .query(&[("type", "TXT"), ("name", name)])
        .bearer_auth(token)
        .send().await?.json().await?;

    for rec in list["result"].as_array().unwrap_or(&vec![]) {
        if let Some(id) = rec["id"].as_str() {
            let _ = client
                .delete(format!("https://api.cloudflare.com/client/v4/zones/{zone}/dns_records/{id}"))
                .bearer_auth(token).send().await;
        }
    }
    Ok(())
}

async fn cloudflare_zone_id(client: &reqwest::Client, token: &str, name: &str) -> Result<String> {
    // Cari zona terpanjang yang cocok: `_acme-challenge.a.b.co.id` bisa
    // berada di zona `b.co.id` maupun `a.b.co.id`.
    let labels: Vec<&str> = name.split('.').collect();
    for i in 1..labels.len() {
        let candidate = labels[i..].join(".");
        let res: serde_json::Value = client
            .get("https://api.cloudflare.com/client/v4/zones")
            .query(&[("name", candidate.as_str())])
            .bearer_auth(token)
            .send().await?.json().await?;
        if let Some(id) = res["result"][0]["id"].as_str() {
            return Ok(id.to_string());
        }
    }
    anyhow::bail!("zona Cloudflare untuk {name} tidak ditemukan")
}
