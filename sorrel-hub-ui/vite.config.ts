import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

function resolveAllowedHosts() {
  const raw = process.env.SORREL_HUB_ALLOWED_HOSTS;
  if (!raw || raw === 'all' || raw === 'true') {
    return true;
  }
  return raw
    .split(',')
    .map((host: string) => host.trim())
    .filter(Boolean);
}

export default defineConfig({
  plugins: [solid()],
  server: {
    host: process.env.HOST ?? '0.0.0.0',
    port: 5181,
    allowedHosts: resolveAllowedHosts(),
    proxy: {
      '/api': {
        target: process.env.HUB_API_URL ?? 'http://127.0.0.1:3000',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
  build: {
    lib: {
      entry: 'src/index.tsx',
      name: 'SorrelHubUi',
      formats: ['es'],
      fileName: 'sorrel-hub-ui',
    },
    rollupOptions: {
      external: ['solid-js', 'solid-js/web', '@solidjs/router', 'convex', 'convex/browser', 'convex/server'],
    },
  },
});
