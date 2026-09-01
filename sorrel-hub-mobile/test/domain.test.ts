import assert from 'node:assert/strict';
import test from 'node:test';

import {
  isInsecureConnection,
  normalizeBaseUrl,
  PROPOSAL_TRANSITIONS,
  tabletColumns,
  unwrapList,
} from '../src/lib/domain';

test('normalizes a safe Hub origin and removes its trailing slash', () => {
  assert.equal(normalizeBaseUrl(' https://hub.example.test/ '), 'https://hub.example.test');
});

test('rejects credential-bearing and path-bearing Hub URLs', () => {
  assert.throws(() => normalizeBaseUrl('https://user:pass@hub.example.test'), /credentials/);
  assert.throws(() => normalizeBaseUrl('https://hub.example.test/api'), /base URL/);
});

test('marks plain HTTP as an explicitly insecure local connection', () => {
  assert.equal(isInsecureConnection('http://desktop:3000'), true);
  assert.equal(isInsecureConnection('https://hub.example.test'), false);
});

test('unwraps Hub collection response variants', () => {
  assert.deepEqual(unwrapList<{ id: string }>({ data: [{ id: 'one' }] }), [{ id: 'one' }]);
  assert.deepEqual(unwrapList<{ id: string }>({ repos: [{ id: 'repo' }] }), [{ id: 'repo' }]);
});

test('keeps client transitions aligned with the Hub lifecycle', () => {
  assert.deepEqual(PROPOSAL_TRANSITIONS.open, [
    'approved',
    'rejected',
    'merged',
    'closed',
    'draft',
  ]);
});

test('uses a second column only at tablet content widths', () => {
  assert.equal(tabletColumns(719), 1);
  assert.equal(tabletColumns(720), 2);
});
