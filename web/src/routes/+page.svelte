<script lang="ts">
  import RouteStrip from '$lib/RouteStrip.svelte';
  import { listHosts, dryRun, type ProxyHost } from '$lib/api';

  let hosts = $state<ProxyHost[]>([]);
  let error = $state<string | null>(null);
  let testing = $state(false);
  let testOutput = $state<string | null>(null);

  $effect(() => {
    listHosts().then((h) => (hosts = h)).catch((e) => (error = e.message));
  });

  async function runTest() {
    testing = true; testOutput = null;
    try {
      const r = await dryRun();
      testOutput = r.ok ? 'Konfigurasi lolos nginx -t.' : r.output;
    } catch (e) { testOutput = (e as Error).message; }
    finally { testing = false; }
  }
</script>

<section class="head">
  <div>
    <p class="eyebrow">Jalur aktif</p>
    <h1>{hosts.filter((h) => h.enabled).length} jalur melayani trafik</h1>
  </div>
  <div class="actions">
    <button onclick={runTest} disabled={testing}>
      {testing ? 'Menguji…' : 'Uji konfigurasi'}
    </button>
    <a class="primary" href="/hosts/new">Tambah jalur</a>
  </div>
</section>

{#if testOutput}
  <pre class="output">{testOutput}</pre>
{/if}

{#if error}
  <p class="empty">Daftar jalur gagal dimuat: {error}. Coba muat ulang halaman.</p>
{:else if hosts.length === 0}
  <p class="empty">Belum ada jalur. Tambahkan satu untuk mulai meneruskan domain ke upstream.</p>
{:else}
  <div class="strips">
    {#each hosts as host (host.id)}<RouteStrip {host} />{/each}
  </div>
{/if}

<style>
  .head { display: flex; justify-content: space-between; align-items: flex-end; gap: var(--s4); margin-bottom: var(--s5); }
  .actions { display: flex; gap: var(--s2); }
  button, .primary {
    font: inherit; font-size: var(--step--1);
    padding: var(--s2) var(--s4); border: 1px solid var(--ink);
    background: transparent; color: var(--ink);
    border-radius: var(--radius); cursor: pointer; text-decoration: none;
  }
  .primary { background: var(--ink); color: var(--paper); }
  .strips { border-top: 1px solid var(--rule); background: var(--paper-2); }
  .output {
    font-family: var(--font-mono); font-size: var(--step--1);
    background: var(--paper-2); border-left: 3px solid var(--tls);
    padding: var(--s3) var(--s4); margin-bottom: var(--s4); overflow-x: auto;
  }
  .empty { color: var(--ink-soft); padding: var(--s6) 0; }
</style>
