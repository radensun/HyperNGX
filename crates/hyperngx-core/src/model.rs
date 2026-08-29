use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalance { RoundRobin, LeastConn, IpHash }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub address: String,
    pub port: u16,
    #[serde(default = "one")]
    pub weight: u32,
    #[serde(default = "three")]
    pub max_fails: u32,
    #[serde(default = "ten")]
    pub fail_timeout: u32,
    #[serde(default)]
    pub backup: bool,
}
fn one() -> u32 { 1 }
fn three() -> u32 { 3 }
fn ten() -> u32 { 10 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationRule {
    pub path: String,
    pub scheme: String,          // http | https
    pub upstream: String,        // host:port
    #[serde(default)] pub websocket: bool,
    #[serde(default)] pub cache_enabled: bool,
    #[serde(default = "ttl")] pub cache_ttl: String,
    #[serde(default)] pub extra_directives: String,
}
fn ttl() -> String { "10m".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyHost {
    pub id: i64,
    pub domains: Vec<String>,
    pub scheme: String,
    pub targets: Vec<Target>,
    pub load_balance: LoadBalance,
    pub locations: Vec<LocationRule>,

    // TLS
    pub ssl_enabled: bool,
    pub force_ssl: bool,
    pub http2: bool,
    pub http3: bool,
    pub hsts_disabled: bool,
    pub cert_slug: String,

    // Proteksi
    pub hardening: bool,
    pub block_common_exploits: bool,
    pub access_list_id: Option<i64>,
    pub max_conn: u32,
    pub client_max_body_size: String,

    /// Blok mentah dari admin. Selalu melewati validate::scan_advanced().
    #[serde(default)]
    pub advanced_config: String,
    pub enabled: bool,
}
