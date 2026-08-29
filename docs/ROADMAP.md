# Roadmap

## Fase 0 — Fondasi (kerangka ini)
- [x] Pemisahan proses & unit systemd yang diperketat
- [x] Template nginx + snippet keamanan
- [x] Mesin apply atomik dengan rollback
- [x] Skema data & audit log
- [x] Profil tuning kernel Debian 13
- [x] Loop IPC supervisor + verifikasi SO_PEERCRED
- [x] Handler route Axum
- [x] Integrasi `instant-acme` (HTTP-01)

## Fase 1 — MVP dapat dipakai produksi
- [x] CRUD proxy host lewat UI
- [ ] CRUD redirect dan stream lewat UI
- [x] ACME DNS-01 Cloudflare untuk wildcard
- [ ] ACME DNS-01 RFC2136
- [ ] Access list: CIDR + basic auth
- [ ] Metrik Prometheus (`/metrics`) dari `stub_status` + log JSON
- [ ] Backup/restore konfigurasi (export satu berkas terenkripsi)
- [ ] Uji beban 10k terotomasi di CI

## Fase 2 — Operasional lanjut
- [ ] ModSecurity + OWASP CRS sebagai modul opsional
- [ ] Cache purge per-URL dari UI
- [ ] Health check aktif ke upstream + tampilan status
- [ ] SSO OIDC untuk panel admin
- [ ] Mode multi-node: satu klaster PostgreSQL dipakai bersama, supervisor jadi agent

## Keputusan yang sudah dikunci
1. **PostgreSQL sebagai penyimpan state, sejak versi pertama.** Konsekuensinya:
   instalasi satu server jadi sedikit lebih berat (butuh satu service tambahan),
   tetapi mode multi-node di Fase 2 tidak menuntut migrasi skema — hanya
   mengarahkan beberapa instance ke satu klaster yang sama. Implikasi lain yang
   perlu diikuti: koneksi lewat unix socket dengan `peer` authentication (tanpa
   password di berkas konfigurasi), dan `pg_dump` masuk ke prosedur backup.
2. **ModSecurity tidak masuk versi pertama.** Biayanya ~30–40% throughput
   ditambah tuning false positive yang tak berujung — dua hal yang langsung
   menabrak target 10.000 concurrent dan menyita waktu yang lebih baik dipakai
   untuk menyelesaikan Fase 1. WAF ditunda ke Fase 2 sebagai modul dinamis
   opsional; sampai saat itu, lapisan pertahanan yang tersedia adalah rate
   limit, `limit_conn`, access list, snippet `hardening.conf`, dan blocklist
   nftables. Konsekuensi yang harus disadari: HyperNGX v1 **bukan** WAF, jadi
   aplikasi di belakangnya tetap perlu validasi input sendiri.
3. **HTTP/3 opt-in per host, mati secara default.** QUIC menambah beban UDP
   dan sebagian middlebox di jaringan Indonesia masih menjatuhkan UDP/443,
   sehingga menyalakannya global berisiko membuat sebagian klien jatuh ke
   fallback TCP dengan tambahan latensi, bukan berkurang. Modul tetap
   dikompilasi (`--with-http_v3_module`) supaya bisa dinyalakan tanpa build
   ulang; kolom `proxy_hosts.http3` default `FALSE`. Implikasi teknis:
   `reuseport` pada listener QUIC hanya boleh dideklarasikan sekali per
   alamat:port, jadi ia dipasang di `default_server` — bukan di setiap host.
