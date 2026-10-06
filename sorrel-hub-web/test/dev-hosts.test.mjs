import assert from 'node:assert/strict';
import http from 'node:http';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { createServer, loadConfigFromFile } from 'vite';

const configFile = fileURLToPath(new URL('../vite.config.ts', import.meta.url));

async function withHosts(allowedHosts, run) {
  const previous = { HOST: process.env.HOST, PORT: process.env.PORT, SORREL_HUB_ALLOWED_HOSTS: process.env.SORREL_HUB_ALLOWED_HOSTS };
  delete process.env.HOST;
  delete process.env.PORT;
  if (allowedHosts === undefined) delete process.env.SORREL_HUB_ALLOWED_HOSTS;
  else process.env.SORREL_HUB_ALLOWED_HOSTS = allowedHosts;
  try {
    await run();
  } finally {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

async function status(port, host) {
  return new Promise((resolve, reject) => {
    http.get({ hostname: '127.0.0.1', port, path: '/', headers: { host } }, (response) => {
      response.resume();
      response.on('end', () => resolve(response.statusCode));
      response.on('error', reject);
    }).on('error', reject);
  });
}

test('development and preview default to loopback and built-in host restrictions', async () => {
  await withHosts(undefined, async () => {
    const { config } = await loadConfigFromFile({ command: 'serve', mode: 'development' }, configFile);
    assert.equal(config.server.host, '127.0.0.1');
    assert.equal(config.preview.host, '127.0.0.1');
    assert.deepEqual(config.server.allowedHosts, []);
    assert.deepEqual(config.preview.allowedHosts, []);
  });
});

test('the actual Vite server rejects an arbitrary host and preserves explicit opt-ins', async () => {
  for (const [allowedHosts, expected] of [[undefined, 403], ['desktop', 403], ['all', 200], ['true', 200]]) {
    await withHosts(allowedHosts, async () => {
      const server = await createServer({ configFile, root: fileURLToPath(new URL('../', import.meta.url)), server: { host: '127.0.0.1', port: 0 }, logLevel: 'silent' });
      try {
        await server.listen();
        const address = server.httpServer.address();
        assert.ok(address && typeof address === 'object');
        assert.equal(await status(address.port, 'untrusted.example'), expected);
        assert.equal(await status(address.port, 'localhost'), 200);
        if (allowedHosts === 'desktop') assert.equal(await status(address.port, 'desktop'), 200);
      } finally {
        await server.close();
      }
    });
  }
});
