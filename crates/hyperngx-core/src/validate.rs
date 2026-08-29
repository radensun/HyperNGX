use crate::{CoreError, Result};
use crate::model::ProxyHost;

/// Direktif yang tidak boleh muncul di "advanced config" milik admin.
/// Tanpa daftar ini, fitur advanced_config = eskalasi privilege:
/// `root /etc/shadow;` atau `perl_set` akan membocorkan berkas server.
const DENY: &[&str] = &[
    "load_module", "user ", "pid ", "daemon ", "master_process",
    "perl", "lua_", "content_by_lua", "js_import", "js_content",
    "root /etc", "root /root", "root /home", "alias /etc",
    "include /etc/shadow", "include /etc/passwd", "include /root",
    "error_log syslog", "dav_methods", "auth_basic_user_file /etc",
];

pub fn scan_advanced(raw: &str) -> Result<()> {
    let lower = raw.to_ascii_lowercase();
    for bad in DENY {
        if lower.contains(bad) {
            return Err(CoreError::UnsafeDirective((*bad).to_string()));
        }
    }
    // Blok mentah tidak boleh keluar dari server{} yang kita generate.
    let open = raw.matches('{').count();
    let close = raw.matches('}').count();
    if open != close {
        return Err(CoreError::UnsafeDirective("kurung kurawal tidak seimbang".into()));
    }
    Ok(())
}

/// Domain divalidasi lewat IDNA (punycode) supaya nama unicode tidak
/// menyelundupkan spasi/`;` ke dalam berkas konfigurasi.
pub fn normalize_domain(input: &str) -> Result<String> {
    let d = input.trim().trim_end_matches('.').to_ascii_lowercase();
    if d.is_empty() || d.len() > 253 {
        return Err(CoreError::InvalidDomain(input.into()));
    }
    let candidate = d.strip_prefix("*.").unwrap_or(&d);
    let ascii = idna::domain_to_ascii(candidate)
        .map_err(|_| CoreError::InvalidDomain(input.into()))?;
    if !ascii.split('.').all(|l| {
        !l.is_empty() && l.len() <= 63
            && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !l.starts_with('-') && !l.ends_with('-')
    }) {
        return Err(CoreError::InvalidDomain(input.into()));
    }
    Ok(if d.starts_with("*.") { format!("*.{ascii}") } else { ascii })
}

pub fn validate_host(h: &ProxyHost) -> Result<()> {
    if h.domains.is_empty() {
        return Err(CoreError::InvalidDomain("daftar domain kosong".into()));
    }
    for d in &h.domains { normalize_domain(d)?; }
    if h.targets.is_empty() {
        return Err(CoreError::InvalidUpstream("tidak ada target".into()));
    }
    for t in &h.targets {
        if t.address.contains(|c: char| c.is_whitespace() || c == ';' || c == '{' || c == '}') {
            return Err(CoreError::InvalidUpstream(t.address.clone()));
        }
    }
    scan_advanced(&h.advanced_config)?;
    for l in &h.locations { scan_advanced(&l.extra_directives)?; }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolak_injeksi_lewat_domain() {
        assert!(normalize_domain("a.com; root /etc").is_err());
    }
    #[test]
    fn terima_wildcard_dan_idn() {
        assert_eq!(normalize_domain("*.Sekolah.ID").unwrap(), "*.sekolah.id");
    }
    #[test]
    fn tolak_direktif_berbahaya() {
        assert!(scan_advanced("root /etc/shadow;").is_err());
    }
}
