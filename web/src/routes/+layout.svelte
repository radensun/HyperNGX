<script lang="ts">
  import '../app.css';
  import { page } from '$app/state';
  import { me, logout } from '$lib/api';

  let { children } = $props();
  let user = $state<{ username: string; role: string } | null>(null);

  const bare = $derived(page.url.pathname === '/login');
  const nav = [
    { href: '/', label: 'Jalur' },
    { href: '/certificates', label: 'Sertifikat' },
    { href: '/generations', label: 'Riwayat konfigurasi' }
  ];

  $effect(() => {
    if (bare) return;
    me().then((u) => (user = u)).catch(() => {});
  });

  async function signOut() {
    await logout();
    location.href = '/login';
  }
</script>

<div class="shell">
  {#if !bare}
    <header>
      <span class="mark">HyperNGX</span>
      <nav>
        {#each nav as item}
          <a href={item.href} class:current={page.url.pathname === item.href}>{item.label}</a>
        {/each}
      </nav>
      {#if user}
        <span class="who">
          <span class="eyebrow">{user.username}</span>
          <button onclick={signOut}>Keluar</button>
        </span>
      {/if}
    </header>
  {/if}
  <main>{@render children()}</main>
</div>

<style>
  .shell { max-width: 1180px; margin: 0 auto; padding: 0 var(--s4); }
  header {
    display: flex; align-items: baseline; gap: var(--s6);
    padding: var(--s5) 0; border-bottom: 2px solid var(--ink);
  }
  .mark {
    font-family: var(--font-display); font-weight: 600; font-size: var(--step-1);
    letter-spacing: -0.02em;
  }
  nav { display: flex; gap: var(--s5); flex-wrap: wrap; }
  nav a {
    font-size: var(--step--1); color: var(--ink-soft); text-decoration: none;
    padding-bottom: 2px; border-bottom: 1px solid transparent;
  }
  nav a:hover, nav a.current { color: var(--ink); border-bottom-color: var(--ink); }
  .who { margin-left: auto; display: flex; align-items: baseline; gap: var(--s3); }
  .who button {
    font: inherit; font-size: var(--step--1); background: none; border: none;
    color: var(--ink-soft); cursor: pointer; padding: 0;
    border-bottom: 1px solid var(--rule);
  }
  .who button:hover { color: var(--ink); }
  main { padding: var(--s5) 0 var(--s6); }
</style>
