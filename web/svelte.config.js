import adapter from '@sveltejs/adapter-static';

// SPA murni: seluruh UI di-build jadi berkas statis, disajikan langsung
// oleh hyperngx-api lewat ServeDir. Tidak ada runtime Node di server.
export default {
  kit: {
    adapter: adapter({ fallback: 'index.html', precompress: true }),
    prerender: { entries: [] },
    csp: {
      mode: 'auto',
      directives: {
        'default-src': ['self'],
        'script-src': ['self'],
        'style-src': ['self', 'unsafe-inline'],
        'connect-src': ['self'],
        'frame-ancestors': ['none'],
        'base-uri': ['none']
      }
    }
  }
};
