use crate::model::ProxyHost;
use crate::{validate, Result};
use minijinja::{context, Environment};
use std::collections::BTreeMap;

pub struct Renderer<'a> { env: Environment<'a> }

#[derive(Debug, Default)]
pub struct Bundle {
    /// path relatif -> isi berkas
    pub files: BTreeMap<String, String>,
}

impl<'a> Renderer<'a> {
    pub fn new(template_dir: &str) -> Self {
        let mut env = Environment::new();
        env.set_loader(minijinja::path_loader(template_dir));
        // Autoescape dimatikan (bukan HTML); keamanan ditegakkan di validate.rs.
        env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
        Self { env }
    }

    /// Menghasilkan seluruh bundle konfigurasi untuk satu generation.
    pub fn render_all(
        &self,
        generation_id: &str,
        hosts: &[ProxyHost],
        globals: &serde_json::Value,
    ) -> Result<Bundle> {
        let mut bundle = Bundle::default();
        let now = chrono::Utc::now().to_rfc3339();

        let tmpl = self.env.get_template("nginx.conf.j2")?;
        bundle.files.insert(
            "nginx.conf".into(),
            tmpl.render(context! {
                generation_id, generated_at => now, globals => globals,
                worker_connections => globals.get("worker_connections"),
            })?,
        );

        let host_tmpl = self.env.get_template("proxy_host.conf.j2")?;
        for h in hosts.iter().filter(|h| h.enabled) {
            validate::validate_host(h)?;
            let rendered = host_tmpl.render(context! {
                host => h, generation_id,
            })?;
            bundle.files.insert(format!("hosts/{:06}.conf", h.id), rendered);
        }
        Ok(bundle)
    }
}
