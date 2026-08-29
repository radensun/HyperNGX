// Klien API. Semua request tulis membawa CSRF token dari cookie
// non-HttpOnly `hngx_csrf`; cookie sesi sendiri HttpOnly dan tak terbaca JS.
const BASE = '/api/v1';

function csrf(): string {
  return document.cookie.split('; ').find((c) => c.startsWith('hngx_csrf='))?.split('=')[1] ?? '';
}

export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const res = await fetch(BASE + path, {
    ...init,
    credentials: 'same-origin',
    headers: {
      'content-type': 'application/json',
      'x-csrf-token': csrf(),
      ...(init.headers ?? {})
    }
  });
  if (res.status === 401 && !path.startsWith('/auth/login')) {
    location.href = '/login';
    throw new Error('unauthenticated');
  }
  if (!res.ok) throw new Error((await res.json().catch(() => ({}))).message ?? `HTTP ${res.status}`);
  return res.status === 204 ? (undefined as T) : ((await res.json()) as T);
}

const post = <T>(p: string, body?: unknown) =>
  api<T>(p, { method: 'POST', body: body ? JSON.stringify(body) : undefined });

export type CertState = 'valid' | 'expiring' | 'expired' | 'none';

export interface Target { address: string; port: number; weight?: number }

export interface ProxyHost {
  id: number;
  domains: string[];
  targets: Target[];
  scheme: 'http' | 'https';
  ssl_enabled: boolean;
  http3: boolean;
  cert_state: CertState;
  cert_days_left: number | null;
  enabled: boolean;
  health: 'up' | 'degraded' | 'down' | 'unknown';
}

export interface HostInput {
  domains: string[];
  scheme: string;
  targets: Target[];
  load_balance: string;
  certificate_id: number | null;
  ssl_enabled: boolean;
  force_ssl: boolean;
  http2: boolean;
  http3: boolean;
  hardening: boolean;
  block_common_exploits: boolean;
  client_max_body_size: string;
  advanced_config: string;
  enabled: boolean;
}

export interface Certificate {
  id: number;
  slug: string;
  domains: string[];
  provider: string;
  not_after: string | null;
  days_left: number | null;
  last_error: string | null;
}

export interface Generation {
  id: string;
  applied_at: string;
  status: 'staged' | 'active' | 'failed' | 'rolled_back';
  nginx_test: string | null;
  by: string | null;
}

export const login = (username: string, password: string, totp?: string) =>
  post<{ username: string; role: string }>('/auth/login', { username, password, totp });
export const logout = () => post('/auth/logout');
export const me = () => api<{ username: string; role: string; totp_enabled: boolean }>('/auth/me');

export const listHosts = () => api<ProxyHost[]>('/hosts');
export const getHost = (id: number) => api<Record<string, unknown>>(`/hosts/${id}`);
export const createHost = (h: HostInput) => post<{ id: number }>('/hosts', h);
export const updateHost = (id: number, h: HostInput) =>
  api<{ id: number }>(`/hosts/${id}`, { method: 'PUT', body: JSON.stringify(h) });
export const deleteHost = (id: number) => api(`/hosts/${id}`, { method: 'DELETE' });
export const toggleHost = (id: number) => post(`/hosts/${id}/toggle`);

export const listCerts = () => api<Certificate[]>('/certificates');
export const requestCert = (domains: string[], challenge: unknown) =>
  post<Certificate>('/certificates', { domains, challenge });
export const renewCert = (slug: string) => post(`/certificates/${slug}/renew`);
export const revokeCert = (slug: string) => api(`/certificates/${slug}`, { method: 'DELETE' });

export const dryRun = () => post<{ ok: boolean; output: string }>('/config/dry-run');
export const rollback = (generation_id?: string) => post('/config/rollback', { generation_id });
export const listGenerations = () => api<Generation[]>('/generations');
