# Berkontribusi

## Menyiapkan lingkungan pengembangan

```bash
# Database untuk verifikasi query sqlx saat kompilasi
createdb hyperngx_build
psql -d hyperngx_build -f migrations/0001_init.sql
cp .env.example .env

cargo build
cd web && npm ci && npm run dev
```

## Sebelum mengirim perubahan

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cd web && npm run check
```

## Aturan yang tidak bisa ditawar

1. **Proses API tidak pernah menulis berkas nginx atau menyentuh private
   key.** Kalau sebuah fitur membutuhkannya, fitur itu pindah ke supervisor
   sebagai varian `Command` baru — bukan privilege API yang dinaikkan.
2. **Tidak ada `sh -c`.** Semua pemanggilan proses memakai `Command` dengan
   argumen terpisah.
3. **Input yang masuk ke konfigurasi nginx harus lewat `hyperngx_core::validate`.**
   Validasi di sisi API adalah untuk pesan error yang enak dibaca; validasi
   di supervisor adalah pertahanan sesungguhnya. Jangan hapus salah satunya.
4. **Perubahan pada template nginx wajib disertai uji beban ulang.** Angka
   di `docs/PERFORMANCE.md` diturunkan dari nilai di template; mengubah
   `worker_connections` atau ukuran buffer tanpa mengukur ulang membuat
   dokumen itu berbohong.
