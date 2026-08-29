#!/usr/bin/env bash
# =====================================================================
# HyperNGX - build nginx "pre-built" untuk Debian 13 (trixie)
# Menghasilkan /usr/sbin/hyperngx-nginx + modul dinamis.
# Alasan build sendiri: paket bawaan Debian tidak menyertakan HTTP/3,
# Brotli, headers-more, dan tidak di-link ke OpenSSL 3.x dengan
# konfigurasi yang kita inginkan.
# =====================================================================
set -euo pipefail

NGINX_VER="${NGINX_VER:-1.28.0}"          # stable branch; cek nginx.org sebelum rilis
PREFIX="/usr/share/hyperngx/nginx"
MODULES_DIR="/usr/lib/hyperngx/modules"
BUILD_DIR="$(mktemp -d)"
JOBS="$(nproc)"

apt-get update
apt-get install -y --no-install-recommends \
  build-essential cmake git curl ca-certificates pkg-config \
  libpcre2-dev zlib1g-dev libssl-dev libmaxminddb-dev
# Catatan: libxml2/libyajl/libcurl sengaja tidak dipasang — itu dependensi
# ModSecurity, yang tidak dibangun di v1 (lihat docs/ROADMAP.md).

cd "$BUILD_DIR"
curl -fsSLO "https://nginx.org/download/nginx-${NGINX_VER}.tar.gz"
curl -fsSLO "https://nginx.org/download/nginx-${NGINX_VER}.tar.gz.asc"   # verifikasi wajib
tar xzf "nginx-${NGINX_VER}.tar.gz"

git clone --depth=1 --recursive https://github.com/google/ngx_brotli.git
git clone --depth=1 https://github.com/openresty/headers-more-nginx-module.git

cd "nginx-${NGINX_VER}"
./configure \
  --prefix="${PREFIX}" \
  --sbin-path=/usr/sbin/hyperngx-nginx \
  --conf-path=/etc/hyperngx/nginx/nginx.conf \
  --pid-path=/run/hyperngx/nginx.pid \
  --lock-path=/run/hyperngx/nginx.lock \
  --error-log-path=/var/log/hyperngx/error.log \
  --http-log-path=/var/log/hyperngx/access.log \
  --modules-path="${MODULES_DIR}" \
  --user=hyperngx --group=hyperngx \
  --with-threads \
  --with-file-aio \
  --with-http_ssl_module \
  --with-http_v2_module \
  --with-http_v3_module \
  --with-http_realip_module \
  --with-http_stub_status_module \
  --with-http_gzip_static_module \
  --with-http_sub_module \
  --with-http_auth_request_module \
  --with-http_slice_module \
  --with-stream \
  --with-stream_ssl_module \
  --with-stream_ssl_preread_module \
  --with-stream_realip_module \
  --without-http_autoindex_module \
  --without-http_ssi_module \
  --without-mail_pop3_module \
  --without-mail_imap_module \
  --without-mail_smtp_module \
  --add-dynamic-module=../ngx_brotli \
  --add-dynamic-module=../headers-more-nginx-module \
  --with-cc-opt="-O2 -fstack-protector-strong -D_FORTIFY_SOURCE=2 -Wformat -Werror=format-security -fPIC" \
  --with-ld-opt="-Wl,-z,relro -Wl,-z,now -Wl,--as-needed -pie"

make -j"${JOBS}"
make install

strip /usr/sbin/hyperngx-nginx || true
/usr/sbin/hyperngx-nginx -V
echo "Build selesai. Modul ada di ${MODULES_DIR}"
