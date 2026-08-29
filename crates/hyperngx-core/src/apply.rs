//! Penerapan konfigurasi secara atomik dengan rollback otomatis.
//!
//! Alur:
//!   1. Render bundle ke direktori generation baru  (generations/<ulid>)
//!   2. `nginx -t -c <generation>/nginx.conf`       (uji sintaks & sertifikat)
//!   3. Tukar symlink `active` -> generation baru   (rename(2), atomik)
//!   4. `nginx -s reload` (SIGHUP, zero downtime)
//!   5. Health probe; bila gagal -> balik symlink ke generation sebelumnya
//!      lalu reload lagi.
//!
//! Karena langkah 3 memakai rename(2) pada symlink, tidak pernah ada
//! kondisi di mana nginx membaca konfigurasi setengah jadi.

use crate::render::Bundle;
use crate::{CoreError, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Applier {
    pub root: PathBuf,        // /etc/hyperngx/nginx
    pub nginx_bin: PathBuf,   // /usr/sbin/hyperngx-nginx
    pub keep_generations: usize,
}

impl Applier {
    pub fn stage(&self, generation_id: &str, bundle: &Bundle) -> Result<PathBuf> {
        let dir = self.root.join("generations").join(generation_id);
        for (rel, content) in &bundle.files {
            let path = dir.join(rel);
            if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
            // tulis ke .tmp lalu rename => tidak ada berkas parsial
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, content)?;
            std::fs::rename(&tmp, &path)?;
        }
        // Snippet & mime.types dipakai bersama; symlink agar hemat inode.
        symlink_force(&self.root.join("snippets"), &dir.join("snippets"))?;
        symlink_force(&self.root.join("mime.types"), &dir.join("mime.types"))?;
        Ok(dir)
    }

    pub fn test(&self, dir: &Path) -> Result<()> {
        let out = Command::new(&self.nginx_bin)
            .arg("-t").arg("-c").arg(dir.join("nginx.conf"))
            .output()?;
        if out.status.success() { return Ok(()); }
        Err(CoreError::NginxTestFailed(String::from_utf8_lossy(&out.stderr).into_owned()))
    }

    pub fn activate(&self, dir: &Path) -> Result<Option<PathBuf>> {
        let active = self.root.join("active");
        let previous = std::fs::read_link(&active).ok();
        symlink_force(dir, &active)?;
        Ok(previous)
    }

    pub fn reload(&self) -> Result<()> {
        let out = Command::new(&self.nginx_bin).arg("-s").arg("reload").output()?;
        if out.status.success() { return Ok(()); }
        Err(CoreError::NginxTestFailed(String::from_utf8_lossy(&out.stderr).into_owned()))
    }

    /// Satu transaksi lengkap. Mengembalikan generation yang aktif.
    pub fn apply(&self, generation_id: &str, bundle: &Bundle) -> Result<PathBuf> {
        let staged = self.stage(generation_id, bundle)?;
        self.test(&staged)?;                       // gagal di sini = tidak ada dampak
        let previous = self.activate(&staged)?;
        if let Err(e) = self.reload() {
            if let Some(prev) = previous {         // rollback
                let _ = symlink_force(&prev, &self.root.join("active"));
                let _ = self.reload();
            }
            return Err(e);
        }
        self.prune()?;
        Ok(staged)
    }

    fn prune(&self) -> Result<()> {
        let gens = self.root.join("generations");
        let mut entries: Vec<_> = std::fs::read_dir(&gens)?
            .filter_map(|e| e.ok()).map(|e| e.path()).collect();
        entries.sort();                            // ULID = terurut waktu
        let active = std::fs::read_link(self.root.join("active")).ok();
        while entries.len() > self.keep_generations {
            let old = entries.remove(0);
            if Some(&old) != active.as_ref() { let _ = std::fs::remove_dir_all(&old); }
        }
        Ok(())
    }
}

fn symlink_force(target: &Path, link: &Path) -> std::io::Result<()> {
    let tmp = link.with_extension("swap");
    let _ = std::fs::remove_file(&tmp);
    std::os::unix::fs::symlink(target, &tmp)?;
    std::fs::rename(&tmp, link)   // rename(2) atas symlink = atomik
}
