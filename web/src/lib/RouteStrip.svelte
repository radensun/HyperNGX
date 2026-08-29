<script lang="ts">
  // Elemen tanda tangan HyperNGX: satu proxy host digambar sebagai satu
  // jalur kabel. Dibaca kiri ke kanan seperti diagram: domain masuk,
  // melewati simpul TLS, keluar ke upstream. Semua kolom monospace
  // sehingga daftar host tersusun rapi jadi satu gambar rangkaian.
  import type { ProxyHost } from './api';
  let { host }: { host: ProxyHost } = $props();

  const certColor = $derived(
    host.cert_state === 'valid' ? 'var(--live)'
    : host.cert_state === 'expiring' ? 'var(--signal)'
    : host.cert_state === 'expired' ? 'var(--fault)' : 'var(--ink-soft)'
  );
  const healthColor = $derived(
    host.health === 'up' ? 'var(--live)'
    : host.health === 'degraded' ? 'var(--signal)'
    : host.health === 'down' ? 'var(--fault)' : 'var(--ink-soft)'
  );
</script>

<a class="strip" href="/hosts/{host.id}" class:off={!host.enabled}>
  <span class="eyebrow id">{String(host.id).padStart(6, '0')}</span>

  <span class="domain mono">{host.domains[0]}</span>
  {#if host.domains.length > 1}
    <span class="more eyebrow">+{host.domains.length - 1}</span>
  {/if}

  <span class="wire" aria-hidden="true"></span>

  <span class="node tls" style:border-color={certColor} style:color={certColor}>
    {host.ssl_enabled ? 'TLS' : 'PLAIN'}
    {#if host.cert_days_left !== null}<em>{host.cert_days_left}h</em>{/if}
  </span>

  <span class="wire" aria-hidden="true"></span>

  <span class="upstream mono">
    {host.targets[0].address}:{host.targets[0].port}
    {#if host.targets.length > 1}<span class="more eyebrow">×{host.targets.length}</span>{/if}
  </span>

  <span class="health" style:background={healthColor}
        title="Status upstream: {host.health}"></span>
</a>

<style>
  .strip {
    display: grid;
    grid-template-columns: 5.5rem minmax(8rem, 1fr) auto 1fr auto 1fr minmax(9rem, auto) 10px;
    align-items: center;
    gap: var(--s2);
    padding: var(--s3) var(--s4);
    border-bottom: 1px solid var(--rule);
    color: inherit;
    text-decoration: none;
  }
  .strip:hover { background: var(--paper-2); }
  .strip.off { opacity: 0.45; }

  .domain { font-size: var(--step-0); font-weight: 500; overflow: hidden; text-overflow: ellipsis; }
  .more { margin-left: var(--s1); }

  /* Kabelnya adalah garis putus-putus, bukan panah dekoratif:
     ia mewakili hop nyata dalam rantai proxy. */
  .wire {
    height: 1px;
    background: repeating-linear-gradient(90deg, var(--rule) 0 6px, transparent 6px 10px);
  }

  .node {
    font-family: var(--font-mono);
    font-size: var(--step--1);
    padding: 2px var(--s2);
    border: 1px solid;
    border-radius: var(--radius);
    white-space: nowrap;
  }
  .node em { font-style: normal; opacity: 0.7; margin-left: var(--s1); }

  .upstream { font-size: var(--step--1); color: var(--ink-soft); }

  .health { width: 10px; height: 10px; border-radius: 50%; justify-self: end; }

  @media (max-width: 720px) {
    .strip { grid-template-columns: 1fr 10px; row-gap: var(--s1); }
    .wire, .id { display: none; }
  }
</style>
