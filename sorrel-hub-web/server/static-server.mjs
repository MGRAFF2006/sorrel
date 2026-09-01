// Production static server for the Vite-built Hub web host.
//
// Serves `dist/` and proxies `/api/*` to a running `sorrel-hub` backend.
//
// Environment:
//   PORT          port to listen on (default 5180)
//   HOST          bind address (default 0.0.0.0)
//   HUB_API_URL   base URL of the sorrel-hub API (default http://localhost:3000)

import { fileURLToPath } from 'node:url';

import { createHubWebServer } from './hub-web-server.mjs';

const root = fileURLToPath(new URL('../dist', import.meta.url));
const port = Number.parseInt(process.env.PORT ?? '5180', 10);
const host = process.env.HOST ?? '0.0.0.0';
const hubApiUrl = process.env.HUB_API_URL ?? 'http://localhost:3000';

const server = createHubWebServer({ root, hubApiUrl });

server.listen(port, host, () => {
  const address = server.address();
  const boundPort = typeof address === 'object' && address ? address.port : port;
  console.log(`sorrel-hub-web listening on http://${host}:${boundPort}`);
  console.log(`proxying /api/* -> ${hubApiUrl}`);
});

process.on('SIGTERM', () => server.close(() => process.exit(0)));
