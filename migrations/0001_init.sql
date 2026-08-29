-- HyperNGX skema awal (PostgreSQL 16+)
-- Konvensi: identitas pakai GENERATED ALWAYS AS IDENTITY, waktu pakai
-- TIMESTAMPTZ (selalu UTC di penyimpanan), struktur pakai JSONB agar bisa
-- diindeks dan divalidasi di sisi database.

CREATE TABLE users (
  id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  username      TEXT NOT NULL UNIQUE,
  email         TEXT NOT NULL,
  password_hash TEXT NOT NULL,                 -- argon2id
  role          TEXT NOT NULL CHECK (role IN ('owner','operator','viewer')),
  totp_secret   BYTEA,                         -- terenkripsi dengan kunci di supervisor
  totp_enabled  BOOLEAN NOT NULL DEFAULT FALSE,
  disabled      BOOLEAN NOT NULL DEFAULT FALSE,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_login_at TIMESTAMPTZ
);

CREATE TABLE sessions (
  token_hash   BYTEA PRIMARY KEY,              -- SHA-256 dari token, bukan tokennya
  user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  ip           INET NOT NULL,
  user_agent   TEXT,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at   TIMESTAMPTZ NOT NULL,
  revoked_at   TIMESTAMPTZ
);
CREATE INDEX idx_sessions_user ON sessions(user_id);
-- Sapu sesi kedaluwarsa tanpa full scan.
CREATE INDEX idx_sessions_expiry ON sessions(expires_at) WHERE revoked_at IS NULL;

CREATE TABLE certificates (
  id             BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  slug           TEXT NOT NULL UNIQUE,
  domains        JSONB NOT NULL,
  provider       TEXT NOT NULL CHECK (provider IN ('letsencrypt','custom','self_signed')),
  challenge      JSONB NOT NULL,               -- http-01 / dns-01 + provider
  key_type       TEXT NOT NULL DEFAULT 'ecdsa256',
  not_before     TIMESTAMPTZ,
  not_after      TIMESTAMPTZ,
  last_error     TEXT,
  renew_attempts INTEGER NOT NULL DEFAULT 0,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Query utama scheduler: "sertifikat apa yang jatuh tempo?"
CREATE INDEX idx_cert_expiry ON certificates(not_after) WHERE not_after IS NOT NULL;
CREATE INDEX idx_cert_domains ON certificates USING GIN (domains);

CREATE TABLE access_lists (
  id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,
  satisfy_any BOOLEAN NOT NULL DEFAULT FALSE,
  rules       JSONB NOT NULL DEFAULT '[]'::jsonb,   -- allow/deny CIDR
  basic_auth  JSONB NOT NULL DEFAULT '[]'::jsonb    -- {user, bcrypt_hash}
);

CREATE TABLE proxy_hosts (
  id                    BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  domains               JSONB NOT NULL,
  scheme                TEXT NOT NULL DEFAULT 'http' CHECK (scheme IN ('http','https')),
  targets               JSONB NOT NULL,        -- [{address,port,weight,...}]
  load_balance          TEXT NOT NULL DEFAULT 'round_robin'
                          CHECK (load_balance IN ('round_robin','least_conn','ip_hash')),
  locations             JSONB NOT NULL DEFAULT '[]'::jsonb,
  certificate_id        BIGINT REFERENCES certificates(id) ON DELETE SET NULL,
  ssl_enabled           BOOLEAN NOT NULL DEFAULT FALSE,
  force_ssl             BOOLEAN NOT NULL DEFAULT TRUE,
  http2                 BOOLEAN NOT NULL DEFAULT TRUE,
  http3                 BOOLEAN NOT NULL DEFAULT FALSE,
  hsts_disabled         BOOLEAN NOT NULL DEFAULT FALSE,
  hardening             BOOLEAN NOT NULL DEFAULT TRUE,
  block_common_exploits BOOLEAN NOT NULL DEFAULT TRUE,
  cache_enabled         BOOLEAN NOT NULL DEFAULT FALSE,
  access_list_id        BIGINT REFERENCES access_lists(id) ON DELETE SET NULL,
  max_conn              INTEGER NOT NULL DEFAULT 4000,
  client_max_body_size  TEXT NOT NULL DEFAULT '64m',
  advanced_config       TEXT NOT NULL DEFAULT '',
  enabled               BOOLEAN NOT NULL DEFAULT TRUE,
  created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT domains_tidak_kosong CHECK (jsonb_array_length(domains) > 0),
  CONSTRAINT targets_tidak_kosong CHECK (jsonb_array_length(targets) > 0)
);
-- Cegah dua host merebut domain yang sama: server_name bentrok di nginx
-- hanya menghasilkan peringatan, jadi batasnya ditegakkan di sini.
CREATE UNIQUE INDEX idx_host_domain_unik
  ON proxy_hosts ((domains->>0)) WHERE enabled;
CREATE INDEX idx_host_domains ON proxy_hosts USING GIN (domains);

CREATE TABLE stream_hosts (
  id             BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  name           TEXT NOT NULL,
  listen_port    INTEGER NOT NULL CHECK (listen_port BETWEEN 1 AND 65535),
  udp            BOOLEAN NOT NULL DEFAULT FALSE,
  targets        JSONB NOT NULL,
  tls_terminate  BOOLEAN NOT NULL DEFAULT FALSE,
  certificate_id BIGINT REFERENCES certificates(id) ON DELETE SET NULL,
  proxy_protocol BOOLEAN NOT NULL DEFAULT FALSE,
  enabled        BOOLEAN NOT NULL DEFAULT TRUE,
  UNIQUE (listen_port, udp)
);

CREATE TABLE redirect_hosts (
  id             BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  domains        JSONB NOT NULL,
  target_url     TEXT NOT NULL,
  status_code    INTEGER NOT NULL DEFAULT 308 CHECK (status_code IN (301,302,307,308)),
  preserve_path  BOOLEAN NOT NULL DEFAULT TRUE,
  certificate_id BIGINT REFERENCES certificates(id) ON DELETE SET NULL,
  enabled        BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE generations (
  id         TEXT PRIMARY KEY,                 -- ULID, terurut waktu
  applied_by BIGINT REFERENCES users(id) ON DELETE SET NULL,
  applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  status     TEXT NOT NULL CHECK (status IN ('staged','active','failed','rolled_back')),
  nginx_test TEXT,
  snapshot   JSONB NOT NULL                    -- state lengkap untuk rollback
);
-- Hanya boleh ada satu generation aktif pada satu waktu.
CREATE UNIQUE INDEX idx_generation_aktif ON generations ((status)) WHERE status = 'active';

CREATE TABLE audit_log (
  id        BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id   BIGINT REFERENCES users(id) ON DELETE SET NULL,
  ip        INET,
  action    TEXT NOT NULL,
  entity    TEXT,
  entity_id TEXT,
  diff      JSONB,
  at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_audit_at ON audit_log(at DESC);
CREATE INDEX idx_audit_user ON audit_log(user_id, at DESC);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
INSERT INTO settings (key, value) VALUES
  ('acme_directory', 'https://acme-v02.api.letsencrypt.org/directory'),
  ('worker_connections', '16384'),
  ('default_client_max_body_size', '64m');

-- updated_at dipelihara database, bukan aplikasi: kalau ada jalur tulis
-- yang lupa menyetelnya, riwayat tetap benar.
CREATE FUNCTION set_updated_at() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  NEW.updated_at := now();
  RETURN NEW;
END $$;

CREATE TRIGGER trg_proxy_hosts_updated
  BEFORE UPDATE ON proxy_hosts
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();
