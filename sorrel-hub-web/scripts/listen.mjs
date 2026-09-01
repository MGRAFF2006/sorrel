#!/usr/bin/env node
/**
 * Start hub-web (built dist/) on an ephemeral port and print one JSON line
 * with the URL. Requires HUB_API_URL to point at a running sorrel-hub.
 * Run `npm run build` first so `dist/` exists.
 *
 * Stdout: {"url":"http://127.0.0.1:<port>","pid":N}
 */

import { access } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createHubWebServer } from '../server/hub-web-server.mjs';

const root = fileURLToPath(new URL('../dist', import.meta.url));
const hubApiUrl = process.env.HUB_API_URL ?? 'http://127.0.0.1:3000';

try {
  await access(join(root, 'index.html'));
} catch {
  console.error('dist/ missing — run `npm run build` in sorrel-hub-web first');
  process.exit(1);
}

const server = createHubWebServer({ root, hubApiUrl });

server.listen(0, '127.0.0.1', () => {
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  process.stdout.write(
    `${JSON.stringify({ url: `http://127.0.0.1:${port}`, pid: process.pid })}\n`,
  );
});

function shutdown() {
  server.close(() => process.exit(0));
}
process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
