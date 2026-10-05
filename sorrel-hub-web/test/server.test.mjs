import assert from 'node:assert/strict';
import http from 'node:http';
import { once } from 'node:events';
import { mkdtemp, mkdir, rm, symlink, writeFile } from 'node:fs/promises';
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


test('static serving refuses traversal and symlinks outside the asset root', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'sorrel-hub-web-boundary-'));
  const root = join(directory, 'dist');
  await mkdir(root);
  await writeFile(join(root, 'index.html'), 'Hub');
  await writeFile(join(root, 'asset.js'), 'safe');
  await writeFile(join(directory, 'private.txt'), 'private');
  await symlink(join(directory, 'private.txt'), join(root, 'leak.txt'));
  await symlink(join(root, 'asset.js'), join(root, 'alias.js'));
  const server = createHubWebServer({ root, hubApiUrl: 'http://127.0.0.1:1' });
  try {
    const url = await listen(server);
    assert.equal((await fetch(`${url}/leak.txt`)).status, 403);
    assert.equal((await fetch(`${url}/..%2fprivate.txt`)).status, 403);
    assert.equal(await (await fetch(`${url}/alias.js`)).text(), 'safe');
    assert.equal((await fetch(`${url}/%zz`)).status, 400);
    assert.equal((await fetch(`${url}//`)).status, 400);
    assert.equal(await (await fetch(`${url}/`)).text(), 'Hub');
  } finally {
    await close(server);
    await rm(directory, { recursive: true, force: true });
  }
});

test('unreachable upstream errors do not disclose URL credentials', async () => {
  const root = await mkdtemp(join(tmpdir(), 'sorrel-hub-web-errors-'));
  const server = createHubWebServer({ root, hubApiUrl: 'http://fixture-user:fixture-password@127.0.0.1:1' });
  try {
    const url = await listen(server);
    const response = await fetch(`${url}/api/healthz`);
    assert.equal(response.status, 502);
    const text = await response.text();
    assert.equal(text.includes('fixture-user'), false);
    assert.equal(text.includes('fixture-password'), false);
    assert.equal(JSON.parse(text).error.code, 'hub_api_unreachable');
  } finally {
    await close(server);
    await rm(root, { recursive: true, force: true });
  }
});
