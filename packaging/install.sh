#!/usr/bin/env bash
# HyperNGX installer - Debian 13 (trixie)
# Idempoten: aman dijalankan ulang untuk upgrade.
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo "Jalankan sebagai root."; exit 1; }

SRC="$(cd "$(dirname "$0")/.." && pwd)"

echo "==> Membuat user sistem"
id -u hyperngx     >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin hyperngx
id -u hyperngx-api >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin hyperngx-api

echo "==> Menyiapkan direktori"
install -d -o root         -g root         -m 0755 /etc/hyperngx
install -d -o root         -g hyperngx     -m 0750 /etc/hyperngx/nginx
install -d -o root         -g hyperngx     -m 0750 /etc/hyperngx/nginx/{hosts,streams,snippets,conf.d,access,generations}
install -d -o root         -g root         -m 0700 /etc/hyperngx/tls          # private key hanya root
install -d -o root         -g root         -m 0700 /etc/hyperngx/tls/{live,archive,accounts,ticket}
install -d -o root         -g root         -m 0700 /etc/hyperngx/secrets      # token DNS provider
install -d -o hyperngx     -g hyperngx     -m 0755 /var/lib/hyperngx/acme
install -d -o hyperngx     -g hyperngx     -m 0750 /var/lib/hyperngx/tmp
install -d -o hyperngx     -g hyperngx     -m 0750 /var/log/hyperngx
install -d -o hyperngx     -g hyperngx     -m 0750 /var/cache/hyperngx/proxy
install -d -o hyperngx     -g hyperngx     -m 0755 /run/hyperngx
install -d -o root         -g root         -m 0755 /usr/share/hyperngx/{templates,web}

echo "==> Memasang template dan konfigurasi"
install -m 0644 "$SRC"/templates/*.j2               /usr/share/hyperngx/templates/
install -d -m 0755 /usr/share/hyperngx/templates/snippets
install -m 0644 "$SRC"/templates/snippets/*.conf    /usr/share/hyperngx/templates/snippets/
install -m 0644 "$SRC"/templates/snippets/*.conf    /etc/hyperngx/nginx/snippets/
install -m 0644 "$SRC"/packaging/nginx-admin.conf   /etc/hyperngx/nginx/conf.d/admin.conf
[[ -f /etc/hyperngx/supervisor.toml ]] || install -m 0640 "$SRC"/packaging/supervisor.toml /etc/hyperngx/supervisor.toml
cp /usr/share/hyperngx/nginx/conf/mime.types /etc/hyperngx/nginx/mime.types 2>/dev/null || \
  cp /etc/nginx/mime.types /etc/hyperngx/nginx/mime.types

echo "==> Sertifikat default (self-signed)"
# Dipakai default_server dan panel admin. Tanpa ini `nginx -t` gagal
# sebelum sertifikat Let's Encrypt pertama sempat terbit.
if [[ ! -f /etc/hyperngx/tls/live/default/privkey.pem ]]; then
  install -d -m 0700 /etc/hyperngx/tls/live/default
  openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
    -keyout /etc/hyperngx/tls/live/default/privkey.pem \
    -out    /etc/hyperngx/tls/live/default/fullchain.pem \
    -days 3650 -nodes -subj "/CN=hyperngx-default" 2>/dev/null
  chmod 600 /etc/hyperngx/tls/live/default/privkey.pem
  chmod 644 /etc/hyperngx/tls/live/default/fullchain.pem
fi
# Bundle CA untuk verifikasi OCSP stapling.
cp /etc/ssl/certs/ca-certificates.crt /etc/hyperngx/tls/ca-bundle.pem

echo "==> Resolver lokal untuk OCSP stapling"
# nginx butuh resolver untuk menghubungi OCSP responder. systemd-resolved
# sudah mendengar di 127.0.0.53; snippet TLS diarahkan ke sana.
sed -i 's|resolver  *127\.0\.0\.1:5353|resolver 127.0.0.53|' /etc/hyperngx/nginx/snippets/ssl-baseline.conf

echo "==> Menyiapkan PostgreSQL"
apt-get install -y --no-install-recommends postgresql openssl
systemctl enable --now postgresql
# Role dipetakan ke user sistem hyperngx-api => autentikasi `peer` lewat
# unix socket, tanpa password yang perlu disimpan di berkas konfigurasi.
sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='hyperngx-api'" | grep -q 1 \
  || sudo -u postgres createuser hyperngx-api
sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='hyperngx'" | grep -q 1 \
  || sudo -u postgres createdb -O hyperngx-api hyperngx

echo "==> Memasang biner dan UI"
install -m 0755 "$SRC"/target/release/hyperngx-supervisor /usr/bin/
install -m 0755 "$SRC"/target/release/hyperngx-api        /usr/bin/
cp -r "$SRC"/web/build/. /usr/share/hyperngx/web/
chmod -R a+rX /usr/share/hyperngx/web

echo "==> Menerapkan kernel tuning"
install -m 0644 "$SRC"/packaging/sysctl/99-hyperngx.conf /etc/sysctl.d/99-hyperngx.conf
install -m 0644 "$SRC"/packaging/limits/hyperngx.conf    /etc/security/limits.d/hyperngx.conf
modprobe tcp_bbr || true
echo tcp_bbr > /etc/modules-load.d/hyperngx-bbr.conf
sysctl --system >/dev/null

echo "==> Firewall (nftables)"
install -d -m 0755 /etc/nftables.d
cat >/etc/nftables.d/hyperngx.nft <<'NFT'
table inet hyperngx {
  set blocklist { type ipv4_addr; flags timeout; }
  chain input {
    type filter hook input priority 0; policy drop;
    ct state established,related accept
    ct state invalid drop
    iif lo accept
    ip saddr @blocklist drop
    tcp dport { 80, 443 } ct count over 200 add @blocklist { ip saddr timeout 10m }
    tcp dport { 80, 443 } accept
    udp dport 443 accept
    tcp dport 22 accept
    # Panel admin: batasi ke jaringan pengelola, jangan dibuka ke internet.
    tcp dport 8443 ip saddr { 10.0.0.0/8, 192.168.0.0/16 } accept
    icmp type echo-request limit rate 5/second accept
  }
}
NFT
grep -q 'include "/etc/nftables.d/\*.nft"' /etc/nftables.conf 2>/dev/null || \
  echo 'include "/etc/nftables.d/*.nft"' >> /etc/nftables.conf

echo "==> Memasang unit systemd"
install -m 0644 "$SRC"/packaging/systemd/*.service /etc/systemd/system/
systemctl daemon-reload

echo
echo "Instalasi selesai. Jalankan:"
echo "    systemctl enable --now hyperngx-supervisor hyperngx-nginx hyperngx-api"
echo "    Panel admin: https://<server>:8443"
echo "    Kredensial awal: /etc/hyperngx/bootstrap.txt"
