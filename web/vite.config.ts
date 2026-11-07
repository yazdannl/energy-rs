import tailwindcss from '@tailwindcss/vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// `base: './'` keeps every asset URL relative so the build works behind the Home Assistant
// ingress path (`/api/hassio_ingress/<token>/...`) without knowing it at build time.
export default defineConfig({
  base: './',
  plugins: [svelte(), tailwindcss()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
