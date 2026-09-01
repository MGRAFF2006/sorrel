import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

test('shared hub-ui api client sends acting-principal on mutations only', async () => {
  const api = await readFile(
    new URL('../../sorrel-hub-ui/src/api.ts', import.meta.url),
    'utf8',
  );
  assert.match(api, /let apiBase = '\/api'/);
  assert.match(api, /configureApiClient/);
  assert.match(api, /LOCAL_PRINCIPAL/);
  assert.match(api, /x-sorrel-acting-principal/);
  assert.match(api, /method !== 'GET'/);
  assert.match(api, /\/capabilities/);
});
