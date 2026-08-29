<script lang="ts">
  import { listCerts, requestCert, renewCert, revokeCert, type Certificate } from '$lib/api';

  let certs = $state<Certificate[]>([]);
  let domainText = $state('');
  let method = $state<'http01' | 'dns01'>('http01');
  let cfTokenRef = $state('cloudflare.token');
  let error = $state<string | null>(null);
  let busy = $state(false);

  const wildcard = $derived(domainText.includes('*.'));

  async function reload() { certs = await listCerts(); }
  $effect(() => { reload().catch((e) => (error = (e as Error).message)); });

  async function issue(e: Event) {
    e.preventDefault();
    busy = true; error = null;
    const domains = domainText.split('\n').map((d) => d.trim()).filter(Boolean);
    const challenge = method === 'http01'
      ? { type: 'http01' }
      : { type: 'dns01', provider: { kind: 'cloudflare', api_token_ref: cfTokenRef } };
    try {
      await requestCert(domains, challenge);
      domainText = '';
      await reload();
    } catch (err) {
      error = (err as Error).message;
    } finally {
      busy = false;
    }
  }

  function stateOf(c: Certificate) {
    if (c.days_left === null) return { label: 'tidak diketahui', color: 'var(--ink-soft)' };
    if (c.days_left < 0) return { label: 'kedaluwarsa', color: 'var(--fault)' };
    if (c.days_left < 15) return { label: `${c.days_left} hari lagi`, color: 'var(--signal)' };
    return { label: `${c.days_left} hari lagi`, color: 'var(--live)' };
  }
</script>

<p class="eyebrow">Sertifikat</p>
<h1>{certs.length} sertifikat dikelola</h1>

<p class="note">
  Perpanjangan berjalan otomatis pada sisa 30 hari. Daftar ini hanya perlu
  disentuh saat menambah domain baru atau saat sebuah perpanjangan gagal.
</p>

<table>
  <thead>
    <tr><th>Slug</th><th>Domain</th><th>Masa berlaku</th><th></th></tr>
  </thead>
  <tbody>
    {#each certs as c (c.slug)}
      {@const s = stateOf(c)}
      <tr>
        <td class="data">{c.slug}</td>
        <td class="data domains">{c.domains.join(', ')}</td>
        <td class="data" style:color={s.color}>{s.label}</td>
        <td class="row-actions">
          <button onclick={() => renewCert(c.slug).then(reload)}>Perpanjang</button>
          <button class="danger" onclick={() => revokeCert(c.slug).then(reload)}>Hapus</button>
        </td>
      </tr>
      {#if c.last_error}
        <tr class="failure"><td colspan="4">Perpanjangan terakhir gagal: {c.last_error}</td></tr>
      {/if}
    {/each}
  </tbody>
</table>

<h2>Terbitkan sertifikat</h2>
<form onsubmit={issue}>
  <label>
    Domain — satu per baris
    <textarea bind:value={domainText} rows="3" placeholder="app.sekolah.id"></textarea>
  </label>

  <fieldset>
    <legend>Metode validasi</legend>
    <label class="check">
      <input type="radio" bind:group={method} value="http01" disabled={wildcard} />
      HTTP-01 — port 80 harus bisa dijangkau dari internet
    </label>
    <label class="check">
      <input type="radio" bind:group={method} value="dns01" />
      DNS-01 — satu-satunya cara untuk sertifikat wildcard
    </label>
    {#if method === 'dns01'}
      <label>
        Nama berkas token Cloudflare di /etc/hyperngx/secrets
        <input bind:value={cfTokenRef} />
      </label>
    {/if}
    {#if wildcard && method === 'http01'}
      <p class="error">Domain wildcard hanya bisa divalidasi lewat DNS-01.</p>
    {/if}
  </fieldset>

  {#if error}<p class="error">{error}</p>{/if}
  <button type="submit" disabled={busy}>{busy ? 'Menghubungi CA…' : 'Terbitkan'}</button>
</form>

<style>
  .note { color: var(--ink-soft); font-size: var(--step--1); max-width: 52ch; }
  table { width: 100%; border-collapse: collapse; margin: var(--s5) 0 var(--s6); }
  th {
    text-align: left; font-size: var(--step--1); font-weight: 500;
    color: var(--ink-soft); border-bottom: 2px solid var(--ink); padding: var(--s2);
  }
  td { padding: var(--s3) var(--s2); border-bottom: 1px solid var(--rule); font-size: var(--step--1); }
  .domains { color: var(--ink-soft); }
  .row-actions { display: flex; gap: var(--s2); justify-content: flex-end; }
  .failure td { color: var(--fault); border-bottom: 1px solid var(--rule); }
  form { display: grid; gap: var(--s4); max-width: 520px; margin-top: var(--s4); }
  fieldset { border: 1px solid var(--rule); border-radius: var(--radius); padding: var(--s4); display: grid; gap: var(--s2); }
  legend { font-size: var(--step--1); color: var(--ink-soft); padding: 0 var(--s2); }
  label { display: grid; gap: var(--s1); font-size: var(--step--1); color: var(--ink-soft); }
  label.check { display: flex; gap: var(--s2); align-items: baseline; }
  input, textarea {
    font: inherit; font-family: var(--font-mono); font-size: var(--step--1);
    padding: var(--s2) var(--s3); border: 1px solid var(--rule);
    border-radius: var(--radius); background: var(--paper-2); color: var(--ink);
  }
  button {
    font: inherit; font-size: var(--step--1); padding: var(--s2) var(--s4);
    border: 1px solid var(--ink); background: var(--ink); color: var(--paper);
    border-radius: var(--radius); cursor: pointer; justify-self: start;
  }
  button.danger { background: transparent; border-color: var(--fault); color: var(--fault); }
  .row-actions button:first-child { background: transparent; color: var(--ink); }
  .error { color: var(--fault); font-size: var(--step--1); margin: 0; }
</style>
