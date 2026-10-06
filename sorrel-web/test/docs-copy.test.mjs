import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import vm from 'node:vm';

function element() {
  const attributes = new Map();
  const listeners = new Map();
  return {
    children: [], textContent: '', dataset: {},
    classList: { add() {} },
    setAttribute(name, value) { attributes.set(name, value); },
    getAttribute(name) { return attributes.get(name) ?? null; },
    append(...children) { this.children.push(...children); },
    insertBefore(child) { this.children.unshift(child); },
    addEventListener(name, handler) { listeners.set(name, handler); },
    click() { return listeners.get('click')(); },
  };
}

async function enhanced(raw) {
  const copied = [];
  const code = Object.assign(element(), { textContent: raw });
  const pre = Object.assign(element(), { querySelector: () => code });
  const document = {
    getElementById: () => null,
    querySelector: () => null,
    querySelectorAll: selector => selector.startsWith('pre') ? [pre] : [],
    createElement: () => element(),
  };
  vm.runInNewContext(await readFile(new URL('../docs/docs.js', import.meta.url), 'utf8'), {
    document, window: { setTimeout() {} },
    navigator: { clipboard: { async writeText(text) { copied.push(text); } } },
  });
  return { copied, code, blockCopy: pre.children[0].children[1] };
}

for (const [name, firstCommand] of [['cli', 'sorrel init'], ['workflows', 'npm run vault -- import .env']]) {
  test(`${name} command examples copy without a literal shell prompt`, async () => {
    const html = await readFile(new URL(`../docs/${name}.html`, import.meta.url), 'utf8');
    const blocks = [...html.matchAll(/<pre><code>([\s\S]*?)<\/code><\/pre>/g)].map(match => match[1]);
    assert.ok(blocks.length > 0);
    for (const block of blocks) {
      const raw = block.replace(/&gt;/g, '>').replace(/&lt;/g, '<').replace(/&quot;/g, '"').replace(/&amp;/g, '&');
      const page = await enhanced(raw);
      await page.blockCopy.click();
      assert.doesNotMatch(page.copied[0], /^\$ /m);
      for (const [index, row] of page.code.children.entries()) {
        await row.children[2].click();
        assert.equal(page.copied.at(-1), raw.replace(/\n$/, '').split('\n')[index]);
        assert.doesNotMatch(page.copied.at(-1), /^\$ /);
      }
    }
    assert.ok(blocks.some(block => block.startsWith(firstCommand)));
  });
}

test('copy controls preserve literal dollar signs in code and data', async () => {
  const raw = 'console.log("$HOME")\n$ literal data';
  const page = await enhanced(raw);
  await page.blockCopy.click();
  assert.equal(page.copied[0], raw);
  await page.code.children[1].children[2].click();
  assert.equal(page.copied[1], '$ literal data');
});
