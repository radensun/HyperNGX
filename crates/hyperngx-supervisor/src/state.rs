use crate::config::Config;
use crate::ipc::{Response, Snapshot};
use anyhow::Result;
use hyperngx_core::apply::Applier;
use hyperngx_core::render::Renderer;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct SupervisorState {
    pub cfg: Arc<Config>,
    inner: Arc<Inner>,
}

struct Inner {
    /// Semua operasi yang menyentuh direktori konfigurasi diserialisasi.
    /// Dua `apply` bersamaan bisa menukar symlink dalam urutan yang salah
    /// sehingga generation lama menang — mutex ini mencegahnya.
    apply_lock: Mutex<()>,
    applier: Applier,
    templates: String,
}

impl SupervisorState {
    pub fn new(cfg: Config) -> Self {
        let applier = Applier {
            root: cfg.nginx_root.clone(),
            nginx_bin: cfg.nginx_bin.clone(),
            keep_generations: cfg.keep_generations,
        };
        let templates = cfg.template_dir.to_string_lossy().into_owned();
        Self {
            cfg: Arc::new(cfg),
            inner: Arc::new(Inner { apply_lock: Mutex::new(()), applier, templates }),
        }
    }

    fn renderer(&self) -> Renderer<'_> {
        Renderer::new(&self.inner.templates)
    }

    /// Render + uji + aktifkan + reload, dengan rollback otomatis.
    pub async fn apply_config(&self, generation_id: &str, snap: &Snapshot) -> Result<Response> {
        let _guard = self.inner.apply_lock.lock().await;
        let globals = serde_json::Value::Object(snap.globals.clone());
        let bundle = self.renderer().render_all(generation_id, &snap.hosts, &globals)?;

        match self.inner.applier.apply(generation_id, &bundle) {
            Ok(dir) => {
                tracing::info!(generation = generation_id, "konfigurasi aktif");
                Ok(Response::ok_gen(generation_id, serde_json::json!({
                    "path": dir.display().to_string(),
                    "hosts": snap.hosts.iter().filter(|h| h.enabled).count(),
                })))
            }
            Err(e) => Ok(Response::err("apply_failed", e)),
        }
    }

    /// Sama seperti apply, berhenti setelah `nginx -t`. Tidak menukar symlink.
    pub async fn dry_run(&self, snap: &Snapshot) -> Result<Response> {
        let _guard = self.inner.apply_lock.lock().await;
        let id = format!("dryrun-{}", ulid::Ulid::new());
        let globals = serde_json::Value::Object(snap.globals.clone());

        let bundle = match self.renderer().render_all(&id, &snap.hosts, &globals) {
            Ok(b) => b,
            Err(e) => return Ok(Response::err("render_failed", e)),
        };
        let staged = self.inner.applier.stage(&id, &bundle)?;
        let verdict = self.inner.applier.test(&staged);
        let _ = std::fs::remove_dir_all(&staged);   // dry run tidak meninggalkan jejak

        Ok(match verdict {
            Ok(()) => Response::ok(serde_json::json!({ "ok": true, "output": "Konfigurasi lolos nginx -t." })),
            Err(e) => Response::ok(serde_json::json!({ "ok": false, "output": e.to_string() })),
        })
    }

    pub async fn rollback(&self, target: Option<&str>) -> Result<Response> {
        let _guard = self.inner.apply_lock.lock().await;
        let gens_dir = self.cfg.nginx_root.join("generations");
        let mut gens: Vec<_> = std::fs::read_dir(&gens_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        gens.sort();                                  // ULID terurut waktu

        let active = std::fs::read_link(self.cfg.nginx_root.join("active")).ok();
        let target_dir = match target {
            Some(id) => gens_dir.join(id),
            None => {
                // satu langkah mundur dari yang aktif
                let idx = active.as_ref().and_then(|a| gens.iter().position(|g| g == a));
                match idx {
                    Some(i) if i > 0 => gens[i - 1].clone(),
                    _ => return Ok(Response::err("no_previous", "tidak ada generation sebelumnya")),
                }
            }
        };
        if !target_dir.is_dir() {
            return Ok(Response::err("not_found", format!("{} tidak ada", target_dir.display())));
        }

        self.inner.applier.test(&target_dir)?;        // generation lama pun tetap diuji
        self.inner.applier.activate(&target_dir)?;
        self.inner.applier.reload()?;

        let id = target_dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
        tracing::warn!(generation = %id, "rollback dijalankan");
        Ok(Response::ok_gen(id, serde_json::json!({ "rolled_back": true })))
    }

    pub async fn status(&self) -> Result<Response> {
        let active = std::fs::read_link(self.cfg.nginx_root.join("active"))
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()));
        let running = std::path::Path::new("/run/hyperngx/nginx.pid").exists();
        let certs = crate::certs::inventory(&self.cfg.tls_dir).unwrap_or_default();

        Ok(Response::ok(serde_json::json!({
            "nginx_running": running,
            "active_generation": active,
            "certificates": certs,
        })))
    }

    pub async fn request_cert(
        &self,
        slug: &str,
        domains: &[String],
        challenge: &hyperngx_acme::Challenge,
    ) -> Result<Response> {
        let req = hyperngx_acme::CertRequest {
            slug: slug.to_string(),
            domains: domains.to_vec(),
            challenge: challenge.clone(),
            key_type: hyperngx_acme::KeyType::Ecdsa256,
            must_staple: false,
        };
        match crate::certs::issue(self, &req).await {
            Ok(info) => {
                self.reload_only().await?;
                Ok(Response::ok(serde_json::to_value(info)?))
            }
            Err(e) => Ok(Response::err("acme_failed", e)),
        }
    }

    pub async fn revoke_cert(&self, slug: &str) -> Result<Response> {
        crate::certs::remove(&self.cfg.tls_dir, slug)?;
        Ok(Response::ok(serde_json::json!({ "removed": slug })))
    }

    pub async fn reload_only(&self) -> Result<()> {
        let _guard = self.inner.apply_lock.lock().await;
        self.inner.applier.reload()?;
        Ok(())
    }

    /// Scheduler perpanjangan.
    ///
    /// Seluruh sertifikat yang jatuh tempo diproses dulu, baru nginx
    /// di-reload SATU KALI. Pada instalasi dengan 500 domain, reload
    /// per sertifikat berarti 500 kali fork worker — cukup untuk
    /// menghabiskan memori server.
    pub async fn acme_renewal_loop(&self) -> Result<()> {
        let period = std::time::Duration::from_secs(self.cfg.acme.check_interval_hours * 3600);
        loop {
            tokio::time::sleep(period).await;
            match crate::certs::renew_due(self).await {
                Ok(0) => tracing::debug!("tidak ada sertifikat yang jatuh tempo"),
                Ok(n) => {
                    tracing::info!(count = n, "sertifikat diperbarui, reload sekali");
                    if let Err(e) = self.reload_only().await {
                        tracing::error!(error = %e, "reload setelah perpanjangan gagal");
                    }
                }
                Err(e) => tracing::error!(error = %e, "siklus perpanjangan gagal"),
            }
        }
    }
}
