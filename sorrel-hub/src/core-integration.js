import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import { randomBytes } from 'node:crypto';
import { PublishedWriteDurabilityError } from './fs-sync-store.js';
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
export async function coreIntegration(payload, datasetLease) {
  const coreBytes = Buffer.from(JSON.stringify(payload));
  const bytes = datasetLease ? Buffer.from(JSON.stringify(datasetLease.envelope({ operation: 'core', request: payload }))) : coreBytes;
  if (coreBytes.length > MAX_BYTES) throw new HttpError(413, 'integration request exceeds safety limit', 'integration_limit');
  if (active >= 4) throw new HttpError(503, 'Core integration is busy', 'integration_unavailable');
  active += 1;
  try {
    return await new Promise((resolve, reject) => {
      const child = spawn(integrationBinary(), datasetLease ? datasetLease.clientArgs : [], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
      let failure;
      let count = 0;
      const chunks = [];
      const fail = message => { failure ??= new HttpError(503, message, 'integration_unavailable'); child.kill('SIGKILL'); datasetLease?.abort(); };
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
        if (datasetLease && result?.value) result = result.value;
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
  const token = randomBytes(32).toString('hex');
  let port;
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
        if (text.includes('\n')) {
          try { const ready = JSON.parse(text); if (ready.locked !== true || !Number.isSafeInteger(ready.port) || ready.port < 1 || ready.port > 65535) throw new Error(); port = ready.port; done(); }
          catch { done(new Error('invalid dataset writer lease response')); }
        } else if (text.length > 128) done(new Error('invalid dataset writer lease response'));
      });
      child.stderr.resume();
      child.stdin.on('error', () => { alive = false; });
      child.stdin.write(`${token}\n`);
    });
  } catch (error) { child.stdin.destroy(); child.kill(); await exited; throw error; }
  // The lease must not keep an embedded host alive after its last server closes.
  child.unref();
  for (const stream of [child.stdin, child.stdout, child.stderr]) stream.unref?.();
  let fault;
  const assert = () => {
    if (!alive || closed) throw new HttpError(503, 'filesystem writer lease lost; restart Hub', 'dataset_writer_lost');
  };
  const abort = () => { alive = false; child.kill('SIGKILL'); };
  const envelope = command => { assert(); return { token, command }; };
  const requestSync = command => {
    const input = JSON.stringify(envelope(command));
    if (Buffer.byteLength(input) > 32 * 1024 * 1024) throw new HttpError(413, 'protected write exceeds safety limit', 'dataset_write_limit');
    const result = spawnSync(integrationBinary(), ['client', String(port)], {
      input, timeout: 12000, maxBuffer: 32 * 1024 * 1024, windowsHide: true,
    });
    if (result.error || result.status !== 0) { abort(); throw new HttpError(503, 'protected dataset writer unavailable; restart Hub', 'dataset_writer_lost'); }
    let response;
    try { response = JSON.parse(result.stdout.toString('utf8')); }
    catch { abort(); throw new HttpError(503, 'protected dataset writer response invalid', 'dataset_writer_lost'); }
    if (response.error) {
      const error = Object.assign(new Error('protected filesystem write failed'), { code: response.error.code ?? 'EIO' });
      if (response.error.published) throw new PublishedWriteDurabilityError(error);
      throw error;
    }
    if (response.ok !== true) { abort(); throw new HttpError(503, 'protected dataset writer acknowledgement invalid', 'dataset_writer_lost'); }
  };
  return {
    pid: child.pid, roots: canonical, clientArgs: ['client', String(port)], assert, abort, envelope,
    covers(root) { return canonical.includes(fs.realpathSync(root)); },
    // Used only by disposable failure tests; release builds reject native faults.
    injectFault(target, stage) { fault = { target: path.resolve(target), stage }; },
    write(target, bytes, root, guard) {
      const canonicalRoot = fs.realpathSync(root);
      if (!canonical.includes(canonicalRoot)) throw new Error('write root lacks a dataset lease');
      const inside = filename => {
        const relative = path.relative(path.resolve(root), path.resolve(filename));
        if (!relative || relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) throw new Error('write is outside its root');
        return path.join(canonicalRoot, relative);
      };
      const nativeGuard = guard ? { ...guard, target: { ...guard.target, path: inside(guard.target.path) }, source: { ...guard.source, path: inside(guard.source.path) } } : undefined;
      const injected = fault?.target === path.resolve(target) ? fault.stage : undefined;
      if (injected) fault = undefined;
      const content = Buffer.from(bytes);
      if (content.length > 16 * 1024 * 1024) throw new HttpError(413, 'protected write exceeds safety limit', 'dataset_write_limit');
      requestSync({ operation: 'write', root: canonicalRoot, target: inside(target), hex: content.toString('hex'), guard: nativeGuard, fault: injected });
    },
    async close() { closed = true; child.ref(); child.stdin.end(); await exited; },
  };
}
