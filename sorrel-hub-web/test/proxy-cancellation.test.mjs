import assert from 'node:assert/strict';
import http from 'node:http';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { createHubWebServer } from '../server/hub-web-server.mjs';

async function listen(server) {
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return `http://127.0.0.1:${server.address().port}`;
}

async function close(server) {
  const closed = once(server, 'close');
  server.close();
  server.closeAllConnections();
  await closed;
}

for (const stage of ['headers', 'body']) {
  test(`browser cancellation closes upstream while waiting for ${stage}`, async (t) => {
    const root = await mkdtemp(join(tmpdir(), 'sorrel-web-proxy-cancel-'));
    let ready;
    const received = new Promise(resolve => { ready = resolve; });
    let cancelled;
    const disconnected = new Promise(resolve => { cancelled = resolve; });
    const upstream = http.createServer((_request, response) => {
      response.once('close', cancelled);
      if (stage === 'body') {
        response.writeHead(200, { 'content-type': 'application/json' });
        response.write('{"pending":');
      }
      if (stage === 'headers') ready();
    });
    let proxy;
    let timer;
    const controller = new AbortController();
    try {
      const upstreamUrl = await listen(upstream);
      if (stage === 'body') {
        // Observe the real response reader so cancellation happens after headers.
        const readText = Response.prototype.text;
        t.mock.method(Response.prototype, 'text', function (...args) {
          if (this.url === `${upstreamUrl}/slow`) ready();
          return readText.apply(this, args);
        });
      }
      proxy = createHubWebServer({ root, hubApiUrl: upstreamUrl });
      const proxyUrl = await listen(proxy);
      const request = fetch(`${proxyUrl}/api/slow`, { signal: controller.signal });
      const aborted = assert.rejects(request, { name: 'AbortError' });
      await received;
      controller.abort();
      await aborted;
      const closed = await Promise.race([
        disconnected.then(() => true),
        new Promise(resolve => { timer = setTimeout(() => resolve(false), 2_000); }),
      ]);
      assert.equal(closed, true, 'abandoned browser request must cancel upstream work');
    } finally {
      clearTimeout(timer);
      controller.abort();
      if (proxy) await close(proxy);
      if (upstream.listening) await close(upstream);
      await rm(root, { recursive: true, force: true });
    }
  });
}
