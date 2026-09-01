import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';

const CONTENT_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
  '.map': 'application/json; charset=utf-8',
};

export function createHubWebServer({ root, hubApiUrl }) {
  const upstreamBaseUrl = hubApiUrl.replace(/\/$/, '');

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
          // Fall through to the not-found response.
        }
      }
      response.writeHead(404, { 'content-type': 'text/plain' }).end('Not found');
    }
  }

  async function proxyApi(request, response) {
    const url = new URL(request.url ?? '/', 'http://localhost');
    const target = upstreamBaseUrl + url.pathname.replace(/^\/api/, '') + url.search;
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
      if (request.headers.authorization) {
        headers.authorization = request.headers.authorization;
      }

      const upstream = await fetch(target, {
        method: request.method,
        headers,
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

  return http.createServer((request, response) => {
    const url = new URL(request.url ?? '/', 'http://localhost');
    const handler =
      url.pathname === '/api' || url.pathname.startsWith('/api/') ? proxyApi : serveStatic;
    handler(request, response).catch(() => {
      response.writeHead(500).end('server error');
    });
  });
}
