import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import vm from 'node:vm';

async function docs() {
  const requests = [];
  const content = {
    innerHTML: '',
    setAttribute() {},
    removeAttribute() {},
    querySelectorAll: () => [],
  };
  const document = {
    getElementById: () => content,
    querySelectorAll: () => [],
    querySelector: () => null,
  };
  const entries = [];
  const context = vm.createContext({
    document,
    window: { location: { href: 'https://sorrel.test/docs/guides.html', search: '' }, addEventListener() {} },
    URL,
    URLSearchParams,
    history: { replaceState: (_state, _title, url) => entries.push(String(url)), pushState: (_state, _title, url) => entries.push(String(url)) },
    fetch: () => new Promise((resolve, reject) => requests.push({ resolve, reject })),
  });
  vm.runInContext(await readFile(new URL('../docs/docs.js', import.meta.url), 'utf8'), context);
  return {
    content, document, entries, requests,
    load: (name) => vm.runInContext(`loadDoc('${name}', 'push')`, context),
    render: (source) => vm.runInContext(`renderMarkdown(${JSON.stringify(source)})`, context),
  };
}

test('escaped pipes in Markdown tables stay inside their cell', async () => {
  const page = await docs();
  const html = page.render('| Area | Commands |\n| --- | --- |\n| CLI | **`secret list\\|sync\\|get`** |');
  assert.equal((html.match(/<td>/g) ?? []).length, 2);
  assert.match(html, /<td><strong><code>secret list\|sync\|get<\/code><\/strong><\/td>/);
});

test('an older document response cannot overwrite a newer selection or history', async () => {
  const page = await docs();
  const current = page.load('DEVELOPMENT.md');
  page.requests[1].resolve({ ok: true, text: async () => '# Development' });
  await current;
  page.requests[0].resolve({ ok: true, text: async () => '# Stale status' });
  await new Promise((resolve) => setImmediate(resolve));
  assert.match(page.content.innerHTML, /Development/);
  assert.equal(page.document.title, 'DEVELOPMENT — Sorrel docs');
  assert.deepEqual(page.entries, ['https://sorrel.test/docs/guides.html?doc=DEVELOPMENT.md']);
});

test('an older request failure cannot replace the current document with an error', async () => {
  const page = await docs();
  const current = page.load('DEVELOPMENT.md');
  page.requests[1].resolve({ ok: true, text: async () => '# Development' });
  await current;
  page.requests[0].reject(new Error('Old request failed'));
  await new Promise((resolve) => setImmediate(resolve));
  assert.match(page.content.innerHTML, /Development/);
  assert.doesNotMatch(page.content.innerHTML, /Old request failed/);
});
