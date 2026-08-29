# HyperNGX

Nginx Proxy Manager yang di-*compile* jadi satu appliance: **control plane Rust**,
**UI Svelte**, **data plane nginx** yang dibangun sendiri dengan HTTP/3, Brotli,
dan Let's Encrypt bawaan. Target: 10.000 concurrent connection per server,
Debian 13 (trixie).

## Kenapa arsitekturnya begini

Nginx Proxy Manager yang ada sekarang punya tiga masalah struktural yang
ingin dihindari HyperNGX:

1. **Satu proses melakukan segalanya.** Panel admin, penulisan config, dan
   penulisan private key jalan dalam satu proses Node yang sama. Satu bug
   deserialisasi di panel = akses ke seluruh kunci TLS.
2. **Config ditulis langsung ke direktori aktif.** Kalau render menghasilkan
   config rusak, nginx sudah terlanjur diminta reload dan proxy mati.
3. **Tuning default.** `worker_connections 768` bawaan Debian menabrak plafon
   jauh sebelum 10.000 klien.

HyperNGX menjawab ketiganya dengan pemisahan proses, penerapan atomik, dan
profil tuning yang diturunkan dari hitungan kapasitas — bukan tebakan.

## Tiga proses, tiga tingkat privilege

```
             :443/:80                          :8443 (admin, di-ACL)
                 │                                   │
        ┌────────▼─────────┐                ┌────────▼─────────┐
        │  nginx           │                │  hyperngx-api    │
        │  user: hyperngx  │                │  user: hyperngx-api
        │  data plane      │                │  TANPA kapabilitas
        └────────┬─────────┘                └────────┬─────────┘
                 │ baca config                       │ unix socket 0660
                 │                                   │ (SO_PEERCRED diperiksa)
        ┌────────▼───────────────────────────────────▼─────────┐
        │  hyperngx-supervisor        (root, kapabilitas dipangkas)
        │  · render + nginx -t + swap symlink + reload
        │  · tulis privkey 0600, rotasi ticket key
        │  · siklus ACME
        └──────────────────────────────────────────────────────┘
```

Proses yang menghadap admin **tidak pernah** menulis berkas nginx dan tidak
pernah menyentuh private key. Ia hanya mengirim `Command` bertipe lewat unix
socket. Permukaan serangan supervisor adalah satu `enum` tertutup
(`crates/hyperngx-supervisor/src/ipc.rs`), bukan string konfigurasi bebas.

## Penerapan konfigurasi yang atomik

`crates/hyperngx-core/src/apply.rs` — setiap perubahan jadi satu *generation*:

1. render bundle ke `generations/<ULID>/`
2. `nginx -t -c generations/<ULID>/nginx.conf` — gagal di sini berarti nol dampak
3. `rename(2)` symlink `active` → generation baru (atomik di level kernel)
4. `nginx -s reload` (SIGHUP, zero downtime)
5. gagal? symlink dibalik ke generation sebelumnya, reload lagi

Konsekuensi praktis: **tidak ada kondisi di mana nginx membaca konfigurasi
setengah jadi**, dan tombol "rollback" di UI hanya menukar symlink — bukan
menjalankan ulang migrasi.

## Isi repositori

| Path | Isi |
|---|---|
| `crates/hyperngx-core` | model domain, validasi anti-injeksi, renderer, mesin apply |
| `crates/hyperngx-acme` | klien ACME (HTTP-01, DNS-01 wildcard), kebijakan perpanjangan |
| `crates/hyperngx-supervisor` | daemon privileged, IPC, rotasi ticket key |
| `crates/hyperngx-api` | REST API + penyaji SPA, auth Argon2id + TOTP |
| `templates/` | template nginx.conf, proxy host, stream, snippet keamanan |
| `packaging/` | build nginx, installer, sysctl, limits, unit systemd |
| `web/` | SvelteKit 5 SPA (adapter-static) |
| `migrations/` | skema PostgreSQL |
| `docs/` | deployment, kapasitas, model ancaman, roadmap |

## Instalasi (Debian 13)

```bash
sudo ./packaging/build-nginx.sh     # nginx dengan HTTP/3 + Brotli + headers-more
cargo build --release               # control plane
(cd web && npm ci && npm run build)
sudo ./packaging/install.sh
sudo systemctl enable --now hyperngx-supervisor hyperngx-nginx hyperngx-api
```

## Status: MVP

Alur ujung-ke-ujung sudah utuh — login, terbitkan sertifikat, buat jalur,
terapkan, kembalikan konfigurasi. Tidak ada lagi `todo!()` di jalur utama.

| Kemampuan | Status |
|---|---|
| Login Argon2id + TOTP, sesi yang bisa dicabut, CSRF | selesai |
| CRUD proxy host + penerapan atomik dengan rollback | selesai |
| ACME HTTP-01 dan DNS-01 (Cloudflare) + perpanjangan otomatis | selesai |
| Riwayat generation + rollback satu klik | selesai |
| Audit log | selesai |
| Stream (L4), redirect host, access list | skema siap, UI belum |
| DNS-01 RFC2136, metrik Prometheus, health check aktif | Fase 1–2 |

**Belum dikompilasi.** Kode ditulis tanpa akses ke toolchain Rust, jadi
`cargo build` pertama hampir pasti memunculkan perbaikan tipe — terutama
di sekitar sqlx (yang memverifikasi query ke database saat kompilasi) dan
API `instant-acme`. Arsitektur, SQL, konfigurasi nginx, dan alur logikanya
yang perlu ditinjau; error kompilasi akan menunjukkan dirinya sendiri.

Panduan deploy lengkap: `docs/DEPLOYMENT.md`.
