# Model ancaman & pengetatan

## Aset yang dilindungi

1. Private key TLS seluruh domain (`/etc/hyperngx/tls/live/*/privkey.pem`)
2. Kunci akun ACME — pencurinya bisa menerbitkan sertifikat atas nama Anda
3. Panel admin — kompromi = kendali routing seluruh trafik
4. Trafik yang lewat (integritas & kerahasiaan)

## Ancaman dan mitigasinya

| Ancaman | Mitigasi |
|---|---|
| RCE lewat panel admin | Proses API tanpa kapabilitas, `PrivateUsers=yes`, tidak punya akses tulis ke `/etc/hyperngx/tls`; eskalasi ke supervisor hanya lewat enum tertutup |
| Injeksi direktif nginx via "advanced config" | `validate::scan_advanced()` menolak `load_module`, `perl`, `lua_`, `root /etc`, dan kurung tak seimbang; domain dinormalisasi lewat IDNA sehingga `;` dan spasi tidak bisa lolos |
| Kredensial admin ditebak | Argon2id (64 MiB, t=3, p=4), rate limit 5/15 menit per IP+username, TOTP wajib untuk role owner |
| Pencurian sesi | Token opaque 256-bit di DB (bisa dicabut seketika), cookie HttpOnly + Secure + SameSite=Strict, CSRF double-submit |
| Panel terekspos internet | Panel di port terpisah dengan access list; disarankan hanya via VPN/WireGuard |
| Slowloris / R-U-Dead-Yet | `client_header_timeout 10s`, `client_body_timeout 15s`, `reset_timedout_connection on`, `limit_conn perip 100` |
| SYN flood | `tcp_syncookies=1`, backlog 65535, rate limit nftables + set `blocklist` berbatas waktu |
| Host header attack / scanning IP | `default_server` mengembalikan `444` untuk Host tak dikenal |
| Kebocoran identitas backend | `server_tokens off`, `more_clear_headers Server`, `proxy_hide_header X-Powered-By` |
| Forward secrecy hilang karena ticket key statis | Rotasi otomatis 12 jam (current/previous) |
| Downgrade TLS | Hanya TLS 1.2/1.3, HSTS `preload`, cipher suite eksplisit |
| Replay 0-RTT | `ssl_early_data off` |
| Kompromi supply chain saat build | `build-nginx.sh` memverifikasi tanda tangan rilis nginx; dependensi Rust dikunci `Cargo.lock` + `cargo audit` di CI |
| Admin nakal / kesalahan operasi | `audit_log` mencatat siapa-kapan-diff; setiap generation menyimpan snapshot penuh untuk rollback |

## Yang sengaja TIDAK dilakukan

* **Tidak ada eksekusi shell dari input UI.** Semua pemanggilan proses memakai
  `Command` dengan argumen terpisah, tidak pernah `sh -c`.
* **Tidak ada mode "run as root" untuk kemudahan.** Kalau sebuah fitur butuh
  root, fitur itu pindah ke supervisor, bukan privilege-nya yang dinaikkan.
* **Tidak ada auto-update yang menarik biner dari internet.** Pembaruan lewat
  paket yang ditandatangani.

## Kepatuhan operasional

Log akses berformat JSON memuat IP klien. Bila HyperNGX memproses data
pribadi, terapkan retensi log yang terbatas (`logrotate` 14 hari) dan, bila
IP penuh tidak diperlukan untuk investigasi, anonimkan oktet terakhir lewat
`map` sebelum ditulis. Ini relevan untuk UU PDP dan GDPR.
