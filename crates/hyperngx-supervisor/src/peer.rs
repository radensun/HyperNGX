//! Verifikasi identitas pemanggil unix socket.

use anyhow::{bail, Result};
use nix::unistd::{Gid, Group, User};
use tokio::net::UnixStream;

pub fn gid_of_group(name: &str) -> Option<Gid> {
    Group::from_name(name).ok().flatten().map(|g| g.gid)
}

/// Menolak koneksi yang bukan dari user yang diizinkan (atau root).
///
/// SO_PEERCRED tidak bisa dipalsukan dari userspace: nilainya diisi kernel
/// saat koneksi dibuat. Ini alasan HyperNGX tidak memerlukan token bersama
/// antara API dan supervisor.
pub fn verify(stream: &UnixStream, allowed_user: &str) -> Result<()> {
    let cred = stream.peer_cred()?;
    let uid = cred.uid();
    if uid == 0 { return Ok(()); }

    let expected = User::from_name(allowed_user)?
        .map(|u| u.uid.as_raw())
        .ok_or_else(|| anyhow::anyhow!("user {allowed_user} tidak ada"))?;

    if uid != expected {
        bail!("koneksi ditolak: uid {uid} bukan {allowed_user} ({expected})");
    }
    Ok(())
}
