# Kapasitas 10.000 concurrent — hitungannya

Angka 10.000 tidak dikonfigurasi, tapi diturunkan. Berikut rantai batasnya.

## 1. File descriptor

Satu koneksi klien yang diteruskan memakai **2 FD**: socket klien + socket
upstream. Ditambah log, cache, dan pool keep-alive:

```
10.000 klien × 2                    = 20.000
keep-alive pool ke upstream         = ~2.000
open_file_cache + log + socket dengar = ~1.000
                                      ─────────
kebutuhan puncak                    ≈ 23.000
```

Kita tetapkan `worker_rlimit_nofile 200000` (≈8× kepala) karena FD murah
(≈1 KB struktur kernel) sedangkan `EMFILE` di tengah lonjakan trafik mahal.
`LimitNOFILE=200000` di unit systemd wajib disetel juga — `ulimit` shell tidak
berlaku untuk layanan systemd.

## 2. worker_connections

```
kapasitas = worker_processes × worker_connections / 2   (karena 2 FD per request)
```

Pada 8 vCPU: `8 × 16.384 / 2 = 65.536` koneksi proxy simultan — 6,5× target.
Marjin ini disengaja: lonjakan koneksi `TIME_WAIT` dan retry saat upstream
lambat bisa melipatgandakan kebutuhan sesaat.

## 3. Memori

| Komponen | Perkiraan |
|---|---|
| Per koneksi HTTP (buffer 8k + 16×16k) | ~30–60 KB saat aktif |
| 10.000 koneksi aktif | ~600 MB |
| TLS session cache 100m | 100 MB (≈400.000 sesi) |
| Zona limit_conn/limit_req (50m) | 50 MB |
| nginx worker overhead | ~50 MB |
| hyperngx-api (dibatasi `MemoryMax=512M`) | ≤512 MB |

**Baseline yang disarankan: 4 vCPU / 8 GB RAM** untuk 10.000 concurrent dengan
TLS termination. Kalau ada `proxy_cache` besar, tambah RAM untuk page cache —
`keys_zone=256m` hanya menyimpan metadata (≈2 juta objek), badan objek ada di
disk dan bergantung pada page cache OS.

## 4. Antrian accept

`net.core.somaxconn` harus **≥** nilai `backlog=` pada direktif `listen`.
Kita pakai 65535 di keduanya. `listen ... reuseport` membuat kernel memberi
tiap worker antrian accept sendiri — menghilangkan thundering herd, dan
itulah sebabnya `accept_mutex off`.

## 5. Port ephemeral

Proxy membuka koneksi keluar ke upstream. Dengan rentang default Debian
(32768–60999) hanya tersedia ~28.000 port per pasangan IP tujuan. Kita
lebarkan ke `10240–65535` (~55.000) dan mengaktifkan `tcp_tw_reuse`.
**Yang jauh lebih menentukan**: `keepalive 128` di blok `upstream` —
koneksi dipakai ulang sehingga port ephemeral nyaris tidak pernah jadi
batas. Tanpa itu, 10.000 rps akan menghabiskan port dalam hitungan detik.

## 6. TLS

Handshake ECDSA P-256 ≈ 10× lebih murah dari RSA-2048 → default sertifikat
adalah ECDSA. `ssl_session_cache 100m` + ticket menekan handshake penuh ke
di bawah 10% pada trafik nyata. `ssl_buffer_size 4k` membuat record TLS muat
dalam satu paket sehingga TTFB turun untuk respons kecil.

`ssl_early_data` sengaja **off**: 0-RTT rentan replay dan sebagian besar
upstream tidak idempoten pada POST.

## 7. Congestion control

`tcp_congestion_control=bbr` + `default_qdisc=fq`. Pada jalur internasional
dengan RTT tinggi dan sedikit packet loss (kasus umum server di Indonesia
melayani klien lintas ISP), BBR memberi throughput jauh lebih stabil daripada
CUBIC.

## Verifikasi

Jangan percaya angka di atas tanpa pengukuran. Urutan uji:

```bash
# 1. Ketahanan koneksi (bukan throughput): 10k koneksi ditahan
wrk2 -t8 -c10000 -d300s -R20000 --latency https://target/

# 2. Handshake TLS penuh (cache dimatikan di sisi klien)
h2load -n 200000 -c 1000 -m 10 https://target/

# 3. Cari titik patah, bukan angka bagus
for c in 2000 5000 10000 15000 20000; do wrk -t8 -c$c -d60s https://target/; done
```

Metrik yang wajib diamati bersamaan: `nginx_connections_waiting`,
`ss -s` (jumlah TIME_WAIT), `dmesg | grep -i "SYN flooding"`, dan
`node_filefd_allocated`. Kalau `waiting` menumpuk sementara CPU rendah,
batasnya ada di upstream — bukan di nginx.
