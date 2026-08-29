use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "d_socket")]   pub socket_path: PathBuf,
    #[serde(default = "d_nginx_root")] pub nginx_root: PathBuf,
    #[serde(default = "d_nginx_bin")] pub nginx_bin: PathBuf,
    #[serde(default = "d_templates")] pub template_dir: PathBuf,
    #[serde(default = "d_tls")]      pub tls_dir: PathBuf,
    #[serde(default = "d_acme_webroot")] pub acme_webroot: PathBuf,
    #[serde(default = "d_keep")]     pub keep_generations: usize,
    #[serde(default = "d_peer")]     pub allowed_peer_user: String,
    #[serde(default)]                pub acme: AcmeConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AcmeConfig {
    #[serde(default = "d_directory")] pub directory: String,
    #[serde(default)]                 pub contact_email: Option<String>,
    /// Interval bangun scheduler perpanjangan, dalam jam.
    #[serde(default = "d_interval")]  pub check_interval_hours: u64,
    /// Rotasi TLS session ticket key, dalam jam.
    #[serde(default = "d_ticket")]    pub ticket_rotate_hours: u64,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            directory: d_directory(),
            contact_email: None,
            check_interval_hours: d_interval(),
            ticket_rotate_hours: d_ticket(),
        }
    }
}

fn d_socket() -> PathBuf { "/run/hyperngx/supervisor.sock".into() }
fn d_nginx_root() -> PathBuf { "/etc/hyperngx/nginx".into() }
fn d_nginx_bin() -> PathBuf { "/usr/sbin/hyperngx-nginx".into() }
fn d_templates() -> PathBuf { "/usr/share/hyperngx/templates".into() }
fn d_tls() -> PathBuf { "/etc/hyperngx/tls".into() }
fn d_acme_webroot() -> PathBuf { "/var/lib/hyperngx/acme".into() }
fn d_keep() -> usize { 20 }
fn d_peer() -> String { "hyperngx-api".into() }
fn d_directory() -> String { hyperngx_acme::LE_PRODUCTION.to_string() }
fn d_interval() -> u64 { 12 }
fn d_ticket() -> u64 { 12 }

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("gagal membaca {path}: {e}"))?;
        Ok(toml::from_str(&raw)?)
    }
}
