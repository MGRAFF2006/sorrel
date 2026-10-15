import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createInterface } from 'node:readline';
import test from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
function alive(pid) {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === 'ESRCH') return false; throw error; }
}
async function gone(pid) {
  for (let attempt = 0; attempt < 100 && alive(pid); attempt++) await delay(20);
  assert.equal(alive(pid), false, `owned child ${pid} survived cancellation`);
}

for (const entry of ['pilot-workflow.mjs', 'happy-path.mjs']) {
  for (const [signal, code] of [['SIGINT', 130], ['SIGTERM', 143]]) {
    test(`${entry} ${signal} awaits Hub/lease shutdown and removes disposable data`, { skip: process.platform === 'win32', timeout: 60000 }, async t => {
      const temporary = await mkdtemp(path.join(tmpdir(), 'sorrel-pilot-cancel-'));
      const child = spawn(process.execPath, [`tests/e2e/${entry}`, '--pilot-only'], {
        cwd: ROOT, env: { ...process.env, TMPDIR: temporary }, stdio: ['ignore', 'pipe', 'pipe'],
      });
      const closed = new Promise(resolve => child.once('close', (exitCode, exitSignal) => resolve({ code: exitCode, signal: exitSignal })));
      let output = '';
      child.stderr.on('data', chunk => { output = (output + chunk).slice(-8000); });
      const lines = createInterface({ input: child.stdout });
      let hub;
      let descendants = [];
      t.after(async () => {
        if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
        await closed;
        for (const pid of [...descendants, ...(hub ? [hub.pid] : [])]) if (alive(pid)) process.kill(pid, 'SIGKILL');
        lines.close();
        await rm(temporary, { recursive: true, force: true });
      });
      hub = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`pilot readiness timed out: ${output}`)), 30000);
        child.once('error', error => { clearTimeout(timer); reject(error); });
        child.once('exit', () => { clearTimeout(timer); reject(new Error(`pilot exited before ready: ${output}`)); });
        lines.on('line', line => {
          const marker = 'disposable Hub ready ';
          const index = line.indexOf(marker);
          if (index < 0) return;
          clearTimeout(timer);
          try { resolve(JSON.parse(line.slice(index + marker.length))); } catch (error) { reject(error); }
        });
      });
      assert.ok(hub.root.startsWith(`${temporary}${path.sep}`));
      assert.equal(alive(hub.pid), true);
      if (process.platform === 'linux') {
        descendants = (await readFile(`/proc/${hub.pid}/task/${hub.pid}/children`, 'utf8')).trim().split(/\s+/).filter(Boolean).map(Number);
        assert.ok(descendants.length > 0, 'real Core writer lease must be running');
      }
      assert.equal((await stat(path.join(hub.root, 'sync'))).isDirectory(), true);
      child.kill(signal);
      assert.deepEqual(await closed, { code, signal: null }, output);
      await gone(hub.pid);
      for (const pid of descendants) await gone(pid);
      await assert.rejects(stat(hub.root), { code: 'ENOENT' });
    });
  }
}
