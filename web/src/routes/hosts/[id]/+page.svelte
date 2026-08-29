<script lang="ts">
  import { page } from '$app/state';
  import { createHost, updateHost, deleteHost, getHost, listCerts,
           type HostInput, type Certificate } from '$lib/api';

  const id = $derived(page.params.id);
  const isNew = $derived(id === 'new');

  let form = $state<HostInput>({
    domains: [], scheme: 'http', targets: [{ address: '', port: 8080 }],
    load_balance: 'round_robin', certificate_id: null,
    ssl_enabled: false, force_ssl: true, http2: true, http3: false,
    hardening: true, block_common_exploits: true,
    client_max_body_size: '64m', advanced_config: '', enabled: true
  });
  let domainText = $state('');
  let certs = $state<Certificate[]>([]);
  let error = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => { listCerts().then((c) => (certs = c)).catch(() => {}); });

  $effect(() => {
    if (isNew) return;
    getHost(Number(id)).then((h) => {
      form = { ...form, ...(h as Partial<HostInput>) };
      domainText = (h.domains as string[]).join('\n');
    }).catch((e) => (error = (e as Error).message));
  });

  function addTarget() { form.targets = [...form.targets, { address: '', port: 8080 }]; }
  function removeTarget(i: number) { form.targets = form.targets.filter((_, n) => n !== i); }

  async function save(e: Event) {
    e.preventDefault();
    busy = true; error = null;
    form.domains = domainText.split('\n').map((d) => d.trim()).filter(Boolean);
    try {
      if (isNew) await createHost(form);
      else await updateHost(Number(id), form);
      location.href = '/';
    } catch (err) {
      error = (err as Error).message;
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!confirm('Hapus jalur ini? Trafik ke domainnya akan berhenti diteruskan.')) return;
    await deleteHost(Number(id));
    location.href = '/';
  }
</script>

<p class="eyebrow">{isNew ? 'Jalur baru' : `Jalur ${String(id).padStart(6, '0')}`}</p>
<h1>{isNew ? 'Tambah jalur' : 'Ubah jalur'}</h1>

<form onsubmit={save}>
  <section>
    <h2>Masuk</h2>
    <label>
      Domain — satu per baris
      <textarea bind:value={domainText} rows="3" placeholder="app.sekolah.id"></textarea>
    </label>
  </section>

  <section>
    <h2>Tujuan</h2>
    <label class="inline">
      Protokol ke upstream
      <select bind:value={form.scheme}><option>http</option><option>https</option></select>
    </label>

    {#each form.targets as target, i}
      <div class="target">
        <input bind:value={target.address} placeholder="10.0.0.5" />
        <input type="number" bind:value={target.port} min="1" max="65535" />
        {#if form.targets.length > 1}
          <button type="button" onclick={() => removeTarget(i)}>Hapus</button>
        {/if}
      </div>
    {/each}
    <button type="button" class="ghost" onclick={addTarget}>Tambah target</button>

    {#if form.targets.length > 1}
      <label class="inline">
        Pembagian beban
        <select bind:value={form.load_balance}>
          <option value="round_robin">Bergiliran</option>
          <option value="least_conn">Koneksi paling sedikit</option>
          <option value="ip_hash">Tetap per IP klien</option>
        </select>
      </label>
    {/if}
  </section>

  <section>
    <h2>TLS</h2>
    <label class="check"><input type="checkbox" bind:checked={form.ssl_enabled} /> Layani lewat HTTPS</label>
    {#if form.ssl_enabled}
      <label class="inline">
        Sertifikat
        <select bind:value={form.certificate_id}>
          <option value={null}>— pilih sertifikat —</option>
          {#each certs as c}<option value={c.id}>{c.slug}</option>{/each}
        </select>
      </label>
      <label class="check"><input type="checkbox" bind:checked={form.force_ssl} /> Alihkan HTTP ke HTTPS</label>
      <label class="check"><input type="checkbox" bind:checked={form.http2} /> HTTP/2</label>
      <label class="check">
        <input type="checkbox" bind:checked={form.http3} /> HTTP/3 (QUIC)
        <small>Sebagian jaringan menjatuhkan UDP/443. Uji dulu sebelum dipakai produksi.</small>
      </label>
    {/if}
  </section>

  <section>
    <h2>Proteksi</h2>
    <label class="check"><input type="checkbox" bind:checked={form.hardening} /> Batasi metode HTTP dan blokir berkas sensitif</label>
    <label class="check"><input type="checkbox" bind:checked={form.block_common_exploits} /> Tolak pola serangan umum</label>
    <label class="inline">
      Ukuran unggahan maksimum
      <input bind:value={form.client_max_body_size} />
    </label>
  </section>

  <details>
    <summary>Konfigurasi nginx lanjutan</summary>
    <p class="note">
      Ditempel ke dalam blok <code>server</code>. Direktif yang bisa membaca
      berkas sistem atau memuat modul ditolak.
    </p>
    <textarea bind:value={form.advanced_config} rows="6" spellcheck="false"></textarea>
  </details>

  {#if error}<p class="error">{error}</p>{/if}

  <div class="actions">
    <button type="submit" disabled={busy}>{busy ? 'Menerapkan…' : 'Simpan dan terapkan'}</button>
    {#if !isNew}<button type="button" class="danger" onclick={remove}>Hapus jalur</button>{/if}
    <a href="/">Batal</a>
  </div>
</form>

<style>
  form { display: grid; gap: var(--s6); max-width: 640px; margin-top: var(--s5); }
  section { display: grid; gap: var(--s3); }
  h2 { font-size: var(--step-1); padding-bottom: var(--s1); border-bottom: 1px solid var(--rule); }
  label { display: grid; gap: var(--s1); font-size: var(--step--1); color: var(--ink-soft); }
  label.inline { grid-template-columns: 1fr auto; align-items: center; }
  label.check { display: flex; align-items: baseline; gap: var(--s2); }
  label.check small { display: block; color: var(--ink-soft); }
  input, select, textarea {
    font: inherit; font-family: var(--font-mono); font-size: var(--step--1);
    padding: var(--s2) var(--s3); border: 1px solid var(--rule);
    border-radius: var(--radius); background: var(--paper-2); color: var(--ink);
  }
  .target { display: grid; grid-template-columns: 1fr 6rem auto; gap: var(--s2); }
  button, .actions a {
    font: inherit; font-size: var(--step--1); padding: var(--s2) var(--s4);
    border: 1px solid var(--ink); background: var(--ink); color: var(--paper);
    border-radius: var(--radius); cursor: pointer; text-decoration: none;
  }
  button.ghost, .actions a { background: transparent; color: var(--ink); justify-self: start; }
  button.danger { background: transparent; border-color: var(--fault); color: var(--fault); }
  .actions { display: flex; gap: var(--s3); align-items: center; }
  .note { font-size: var(--step--1); color: var(--ink-soft); }
  .error { color: var(--fault); font-size: var(--step--1); }
</style>
