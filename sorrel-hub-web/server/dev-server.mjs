// Static + API-proxy server for the Sorrel Hub web interface.
//
// Serves `public/` and proxies `/api/*` to a running `sorrel-hub` backend so the
// browser can avoid CORS. Suitable for local development and container deploys
// (pair with the Hub API via HUB_API_URL).
//
// Environment:
//   PORT          port to listen on (default 5180)
//   HOST          bind address (default 0.0.0.0)
//   HUB_API_URL   base URL of the sorrel-hub API (default http://localhost:3000)

import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../public', import.meta.url));
const port = Number.parseInt(process.env.PORT ?? '5180', 10);
const host = process.env.HOST ?? '0.0.0.0';
const hubApiUrl = process.env.HUB_API_URL ?? 'http://localhost:3000';

const CONTENT_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
};

async function serveStatic(request, response) {
  const url = new URL(request.url ?? '/', 'http://localhost');
  let pathname = decodeURIComponent(url.pathname);
  if (pathname === '/') {
    pathname = '/index.html';
  }

  // Prevent path traversal.
  const safePath = normalize(pathname).replace(/^(\.\.[/\\])+/, '');
  const filePath = join(root, safePath);
  if (!filePath.startsWith(root)) {
    response.writeHead(403).end('Forbidden');
    return;
  }

  try {
    const body = await readFile(filePath);
    const type = CONTENT_TYPES[extname(filePath)] ?? 'application/octet-stream';
    response.writeHead(200, { 'content-type': type }).end(body);
  } catch {
    // SPA-ish fallback to index for unknown non-asset routes.
    if (!extname(filePath)) {
      try {
        const fallback = await readFile(join(root, 'index.html'));
        response
          .writeHead(200, { 'content-type': CONTENT_TYPES['.html'] })
          .end(fallback);
        return;
      } catch {
        /* fall through */
      }
    }
    response.writeHead(404, { 'content-type': 'text/plain' }).end('Not found');
  }
}

async function proxyApi(request, response) {
  const url = new URL(request.url ?? '/', 'http://localhost');
  const target = hubApiUrl.replace(/\/$/, '') + url.pathname.replace(/^\/api/, '') + url.search;

  const chunks = [];
  for await (const chunk of request) {
    chunks.push(chunk);
  }
  const body = chunks.length > 0 ? Buffer.concat(chunks) : undefined;

  try {
    const upstream = await fetch(target, {
      method: request.method,
      headers: { 'content-type': request.headers['content-type'] ?? 'application/json' },
      body: request.method === 'GET' || request.method === 'HEAD' ? undefined : body,
    });
    const text = await upstream.text();
    response.writeHead(upstream.status, {
      'content-type': upstream.headers.get('content-type') ?? 'application/json',
    });
    response.end(text);
  } catch (error) {
    response.writeHead(502, { 'content-type': 'application/json' });
    response.end(
      JSON.stringify({
        error: {
          code: 'hub_api_unreachable',
          message: `Could not reach sorrel-hub API at ${hubApiUrl}: ${error.message}`,
        },
      }),
    );
  }
}

const server = http.createServer((request, response) => {
  const url = new URL(request.url ?? '/', 'http://localhost');
  if (url.pathname === '/api' || url.pathname.startsWith('/api/')) {
    proxyApi(request, response).catch(() => {
      response.writeHead(500).end('proxy error');
    });
    return;
  }
  serveStatic(request, response).catch(() => {
    response.writeHead(500).end('server error');
  });
});

server.listen(port, host, () => {
  const address = server.address();
  const boundPort = typeof address === 'object' && address ? address.port : port;
  console.log(`sorrel-hub-web listening on http://${host}:${boundPort}`);
  console.log(`proxying /api/* -> ${hubApiUrl}`);
});

process.on('SIGTERM', () => server.close(() => process.exit(0)));
