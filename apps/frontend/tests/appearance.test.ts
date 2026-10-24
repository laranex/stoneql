import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const css = readFileSync(new URL('../src/style.css', import.meta.url), 'utf8');
function tokens(selector: string) {
  const block = css.slice(css.indexOf(selector)).split('}')[0]!;
  return Object.fromEntries(
    [...block.matchAll(/--([\w-]+):\s*(#[\da-f]{6})\s*;/gi)].map((m) => [m[1], m[2]]),
  );
}
function luminance(hex: string) {
  const rgb = hex
    .slice(1)
    .match(/../g)!
    .map((n) => parseInt(n, 16) / 255)
    .map((n) => (n <= 0.04045 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4));
  return rgb[0]! * 0.2126 + rgb[1]! * 0.7152 + rgb[2]! * 0.0722;
}
function contrast(a: string, b: string) {
  const x = luminance(a),
    y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
for (const theme of [':root {', '.dark {']) {
  test(`${theme} primary and secondary text maintain readable contrast on UI surfaces`, () => {
    const t = tokens(theme);
    for (const foreground of ['text', 'muted', 'accent'])
      for (const background of [
        'bg',
        'panel',
        'sidebar',
        'toolbar',
        'stripe',
        'selection',
        'accent-bg',
      ])
        assert.ok(
          contrast(t[foreground]!, t[background]!) >= 4.5,
          `${foreground} on ${background}`,
        );
    for (const state of ['success', 'danger'])
      assert.ok(contrast(t[state]!, t[state + '-bg']!) >= 4.5, state);
    assert.ok(contrast(t['on-primary']!, t.primary!) >= 4.5, 'primary button');
    assert.ok(contrast(t['on-primary']!, t['primary-hover']!) >= 4.5, 'primary button hover');
    assert.ok(contrast(t.warning!, t.bg!) >= 4.5, 'warning text');
    assert.notEqual(t['footer-bg'], t.bg);
    assert.notEqual(t['footer-bg'], t.sidebar);
    assert.ok(contrast(t['footer-text']!, t['footer-bg']!) >= 4.5, 'footer text and icons');
    assert.ok(contrast(t.accent!, t['footer-bg']!) >= 4.5, 'footer hover state');
    for (const name of ['keyword', 'string', 'number', 'type', 'comment']) {
      for (const background of ['bg', 'panel', 'selection']) {
        assert.ok(
          contrast(t['syntax-' + name]!, t[background]!) >= 4.5,
          `SQL ${name} on ${background}`,
        );
      }
    }
  });
}
test('font sizes are centralized and no screen shrinks text with pixel declarations', () => {
  assert.match(css, /--font-body: 14px/);
  assert.match(css, /--font-heading: 13px/);
  assert.match(css, /--font-title: 20px/);
  assert.match(css, /--font-small: 12px/);
  assert.doesNotMatch(css, /font-size:\s*\d/);
});

test('Light and Dark define matching semantic palettes with distinct working surfaces', () => {
  const light = tokens(':root {');
  const dark = tokens('.dark {');
  assert.deepEqual(Object.keys(light).sort(), Object.keys(dark).sort());
  for (const t of [light, dark]) {
    assert.equal(new Set([t.bg, t.sidebar, t.toolbar, t.stripe]).size, 4);
    assert.notEqual(t['accent-bg'], t.panel);
    assert.notEqual(t.selection, t['accent-bg']);
  }
});
test('H1/H5 compact grid owns scrolling, retains sticky headers and bounds inline editors',()=>{
 assert.match(css,/\.browse-content\s*\{[^}]*overflow:\s*hidden/s);
 assert.match(css,/\.browse-content > \.data-grid-wrap\s*\{[^}]*min-height:\s*0;[^}]*overflow:\s*auto/s);
 assert.match(css,/\.data-grid-wrap thead\s*\{[^}]*position:\s*sticky;[^}]*top:\s*0/s);
 assert.match(css,/\.cell-editor\s*\{[^}]*position:\s*fixed;[^}]*max-height:[^}]*overflow:\s*auto/s);
});
for(const selector of [':root {','.dark {']) test(`${selector} form boundaries have at least 3:1 contrast`,()=>{
 const t=tokens(selector);assert.ok(contrast(t['control-border']!,t.input!)>=3);
});
