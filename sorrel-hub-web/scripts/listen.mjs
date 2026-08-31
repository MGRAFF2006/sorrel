#!/usr/bin/env node
/**
 * Start hub-web (built dist/) on an ephemeral port and print one JSON line
 * with the URL. Requires HUB_API_URL to point at a running sorrel-hub.
 * Run `npm run build` first so `dist/` exists.
 *
 * Stdout: {"url":"http://127.0.0.1:<port>","pid":N}
 */

import http from 'node:http';
import { access, readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../dist', import.meta.url));
const hubApiUrl = process.env.HUB_API_URL ?? 'http://127.0.0.1:3000';

const CONTENT_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.map': 'application/json; charset=utf-8',
};

try {
  await access(join(root, 'index.html'));
} catch {
  console.error('dist/ missing — run `npm run build` in sorrel-hub-web first');
  process.exit(1);
}

async function serveStatic(request, response) {
  const url = new URL(request.url ?? '/', 'http://localhost');
  let pathname = decodeURIComponent(url.pathname);
  if (pathname === '/') {
    pathname = '/index.html';
  }
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
    if (!extname(filePath)) {
      try {
        const fallback = await readFile(join(root, 'index.html'));
        response.writeHead(200, { 'content-type': CONTENT_TYPES['.html'] }).end(fallback);
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
  const target =
    hubApiUrl.replace(/\/$/, '') + url.pathname.replace(/^\/api/, '') + url.search;
  const chunks = [];
  for await (const chunk of request) {
    chunks.push(chunk);
  }
  const body = chunks.length > 0 ? Buffer.concat(chunks) : undefined;
  try {
    const headers = {};
    if (request.headers['content-type']) {
      headers['content-type'] = request.headers['content-type'];
    }
    if (request.headers['x-sorrel-acting-principal']) {
      headers['x-sorrel-acting-principal'] = request.headers['x-sorrel-acting-principal'];
    }
    const upstream = await fetch(target, {
      method: request.method,
      headers,
      body: body && request.method !== 'GET' && request.method !== 'HEAD' ? body : undefined,
    });
    const buf = Buffer.from(await upstream.arrayBuffer());
    response.writeHead(upstream.status, {
      'content-type': upstream.headers.get('content-type') ?? 'application/octet-stream',
    });
    response.end(buf);
  } catch (error) {
    response
      .writeHead(502, { 'content-type': 'text/plain' })
      .end(`Bad gateway: ${error instanceof Error ? error.message : String(error)}`);
  }
}

const server = http.createServer((request, response) => {
  const url = new URL(request.url ?? '/', 'http://localhost');
  if (url.pathname.startsWith('/api/') || url.pathname === '/api') {
    void proxyApi(request, response);
    return;
  }
  void serveStatic(request, response);
});

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
