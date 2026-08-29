<script lang="ts">
  import { listGenerations, rollback, type Generation } from '$lib/api';

  let gens = $state<Generation[]>([]);
  let busy = $state<string | null>(null);

  async function reload() { gens = await listGenerations(); }
  $effect(() => { reload().catch(() => {}); });

  async function goBack(id: string) {
    if (!confirm(`Kembalikan konfigurasi ke ${id}? nginx akan di-reload.`)) return;
    busy = id;
    try { await rollback(id); await reload(); } finally { busy = null; }
  }

  const color = (s: Generation['status']) =>
    s === 'active' ? 'var(--live)' : s === 'failed' ? 'var(--fault)' : 'var(--ink-soft)';

  const label = (s: Generation['status']) =>
    ({ active: 'aktif', failed: 'gagal', staged: 'disiapkan', rolled_back: 'digantikan' })[s];
</script>

<p class="eyebrow">Riwayat konfigurasi</p>
<h1>Setiap perubahan tersimpan utuh</h1>
<p class="note">
  Tiap baris adalah satu konfigurasi nginx lengkap. Mengembalikan konfigurasi
  hanya menukar symlink dan me-reload — tidak ada yang dibangun ulang.
</p>

<table>
  <thead><tr><th>Waktu</th><th>Generation</th><th>Oleh</th><th>Status</th><th></th></tr></thead>
  <tbody>
    {#each gens as g (g.id)}
      <tr>
        <td class="data">{new Date(g.applied_at).toLocaleString('id-ID')}</td>
        <td class="data id">{g.id}</td>
        <td class="data">{g.by ?? '—'}</td>
        <td class="data" style:color={color(g.status)}>{label(g.status)}</td>
        <td class="right">
          {#if g.status !== 'active' && g.status !== 'failed'}
            <button onclick={() => goBack(g.id)} disabled={busy === g.id}>
              {busy === g.id ? 'Menerapkan…' : 'Kembalikan'}
            </button>
          {/if}
        </td>
      </tr>
      {#if g.nginx_test}
        <tr class="failure"><td colspan="5"><pre>{g.nginx_test}</pre></td></tr>
      {/if}
    {/each}
  </tbody>
</table>

<style>
  .note { color: var(--ink-soft); font-size: var(--step--1); max-width: 56ch; }
  table { width: 100%; border-collapse: collapse; margin-top: var(--s5); }
  th {
    text-align: left; font-size: var(--step--1); font-weight: 500;
    color: var(--ink-soft); border-bottom: 2px solid var(--ink); padding: var(--s2);
  }
  td { padding: var(--s3) var(--s2); border-bottom: 1px solid var(--rule); font-size: var(--step--1); }
  .id { color: var(--ink-soft); }
  .right { text-align: right; }
  .failure pre {
    font-family: var(--font-mono); font-size: var(--step--1); color: var(--fault);
    margin: 0; white-space: pre-wrap;
  }
  button {
    font: inherit; font-size: var(--step--1); padding: var(--s1) var(--s3);
    border: 1px solid var(--ink); background: transparent; color: var(--ink);
    border-radius: var(--radius); cursor: pointer;
  }
</style>
