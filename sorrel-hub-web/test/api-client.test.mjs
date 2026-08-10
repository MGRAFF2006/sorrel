import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';

import { apiRequest, LOCAL_PRINCIPAL } from '../public/app.js';

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

test('mutation requests send the SDK acting-principal header', async () => {
  const requests = [];
  globalThis.fetch = async (url, init) => {
    requests.push({ url, init });
    return new Response(JSON.stringify({ data: {} }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  };

  await apiRequest('POST', '/projects', { name: 'test' });
  await apiRequest('PATCH', '/admin/proposals/prop_test', { status: 'approved' });

  assert.equal(requests.length, 2);
  for (const { init } of requests) {
    assert.equal(
      init.headers['x-sorrel-acting-principal'],
      JSON.stringify(LOCAL_PRINCIPAL),
    );
  }
});

test('read requests do not claim an acting principal', async () => {
  let request;
  globalThis.fetch = async (url, init) => {
    request = { url, init };
    return new Response(JSON.stringify({ data: [] }), { status: 200 });
  };

  await apiRequest('GET', '/projects');

  assert.equal(request.url, '/api/projects');
  assert.equal(request.init.headers['x-sorrel-acting-principal'], undefined);
});
