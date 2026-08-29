import { sveltekit } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    // Saat `npm run dev`, panggilan /api/v1 diteruskan ke hyperngx-api
    // yang jalan lokal, sehingga cookie sesi tetap same-origin.
    proxy: {
      '/api': { target: 'http://127.0.0.1:8081', changeOrigin: false }
    }
  }
});
