import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { HttpError } from './http.js';

const MAX_BYTES = 2 * 1024 * 1024;
let active = 0;
export function integrationBinary() {
  const root = fileURLToPath(new URL('../../', import.meta.url));
  const target = process.env.CARGO_TARGET_DIR ? path.resolve(process.env.CARGO_TARGET_DIR) : path.join(root, 'target');
  return process.env.SORREL_HUB_CORE_INTEGRATION_BIN ?? path.join(target, 'debug', `sorrel-core-integration${process.platform === 'win32' ? '.exe' : ''}`);
}
export async function coreIntegration(payload) {
  const bytes = Buffer.from(JSON.stringify(payload));
  if (bytes.length > MAX_BYTES) throw new HttpError(413, 'integration request exceeds safety limit', 'integration_limit');
  if (active >= 4) throw new HttpError(503, 'Core integration is busy', 'integration_unavailable');
  active += 1;
  try {
    return await new Promise((resolve, reject) => {
      const child = spawn(integrationBinary(), [], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
      let failure;
      let count = 0;
      const chunks = [];
      const fail = message => { failure ??= new HttpError(503, message, 'integration_unavailable'); child.kill('SIGKILL'); };
      const timer = setTimeout(() => fail('Core integration timed out'), 10000);
      child.on('error', () => fail('Core integration executable is unavailable'));
      child.stdin.on('error', () => fail('Core integration request could not be delivered'));
      for (const stream of [child.stdout, child.stderr]) stream.on('data', chunk => {
        count += chunk.length;
        if (count > MAX_BYTES) return fail('Core integration output exceeds safety limit');
        if (stream === child.stdout) chunks.push(chunk);
      });
      child.on('close', code => {
        clearTimeout(timer);
        if (failure) return reject(failure);
        let result;
        try { result = JSON.parse(Buffer.concat(chunks).toString('utf8')); }
        catch { return reject(new HttpError(503, 'Core integration response is invalid', 'integration_unavailable')); }
        if (result?.error) return reject(new HttpError(409, result.error.message, 'integration_blocked'));
        if (code !== 0 || !result || typeof result !== 'object') return reject(new HttpError(503, 'Core integration failed', 'integration_unavailable'));
        resolve(result);
      });
      child.stdin.end(bytes);
    });
  } finally { active -= 1; }
}

/** OS leases cover canonical aliases of *both* roots before server hydration. */
export async function acquireDatasetWriter(roots) {
  const canonical = [...new Set(roots.map(root => {
    fs.mkdirSync(root, { recursive: true });
    return fs.realpathSync(root);
  }))].sort();
  const child = spawn(integrationBinary(), ['lock', ...canonical], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
  let alive = true;
  let closed = false;
  const exited = new Promise(resolve => child.once('close', () => { alive = false; resolve(); }));
  try {
    await new Promise((resolve, reject) => {
      let text = '';
      const timer = setTimeout(() => reject(new Error('dataset writer lease timed out')), 5000);
      const done = error => { clearTimeout(timer); error ? reject(error) : resolve(); };
      child.once('error', () => done(new Error('Core integration executable is required for filesystem writer locking')));
      child.once('close', () => done(new Error('filesystem dataset already has a Hub writer or cannot be locked')));
      child.stdout.on('data', chunk => {
        text += chunk.toString('utf8');
        if (text === '{"locked":true}\n') done();
        else if (text.length > 128) done(new Error('invalid dataset writer lease response'));
      });
      child.stderr.resume();
      child.stdin.on('error', () => { alive = false; });
    });
  } catch (error) { child.stdin.destroy(); child.kill(); await exited; throw error; }
  // The lease must not keep an embedded host alive after its last server closes.
  child.unref();
  for (const stream of [child.stdin, child.stdout, child.stderr]) stream.unref?.();
  return {
    roots: canonical,
    assert() {
      if (!alive || closed) throw new HttpError(503, 'filesystem writer lease lost; restart Hub', 'dataset_writer_lost');
    },
    covers(root) { return canonical.includes(fs.realpathSync(root)); },
    async close() { closed = true; child.ref(); child.stdin.end(); await exited; },
  };
}
