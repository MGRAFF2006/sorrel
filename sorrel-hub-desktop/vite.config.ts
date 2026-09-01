import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

const rootDir = dirname(fileURLToPath(import.meta.url));
const devHost = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  resolve: {
    preserveSymlinks: true,
  },
  server: {
    host: devHost || false,
    port: 1420,
    strictPort: true,
    fs: {
      allow: [resolve(rootDir, '..')],
    },
    hmr: devHost
      ? {
          protocol: 'ws',
          host: devHost,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
