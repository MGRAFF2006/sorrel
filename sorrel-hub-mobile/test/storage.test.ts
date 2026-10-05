import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import ts from 'typescript';

import type * as Storage from '../src/lib/storage';

function storage() {
  const values = new Map<string, string>();
  let failedKey: string | undefined;
  const secureStore = {
    getItemAsync: async (key: string) => values.get(key) ?? null,
    deleteItemAsync: async (key: string) => { values.delete(key); },
    setItemAsync: async (key: string, value: string) => {
      if (key === failedKey) throw new Error('Keystore unavailable');
      values.set(key, value);
    },
  };
  const source = readFileSync(new URL('../src/lib/storage.ts', import.meta.url), 'utf8');
  const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
  const exports = {};
  vm.runInNewContext(code, {
    exports,
    require(name: string) {
      assert.equal(name, 'expo-secure-store');
      return secureStore;
    },
  });
  return { api: exports as typeof Storage, failWrites: (key: string) => { failedKey = key; } };
}

const first = { baseUrl: 'https://first.example.test', principal: { type: 'user', id: 'local' } };
const second = { ...first, baseUrl: 'https://second.example.test' };

test('preserves saved bearer credentials only for the same Hub', async () => {
  const { api } = storage();
  await api.saveConnection(first, { accessToken: 'first-token' });
  assert.equal(await api.connectionAccessToken(first.baseUrl, { preserveAccessToken: true }), 'first-token');
  assert.equal(await api.connectionAccessToken(second.baseUrl, { preserveAccessToken: true }), undefined);
  assert.equal(await api.saveConnection(first, { preserveAccessToken: true }), 'first-token');
  assert.equal(await api.saveConnection(second, { preserveAccessToken: true }), undefined);
  assert.equal((await api.loadConnection()).accessToken, undefined);
});

test('a failed credential write never pairs a new Hub with the previous token', async () => {
  const { api, failWrites } = storage();
  await api.saveConnection(first, { accessToken: 'first-token' });
  failWrites('sorrel.hub.mobile.access-token.v1');
  await assert.rejects(api.saveConnection(second, { accessToken: 'second-token' }), /Keystore/);
  const loaded = await api.loadConnection();
  assert.ok(loaded.connection === null || loaded.connection.baseUrl === first.baseUrl);
});
