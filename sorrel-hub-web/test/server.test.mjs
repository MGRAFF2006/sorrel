import assert from 'node:assert/strict';
import http from 'node:http';
import { once } from 'node:events';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import { createHubWebServer } from '../server/hub-web-server.mjs';

async function listen(server) {
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  assert.ok(address && typeof address === 'object');
  return `http://127.0.0.1:${address.port}`;
}

async function close(server) {
  server.close();
  await once(server, 'close');
}

test('shared server serves the SPA and forwards Hub auth headers', async () => {
  const root = await mkdtemp(join(tmpdir(), 'sorrel-hub-web-'));
  await writeFile(join(root, 'index.html'), '<main id="root">Sorrel Hub</main>');

  let received;
  const upstream = http.createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    received = {
      url: request.url,
      method: request.method,
      authorization: request.headers.authorization,
      actingPrincipal: request.headers['x-sorrel-acting-principal'],
      body: Buffer.concat(chunks).toString('utf8'),
    };
    response.writeHead(201, { 'content-type': 'application/json' });
    response.end('{"created":true}');
  });

  let server;
  try {
    const upstreamUrl = await listen(upstream);
    server = createHubWebServer({ root, hubApiUrl: upstreamUrl });
    const serverUrl = await listen(server);

    const page = await fetch(`${serverUrl}/projects/project_1`).then((response) =>
      response.text(),
    );
    assert.match(page, /Sorrel Hub/);

    const response = await fetch(`${serverUrl}/api/projects?source=test`, {
      method: 'POST',
      headers: {
        authorization: 'Bearer test-token',
        'content-type': 'application/json',
        'x-sorrel-acting-principal': '{"type":"user","id":"local"}',
      },
      body: '{"name":"Shared server"}',
    });
    assert.equal(response.status, 201);
    assert.deepEqual(await response.json(), { created: true });
    assert.deepEqual(received, {
      url: '/projects?source=test',
      method: 'POST',
      authorization: 'Bearer test-token',
      actingPrincipal: '{"type":"user","id":"local"}',
      body: '{"name":"Shared server"}',
    });
  } finally {
    if (server) await close(server);
    if (upstream.listening) await close(upstream);
    await rm(root, { recursive: true, force: true });
  }
});
