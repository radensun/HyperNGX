// Panel admin sepenuhnya berjalan di browser: tidak ada server-side
// rendering, dan tidak ada prerender karena setiap halaman butuh sesi.
export const ssr = false;
export const prerender = false;
export const trailingSlash = 'never';
