import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));

function pngHeader(relativePath: string) {
  const bytes = readFileSync(`${root}/${relativePath}`);
  assert.equal(bytes.subarray(1, 4).toString('ascii'), 'PNG');
  return {
    width: bytes.readUInt32BE(16),
    height: bytes.readUInt32BE(20),
    colorType: bytes.readUInt8(25),
  };
}

test('declares both mobile platforms and tablet support', () => {
  const config = JSON.parse(readFileSync(`${root}/app.json`, 'utf8')).expo;
  assert.deepEqual(config.platforms, ['ios', 'android']);
  assert.equal(config.orientation, 'default');
  assert.equal(config.ios.supportsTablet, true);
  assert.equal(config.ios.requireFullScreen, false);
  assert.equal(config.android.predictiveBackGestureEnabled, true);
});

test('ships store-sized icons with an opaque primary app icon', () => {
  assert.deepEqual(pngHeader('assets/icon.png'), {
    width: 1024,
    height: 1024,
    colorType: 2,
  });
  assert.deepEqual(pngHeader('assets/adaptive-icon.png'), {
    width: 1024,
    height: 1024,
    colorType: 6,
  });
});
