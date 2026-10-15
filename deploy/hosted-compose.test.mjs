import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

const cwd = new URL('..', import.meta.url);
const compose = spawnSync('docker', ['compose', 'version'], { encoding: 'utf8' });
const skip = compose.status !== 0 && !process.env.CI && 'Install Docker Compose 2.24.4+ to render hosted deployment configuration';

for (const mode of ['preview', 'hosted TLS', 'hosted tunnel']) {
  test(`${mode} renders the intended API/web bindings with SORREL_BIND_ADDRESS=0.0.0.0`, { skip }, t => {
    assert.equal(compose.status, 0, 'CI requires Docker Compose 2.24.4+; missing or unsupported Compose must fail');
    const hosted = mode !== 'preview';
    const tunnel = mode === 'hosted tunnel';
    const fixture = mkdtempSync(join(tmpdir(), 'sorrel-compose-'));
    t.after(() => rmSync(fixture, { recursive: true, force: true }));
    const envFile = join(fixture, 'empty.env');
    writeFileSync(envFile, '');
    const args = ['compose', '--env-file', envFile, '-f', 'deploy/sorrel-server.compose.yml'];
    if (hosted) args.push('-f', 'deploy/sorrel-hosted.compose.yml');
    if (tunnel) args.push('-f', 'deploy/sorrel-tunnel.compose.yml');
    const result = spawnSync('docker', [...args, 'config', '--format', 'json'], {
      cwd, encoding: 'utf8',
      env: {
        // Fixture configuration only: never read private deployment .env values.
        PATH: process.env.PATH, HOME: process.env.HOME, SYSTEMROOT: process.env.SYSTEMROOT,
        SORREL_VERSION: 'test', SORREL_BIND_ADDRESS: '0.0.0.0',
        SORREL_HUB_PORT: '13000', SORREL_HUB_WEB_PORT: '15180',
        SORREL_HUB_HOST: 'hub.example.com', SORREL_HUB_PUBLIC_URL: 'https://hub.example.com',
        WORKOS_API_KEY: 'test-placeholder', WORKOS_CLIENT_ID: 'test-placeholder',
        WORKOS_COOKIE_PASSWORD: 'test-placeholder-at-least-32-characters',
        SORREL_TUNNEL_NETWORK: 'test-tunnel', SORREL_HUB_DATA_PATH: '/tmp/test-hub-data',
      },
    });
    assert.equal(result.status, 0, result.stderr);
    const { services } = JSON.parse(result.stdout);
    for (const [name, target, published] of [['hub', 3000, '13000'], ['hub-web', 5180, '15180']]) {
      assert.equal(services[name].ports.length, 1, `${name} must not retain a public inherited binding`);
      assert.equal(services[name].ports[0].host_ip, hosted ? '127.0.0.1' : '0.0.0.0');
      assert.equal(services[name].ports[0].target, target);
      assert.equal(services[name].ports[0].published, published);
    }
    if (!hosted) return;
    assert.equal(services.hub.environment.SORREL_HUB_AUTH, 'workos');
    assert.equal(services.hub.environment.SORREL_HUB_LOCAL_DEMO, '0');
    assert.equal(services.hub.environment.SORREL_HUB_BOOTSTRAP_GRANTS, '0');
    assert.equal(services.hub.environment.SORREL_HUB_ALLOW_INSECURE_DEV_AUTH, '0');
    assert.ok(services.hub.volumes.some(volume => volume.target === '/app/config' && volume.read_only));
    if (tunnel) assert.equal(services.caddy, undefined);
  });
}
