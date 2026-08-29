# Deploy dan menggunakan HyperNGX

Panduan ini mengasumsikan Debian 13 (trixie) yang bersih, akses root, dan
domain yang DNS-nya sudah mengarah ke server. Sisanya dijelaskan dari nol.

---

## Bagian 1 — Menyiapkan server

### 1.1 Ukuran server

Untuk target 10.000 concurrent dengan TLS termination: **4 vCPU / 8 GB RAM**
sebagai titik awal. Lebih kecil dari 2 vCPU / 4 GB tidak disarankan — bukan
karena CPU, tapi karena buffer koneksi dan TLS session cache butuh RAM.

### 1.2 Paket dasar

```bash
apt update && apt upgrade -y
apt install -y curl git build-essential nftables ca-certificates
```

### 1.3 Pastikan port 80 dan 443 bebas

Kalau Apache atau nginx bawaan sudah jalan, keduanya akan berebut port dan
`hyperngx-nginx` gagal start dengan pesan "Address already in use":

```bash
systemctl disable --now apache2 nginx 2>/dev/null || true
ss -tlnp | grep -E ':(80|443)\s'   # harus kosong
```

---

## Bagian 2 — Build

### 2.1 Build nginx

Memakan 5–15 menit tergantung CPU. Ini yang menghasilkan `hyperngx-nginx`
dengan HTTP/3, Brotli, dan headers-more — modul yang tidak ada di paket
bawaan Debian.

```bash
git clone https://github.com/<anda>/hyperngx.git && cd hyperngx
sudo ./packaging/build-nginx.sh
/usr/sbin/hyperngx-nginx -V     # verifikasi: cari --with-http_v3_module
```

### 2.2 Build control plane

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# sqlx memverifikasi query ke database saat kompilasi, jadi database
# harus ada lebih dulu.
sudo -u postgres createuser --superuser "$USER" 2>/dev/null || true
createdb hyperngx_build 2>/dev/null || true
psql -d hyperngx_build -f migrations/0001_init.sql
export DATABASE_URL="postgres:///hyperngx_build"

cargo build --release
```

### 2.3 Build UI

```bash
curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
apt install -y nodejs
cd web && npm ci && npm run build && cd ..
```

### 2.4 Pasang

```bash
sudo ./packaging/install.sh
sudo systemctl enable --now hyperngx-supervisor hyperngx-nginx hyperngx-api
systemctl status hyperngx-supervisor hyperngx-nginx hyperngx-api --no-pager
```

---

## Bagian 3 — Login pertama

Password owner dibuat acak saat start pertama dan ditulis ke berkas 0600:

```bash
sudo cat /etc/hyperngx/bootstrap.txt
```

Buka `https://<ip-server>:8443`. Browser akan memperingatkan sertifikat
self-signed — wajar, panel memakai sertifikat sementara sampai Anda memberinya
domain sendiri.

**Tiga hal yang harus segera dilakukan:**

1. Ganti password owner.
2. Aktifkan autentikator (TOTP). Untuk role owner ini bukan opsional dalam
   praktik: siapa pun yang menguasai panel menguasai routing seluruh trafik.
3. Hapus `/etc/hyperngx/bootstrap.txt`.

Panel dibatasi ke jaringan privat oleh `conf.d/admin.conf` dan nftables.
Kalau Anda mengaksesnya dari internet, edit kedua berkas itu — dan
pertimbangkan serius untuk memakai VPN alih-alih membuka 8443 ke publik.

---

## Bagian 4 — Jalur pertama

Skenario: `app.sekolah.id` diteruskan ke aplikasi di `10.0.0.5:3000`.

### 4.1 Pastikan DNS sudah mengarah

```bash
dig +short app.sekolah.id      # harus menghasilkan IP server ini
```

Kalau belum, ACME akan gagal dan kegagalannya memakan kuota rate limit
Let's Encrypt (5 kegagalan/jam/akun). Periksa dulu.

### 4.2 Terbitkan sertifikat

Buka **Sertifikat → Terbitkan**. Isi `app.sekolah.id`, pilih **HTTP-01**,
klik Terbitkan. Butuh 5–20 detik.

Untuk wildcard (`*.sekolah.id`) pilih **DNS-01**. Sebelumnya, simpan token
Cloudflare:

```bash
sudo install -m 600 /dev/null /etc/hyperngx/secrets/cloudflare.token
sudo tee /etc/hyperngx/secrets/cloudflare.token <<< "TOKEN_ANDA"
```

Token tidak pernah masuk database atau melewati IPC — yang dikirim hanya
nama berkasnya.

### 4.3 Buat jalur

**Jalur → Tambah jalur**:

| Kolom | Isi |
|---|---|
| Domain | `app.sekolah.id` |
| Protokol ke upstream | `http` |
| Target | `10.0.0.5` port `3000` |
| Layani lewat HTTPS | ya |
| Sertifikat | `app.sekolah.id` |
| Alihkan HTTP ke HTTPS | ya |

Klik **Simpan dan terapkan**. Yang terjadi di balik layar: API mengirim
seluruh state ke supervisor, supervisor merender konfigurasi ke direktori
generation baru, menjalankan `nginx -t`, menukar symlink, lalu reload.
Kalau `nginx -t` gagal, tidak ada yang berubah dan pesan errornya muncul
di UI.

### 4.4 Verifikasi

```bash
curl -I https://app.sekolah.id
curl -I http://app.sekolah.id          # harus 308 ke https
```

---

## Bagian 5 — Operasi sehari-hari

### Menguji sebelum menerapkan

Tombol **Uji konfigurasi** di halaman Jalur menjalankan `nginx -t` atas
state saat ini tanpa mengaktifkan apa pun. Pakai ini setelah mengedit
konfigurasi lanjutan.

### Mengembalikan konfigurasi

**Riwayat konfigurasi** menyimpan 20 generation terakhir. Tombol
*Kembalikan* menukar symlink dan reload — hitungan detik, dan tidak ada
yang dibangun ulang. Ini jaring pengaman untuk perubahan yang lolos
`nginx -t` tapi ternyata salah secara logika (misalnya salah upstream).

### Sertifikat

Perpanjangan otomatis pada sisa 30 hari, dengan jitter 0–12 jam. Halaman
Sertifikat hanya perlu disentuh saat menambah domain atau saat sebuah
perpanjangan gagal — kegagalan ditampilkan merah di baris sertifikat.

Memantau dari luar UI:

```bash
journalctl -u hyperngx-supervisor -f | grep -i acme
```

### Log

```bash
tail -f /var/log/hyperngx/access.log | jq .      # JSON per baris
journalctl -u hyperngx-api -f                    # aktivitas panel
```

Log akses memuat IP klien. Kalau HyperNGX memproses data pribadi, atur
retensi terbatas lewat logrotate — relevan untuk UU PDP.

### Backup

```bash
sudo -u postgres pg_dump hyperngx | gzip > hyperngx-$(date +%F).sql.gz
sudo tar czf hyperngx-tls-$(date +%F).tar.gz /etc/hyperngx/tls
```

Uji restore-nya minimal sekali. Backup yang tidak pernah dicoba dipulihkan
adalah asumsi, bukan cadangan.

---

## Bagian 6 — Verifikasi kapasitas

Jangan percaya angka 10.000 tanpa mengukurnya di server Anda sendiri.

```bash
# Ketahanan koneksi, bukan throughput
wrk2 -t8 -c10000 -d300s -R20000 --latency https://app.sekolah.id/

# Amati bersamaan, di terminal lain:
watch -n1 'ss -s; cat /proc/sys/fs/file-nr'
```

Kalau `nginx_connections_waiting` menumpuk sementara CPU rendah, batasnya
ada di upstream — bukan di HyperNGX. Menaikkan `worker_connections` tidak
akan menolong; yang perlu diperbaiki adalah aplikasi di belakangnya.

---

## Bagian 7 — Saat ada yang salah

| Gejala | Kemungkinan penyebab | Langkah |
|---|---|---|
| `nginx -t` gagal saat menyimpan jalur | Sertifikat belum ada atau salah pilih | Cek `journalctl -u hyperngx-supervisor`; pesan lengkap juga tampil di Riwayat konfigurasi |
| Penerbitan sertifikat gagal | DNS belum propagasi, atau port 80 tertutup | `dig +short <domain>`, lalu `curl -I http://<domain>/.well-known/acme-challenge/test` |
| Panel tidak bisa dibuka | IP Anda di luar daftar allow | Edit `/etc/hyperngx/nginx/conf.d/admin.conf` dan aturan nftables |
| API gagal start, error autentikasi database | `PrivateUsers=yes` bentrok dengan `peer` auth | Matikan baris itu di unit, atau pindah ke `scram-sha-256` |
| Reload sukses tapi trafik tetap ke upstream lama | Browser menyimpan koneksi keep-alive | Normal; koneksi lama selesai sendiri dalam 65 detik |
| 502 setelah menambah jalur | Upstream tidak menerima koneksi dari server ini | `curl -v http://<upstream>:<port>` dari server HyperNGX |

Perintah diagnosa yang paling sering berguna:

```bash
systemctl status hyperngx-supervisor hyperngx-nginx hyperngx-api --no-pager
/usr/sbin/hyperngx-nginx -t -c /etc/hyperngx/nginx/active/nginx.conf
ls -l /etc/hyperngx/nginx/active         # symlink menunjuk generation mana
journalctl -u hyperngx-supervisor -n 100 --no-pager
```
