import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { JSDOM } from 'jsdom';

const css = await readFile(new URL('../src/styles/hub.css', import.meta.url), 'utf8');
const dom = new JSDOM(`<style>${css}</style>`);
const rules = [...dom.window.document.styleSheets[0].cssRules];
function style(selector) {
  const rule = rules.find(rule => rule.selectorText === selector);
  assert.ok(rule, `Missing CSS rule: ${selector}`);
  return rule.style;
}
const root = style(':root');
function color(value) {
  if (value.startsWith('var(')) return color(root.getPropertyValue(value.slice(4, -1)).trim());
  if (value.startsWith('#')) {
    const hex = value.length === 4 ? value.slice(1).split('').map(digit => digit + digit).join('') : value.slice(1);
    return [0, 2, 4].map(offset => parseInt(hex.slice(offset, offset + 2), 16)).concat(1);
  }
  const match = value.match(/^rgba?\(([^)]+)\)$/);
  assert.ok(match, `Unsupported color: ${value}`);
  const channels = match[1].split(',').map(Number);
  return channels.length === 3 ? [...channels, 1] : channels;
}
function over(foreground, background) {
  return foreground.slice(0, 3).map((channel, index) => channel * foreground[3] + background[index] * (1 - foreground[3])).concat(1);
}
function luminance(rgb) {
  return rgb.slice(0, 3).reduce((sum, channel, index) => {
    const value = channel / 255;
    return sum + (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4) * [0.2126, 0.7152, 0.0722][index];
  }, 0);
}
function readable(foreground, background, label) {
  const values = [luminance(foreground), luminance(background)].sort((a, b) => a - b);
  const ratio = (values[1] + 0.05) / (values[0] + 0.05);
  assert.ok(ratio >= 4.5, `${label}: ${ratio.toFixed(3)}:1 is below 4.5:1 for normal text`);
}
const surfaces = ['--bg', '--bg-deep', '--bg-elev', '--bg-card', '--bg-hover'].map(name => [name, color(root.getPropertyValue(name).trim())]);

test('small work-card metadata remains readable on normal and hovered cards', () => {
  for (const selector of ['.work-card-top', '.work-card footer']) {
    for (const card of ['.work-card', '.work-card:hover']) {
      readable(color(style(selector).color), color(style(card).background), `${selector} on ${card}`);
    }
  }
});

test('closed and failed status text remains readable on dark surfaces and selected rows', () => {
  for (const selector of ['.pill-closed, .pill-archived', '.pill-rejected, .pill-failed, .pill-cancelled']) {
    for (const [name, background] of surfaces) {
      const foreground = color(style(selector).color);
      readable(foreground, background, `${selector} on ${name}`);
      // Selected rows overlay the page/surface backgrounds; card hover replaces its background.
      if (['--bg', '--bg-deep', '--bg-elev'].includes(name)) {
        readable(foreground, over(color(style('.list-row:hover, .list-row.selected').background), background), `${selector} on selected ${name}`);
      }
    }
  }
});

dom.window.close();
