import http from 'node:http';
import { readFile, realpath } from 'node:fs/promises';
import { extname, isAbsolute, join, relative, resolve, sep } from 'node:path';

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

export function createHubWebServer({ root, hubApiUrl, maxBodyBytes = 64 * 1024 * 1024, upstreamTimeoutMs = 30_000 }) {
  for (const [name, value] of Object.entries({ maxBodyBytes, upstreamTimeoutMs })) {
    if (!Number.isSafeInteger(value) || value < 1) throw new Error(`${name} must be a positive safe integer`);
  }
  root = resolve(root);
  const upstreamBaseUrl = hubApiUrl.replace(/\/$/, '');

  function withinRoot(base, path) {
    const fromRoot = relative(base, path);
    return fromRoot !== '..' && !fromRoot.startsWith(`..${sep}`) && !isAbsolute(fromRoot);
  }

  async function readStatic(path) {
    const [assetRoot, assetPath] = await Promise.all([realpath(root), realpath(path)]);
    if (!withinRoot(assetRoot, assetPath)) {
      const error = new Error('Forbidden');
      error.code = 'FORBIDDEN';
      throw error;
    }
    return readFile(assetPath);
  }

  async function serveStatic(request, response) {
    const url = new URL(request.url ?? '/', 'http://localhost');
    let pathname;
    try {
      pathname = decodeURIComponent(url.pathname);
    } catch {
      response.writeHead(400).end('Invalid URL');
      return;
    }
    if (pathname === '/') pathname = '/index.html';

    const filePath = resolve(root, `.${pathname}`);
    if (!withinRoot(root, filePath)) {
      response.writeHead(403).end('Forbidden');
      return;
    }

    try {
      const body = await readStatic(filePath);
      const type = CONTENT_TYPES[extname(filePath)] ?? 'application/octet-stream';
      response.writeHead(200, { 'content-type': type }).end(body);
    } catch (error) {
      if (error.code === 'FORBIDDEN') {
        response.writeHead(403).end('Forbidden');
        return;
      }
      if (!extname(filePath) || /^\/(?:projects|orgs)\/[^/]+\/?$/.test(pathname)) {
        try {
          const fallback = await readStatic(join(root, 'index.html'));
          response.writeHead(200, { 'content-type': CONTENT_TYPES['.html'] }).end(fallback);
          return;
        } catch (error) {
          if (error.code === 'FORBIDDEN') {
            response.writeHead(403).end('Forbidden');
            return;
          }
        }
      }
      response.writeHead(404, { 'content-type': 'text/plain' }).end('Not found');
    }
  }

  async function proxyApi(request, response) {
    const url = new URL(request.url ?? '/', 'http://localhost');
    const target = upstreamBaseUrl + url.pathname.replace(/^\/api/, '') + url.search;
    const controller = new AbortController();
    const requestClosed = () => {
      if (!request.complete) controller.abort();
    };
    const responseClosed = () => {
      if (!response.writableFinished) controller.abort();
    };
    request.once('close', requestClosed);
    response.once('close', responseClosed);
    let timeout;

    function rejectOversizedBody() {
      response.writeHead(413, { 'content-type': 'application/json', connection: 'close' });
      response.end(JSON.stringify({ error: { code: 'request_body_too_large', message: 'Request body exceeds the Hub safety limit' } }));
    }

    try {
      if (Number(request.headers['content-length']) > maxBodyBytes) {
        rejectOversizedBody();
        return;
      }
      const chunks = [];
      let size = 0;
      for await (const chunk of request.iterator({ destroyOnReturn: false })) {
        size += Buffer.byteLength(chunk);
        if (size > maxBodyBytes) {
          rejectOversizedBody();
          return;
        }
        chunks.push(chunk);
      }
      const body = chunks.length > 0 ? Buffer.concat(chunks) : undefined;
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
      for (const name of ['cookie', 'origin', 'sec-fetch-site']) {
        if (request.headers[name]) headers[name] = request.headers[name];
      }

      timeout = setTimeout(() => controller.abort(new Error('upstream timeout')), upstreamTimeoutMs);
      timeout.unref();

      const upstream = await fetch(target, {
        method: request.method,
        headers,
        body: request.method === 'GET' || request.method === 'HEAD' ? undefined : body,
        signal: controller.signal,
        redirect: 'manual',
      });
      const text = await upstream.text();
      const responseHeaders = {
        'content-type': upstream.headers.get('content-type') ?? 'application/json',
        'cache-control': upstream.headers.get('cache-control') ?? 'no-store',
      };
      const cookies = upstream.headers.getSetCookie();
      if (cookies.length) responseHeaders['set-cookie'] = cookies;
      if (upstream.headers.has('location')) responseHeaders.location = upstream.headers.get('location');
      for (const name of ['referrer-policy', 'x-content-type-options', 'x-frame-options']) {
        if (upstream.headers.has(name)) responseHeaders[name] = upstream.headers.get(name);
      }
      response.writeHead(upstream.status, responseHeaders);
      response.end(text);
    } catch {
      if (response.destroyed || (controller.signal.aborted && controller.signal.reason?.message !== 'upstream timeout')) return;
      response.writeHead(controller.signal.aborted ? 504 : 502, { 'content-type': 'application/json', 'cache-control': 'no-store' });
      response.end(
        JSON.stringify({
          error: {
            code: controller.signal.aborted ? 'hub_api_timeout' : 'hub_api_unreachable',
            message: controller.signal.aborted ? 'Hub API request timed out' : 'Could not reach sorrel-hub API',
          },
        }),
      );
    } finally {
      clearTimeout(timeout);
      request.off('close', requestClosed);
      response.off('close', responseClosed);
    }
  }

  return http.createServer({ requestTimeout: 30_000, headersTimeout: 15_000 }, (request, response) => {
    let url;
    try {
      url = new URL(request.url ?? '/', 'http://localhost');
    } catch {
      response.writeHead(400).end('Invalid URL');
      return;
    }
    const handler =
      url.pathname === '/api' || url.pathname.startsWith('/api/') ? proxyApi : serveStatic;
    handler(request, response).catch(() => {
      response.writeHead(500).end('server error');
    });
  });
}
