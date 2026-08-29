//! Klien ACME (Let's Encrypt) bawaan — tanpa certbot, tanpa Python.
//!
//! Dukungan:
//!   * HTTP-01 : token ditulis ke /var/lib/hyperngx/acme (dilayani nginx)
//!   * DNS-01  : untuk sertifikat wildcard (*.domain.tld) via provider plugin
//!
//! Kunci akun dan private key sertifikat ditulis oleh supervisor dengan
//! mode 0600 milik root. Proses API tidak pernah menyentuhnya.

pub mod dns;
pub mod order;
pub mod renewal;
pub mod store;

use serde::{Deserialize, Serialize};

pub const LE_PRODUCTION: &str = "https://acme-v02.api.letsencrypt.org/directory";
pub const LE_STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Challenge {
    Http01,
    Dns01 { provider: DnsProvider },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DnsProvider {
    Cloudflare { api_token_ref: String },
    Rfc2136 { server: String, key_ref: String },
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyType { Ecdsa256, Ecdsa384, Rsa2048 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertRequest {
    pub slug: String,
    pub domains: Vec<String>,
    pub challenge: Challenge,
    pub key_type: KeyType,
    #[serde(default)]
    pub must_staple: bool,
}

impl CertRequest {
    /// Wildcard hanya bisa lewat DNS-01 — CA menolak HTTP-01 untuk `*.x`.
    /// Diperiksa di sini supaya kegagalan muncul saat menyimpan di UI,
    /// bukan setelah menabrak rate limit CA.
    pub fn validate(&self) -> anyhow::Result<()> {
        let wildcard = self.domains.iter().any(|d| d.starts_with("*."));
        if wildcard && matches!(self.challenge, Challenge::Http01) {
            anyhow::bail!("sertifikat wildcard memerlukan DNS-01, bukan HTTP-01");
        }
        if self.domains.is_empty() {
            anyhow::bail!("daftar domain kosong");
        }
        if self.domains.len() > 100 {
            anyhow::bail!("Let's Encrypt membatasi 100 nama per sertifikat");
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct IssuedCert {
    pub slug: String,
    pub fullchain_pem: String,
    pub private_key_pem: String,
    pub not_after: chrono::DateTime<chrono::Utc>,
}
