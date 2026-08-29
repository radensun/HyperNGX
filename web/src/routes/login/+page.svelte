<script lang="ts">
  import { login } from '$lib/api';

  let username = $state('');
  let password = $state('');
  let totp = $state('');
  let needsTotp = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function submit(e: Event) {
    e.preventDefault();
    busy = true; error = null;
    try {
      await login(username, password, totp || undefined);
      location.href = '/';
    } catch {
      // Server sengaja memberi pesan yang sama untuk password salah dan
      // kode TOTP salah. Kalau field TOTP sudah terlihat, pesannya
      // dipertajam di sini tanpa membocorkan apa pun ke penyerang.
      error = needsTotp
        ? 'Login gagal. Periksa kembali password dan kode autentikator.'
        : 'Login gagal. Kalau akun Anda memakai autentikator, masukkan kodenya.';
      needsTotp = true;
    } finally {
      busy = false;
    }
  }
</script>

<div class="frame">
  <p class="eyebrow">HyperNGX</p>
  <h1>Masuk</h1>

  <form onsubmit={submit}>
    <label>
      Nama pengguna
      <input bind:value={username} autocomplete="username" required />
    </label>
    <label>
      Password
      <input type="password" bind:value={password} autocomplete="current-password" required />
    </label>
    {#if needsTotp}
      <label>
        Kode autentikator
        <input bind:value={totp} inputmode="numeric" autocomplete="one-time-code" maxlength="6" />
      </label>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}

    <button type="submit" disabled={busy}>{busy ? 'Memeriksa…' : 'Masuk'}</button>
  </form>
</div>

<style>
  .frame { max-width: 340px; margin: 12vh auto; }
  h1 { margin-bottom: var(--s5); }
  form { display: grid; gap: var(--s4); }
  label { display: grid; gap: var(--s1); font-size: var(--step--1); color: var(--ink-soft); }
  input {
    font: inherit; font-family: var(--font-mono);
    padding: var(--s2) var(--s3);
    border: 1px solid var(--rule); border-radius: var(--radius);
    background: var(--paper-2); color: var(--ink);
  }
  button {
    font: inherit; padding: var(--s3);
    background: var(--ink); color: var(--paper);
    border: none; border-radius: var(--radius); cursor: pointer;
  }
  button:disabled { opacity: 0.6; cursor: default; }
  .error { color: var(--fault); font-size: var(--step--1); margin: 0; }
</style>
