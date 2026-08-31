import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { test } from 'node:test';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function eventTarget(properties = {}) {
  const listeners = new Map();
  return Object.assign(properties, {
    addEventListener(type, listener) {
      const handlers = listeners.get(type) ?? [];
      handlers.push(listener);
      listeners.set(type, handlers);
    },
    emit(type, event = {}) {
      for (const listener of listeners.get(type) ?? []) listener(event);
    },
  });
}

function classes() {
  const values = new Set();
  return {
    add(name) {
      values.add(name);
    },
    contains(name) {
      return values.has(name);
    },
    toggle(name, force) {
      const enabled = force ?? !values.has(name);
      if (enabled) values.add(name);
      else values.delete(name);
      return enabled;
    },
  };
}

function element(properties = {}) {
  const attributes = new Map();
  return eventTarget(Object.assign(properties, {
    getAttribute(name) {
      return attributes.get(name) ?? null;
    },
    setAttribute(name, value) {
      attributes.set(name, String(value));
    },
  }));
}

async function loadSite() {
  const source = await readFile(resolve(ROOT, 'site.js'), 'utf8');
  const nav = element();
  const navToggle = element({ focusCount: 0, focus() { this.focusCount += 1; } });
  const themeToggle = element();
  const header = element({
    classList: classes(),
    querySelector(selector) {
      return selector === '.nav' ? nav : null;
    },
  });
  const root = element();
  root.setAttribute('data-theme', 'dark');
  const desktop = eventTarget({ matches: false });
  const document = eventTarget({
    documentElement: root,
    querySelector(selector) {
      return new Map([
        ['.site-header', header],
        ['.nav-toggle', navToggle],
        ['.theme-toggle', themeToggle],
      ]).get(selector) ?? null;
    },
  });
  const window = eventTarget({
    scrollY: 0,
    matchMedia() {
      return desktop;
    },
  });
  const stored = new Map();
  const localStorage = {
    setItem(key, value) {
      stored.set(key, value);
    },
  };

  vm.runInNewContext(source, { document, localStorage, window });
  return { desktop, document, header, nav, navToggle, root, stored, themeToggle };
}

test('compact navigation keeps its state and accessible label in sync', async () => {
  const site = await loadSite();
  assert.equal(site.header.classList.contains('has-nav-toggle'), true);
  assert.equal(site.navToggle.getAttribute('aria-expanded'), 'false');
  assert.equal(site.navToggle.getAttribute('aria-label'), 'Open navigation');

  site.navToggle.emit('click');
  assert.equal(site.header.classList.contains('is-nav-open'), true);
  assert.equal(site.navToggle.getAttribute('aria-expanded'), 'true');
  assert.equal(site.navToggle.getAttribute('aria-label'), 'Close navigation');

  site.document.emit('keydown', { key: 'Escape' });
  assert.equal(site.header.classList.contains('is-nav-open'), false);
  assert.equal(site.navToggle.focusCount, 1);

  site.navToggle.emit('click');
  site.nav.emit('click', { target: { closest: () => ({}) } });
  assert.equal(site.header.classList.contains('is-nav-open'), false);
});

test('theme toggle announces its next action and persists the choice', async () => {
  const site = await loadSite();
  assert.equal(site.themeToggle.getAttribute('aria-label'), 'Switch to light theme');

  site.themeToggle.emit('click');
  assert.equal(site.root.getAttribute('data-theme'), 'light');
  assert.equal(site.themeToggle.getAttribute('aria-label'), 'Switch to dark theme');
  assert.equal(site.stored.get('sorrel-theme'), 'light');
});
