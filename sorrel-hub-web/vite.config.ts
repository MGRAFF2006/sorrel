import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const rootDir = dirname(fileURLToPath(import.meta.url));

/** Tailscale / LAN Host headers (e.g. http://desktop:5180). Override with SORREL_HUB_ALLOWED_HOSTS=a,b or =all. */
function resolveAllowedHosts() {
  const raw = process.env.SORREL_HUB_ALLOWED_HOSTS;
  if (!raw || raw === 'all' || raw === 'true') {
    return true;
  }
  return raw
    .split(',')
    .map((host) => host.trim())
    .filter(Boolean);
}

const allowedHosts = resolveAllowedHosts();

export default defineConfig({
  plugins: [solid()],
  resolve: {
    preserveSymlinks: true,
  },
  server: {
    host: process.env.HOST ?? '0.0.0.0',
    port: Number.parseInt(process.env.PORT ?? '5180', 10),
    allowedHosts,
    fs: {
      allow: [resolve(rootDir, '..')],
    },
    proxy: {
      '/api': {
        target: process.env.HUB_API_URL ?? 'http://127.0.0.1:3000',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
  preview: {
    host: process.env.HOST ?? '0.0.0.0',
    port: Number.parseInt(process.env.PORT ?? '5180', 10),
    allowedHosts,
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
