// "Display what the database stores, not what the application thinks it means."
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createPinia } from 'pinia';
import { nextTick } from 'vue';
import { hostRenderer, hostNode, findAll, textContent, loadVue, loadModule, type HostNode } from './helpers/vueHost.ts';
const preferences = new Map<string, string>();
let osDark = true;
const listeners = new Set<(event: any) => void>();
const classes = new Set<string>();
Object.assign(globalThis, {
  Document: class {},
  ShadowRoot: class {},
  requestAnimationFrame: (fn: any) => setTimeout(fn, 0),
  getComputedStyle: () => ({
    transitionDelay: '0s',
    transitionDuration: '0s',
    animationDelay: '0s',
    animationDuration: '0s',
  }),
  localStorage: {
    getItem: (key: string) => preferences.get(key) ?? null,
    setItem: (key: string, value: string) => preferences.set(key, value),
  },
  window: {
    getComputedStyle: () => ({
      transitionDelay: '0s',
      transitionDuration: '0s',
      animationDelay: '0s',
      animationDuration: '0s',
    }),
    innerHeight: 1000,
    confirm: () => true,
    addEventListener() {},
    removeEventListener() {},
    matchMedia: () => ({
      get matches() {
        return osDark;
      },
      addEventListener(_name: string, fn: any) {
        listeners.add(fn);
      },
      removeEventListener(_name: string, fn: any) {
        listeners.delete(fn);
      },
    }),
  },
  document: {
    body: { offsetHeight: 0, style: {} },
    documentElement: {
      style: {},
      classList: {
        toggle: (name: string, on: boolean) => (on ? classes.add(name) : classes.delete(name)),
      },
    },
  },
});
const App = await loadVue('../../src/App.vue');
const VALUES = ['active', 'inactive', 'admin', 'customer', 'suspended', 'enabled'];
const TONE = /\b(enum-values?|success|warning|inactive|status-badge|role-badge|email-cell|date-cell)\b/;
/** Element/class structure of a cell, ignoring text, so differently-valued cells can be compared. */
const shape = (n: HostNode): string =>
  n.type === '#text' || n.type === '#comment' ? '' : `${n.type}[${n.props.class ?? ''}](${n.children.map(shape).join('')})`;
function assertPlain(grid: HostNode, columnIndexes: number[], label: string) {
  for (const n of findAll(grid, () => true))
    assert.ok(!TONE.test(String(n.props.class ?? '')), `${label}: semantic class ${n.props.class}`);
  const rows = findAll(grid, (n) => n.type === 'tr').slice(1, 1 + VALUES.length);
  for (const columnIndex of columnIndexes) {
    const cells = rows.map((r) => r.children.filter((n) => n.type === 'td')[columnIndex]!);
    assert.deepEqual(cells.map((c) => textContent(c).trim()), VALUES, `${label}: exact stored text`);
    assert.equal(new Set(cells.map(shape)).size, 1, `${label}: identical rendering for every value`);
  }
}
for (const engine of ['MySQL', 'PostgreSQL'])
  test(`${engine}: VARCHAR and ENUM values render as identical plain text in Browse and Search grids (SQL result grid: tests/browser/plain-values.spec.ts)`, async () => {
    preferences.clear();
    const pinia = createPinia(), root = hostNode('root'), app = hostRenderer.createApp(App);
    app.use(pinia);
    app.mount(root);
    const { useWorkspace } = await loadModule('../../src/stores/workspace.ts');
    const store = useWorkspace(pinia);
    if (store.connection.engine !== engine) {
      const profile = store.connections.find((c: any) => c.engine === engine);
      assert.ok(profile, engine);
      store.activeId = profile.id;
      await nextTick();
    }
    const names = store.table.columns.map((c: any) => c.name);
    const role = names.indexOf('role'), status = names.indexOf('status');
    assert.ok(role >= 0 && status >= 0);
    VALUES.forEach((value, i) => Object.assign(store.table.rows[i], { role: value, status: value }));
    await nextTick();
    const table = (cls: string) => findAll(root, (n) => n.type === 'table' && String(n.props.class).includes(cls))[0]!;
    // Browse cells are offset by the selection and actions columns.
    const browse = table('browse-grid');
    const offset = findAll(browse, (n) => n.type === 'tr')[1]!.children.filter((n) => n.type === 'td').length - names.length;
    assertPlain(browse, [role + offset, status + offset], `${engine} Browse`);
    const button = (label: string) => findAll(root, (n) => n.type === 'button' && textContent(n).trim() === label)[0]!;
    await button('Search').props.onClick();
    await nextTick();
    findAll(root, (n) => n.type === 'form')[0]!.props.onSubmit({ preventDefault() {} });
    await nextTick();
    assertPlain(findAll(root, (n) => n.type === 'table' && n.props.class === 'data-grid')[0]!, [role, status], `${engine} Search`);
    app.unmount();
  });

// Stored value → expected cell tokens: text runs, display-only markers (M:) or the SQL NULL marker.
const INVISIBLE: [unknown, string[]][] = [
  [null, ['NULL!']],
  ['NULL', ['T:NULL']],
  ['', []],
  ['a\tb', ['T:a', 'M:⇥', 'T:b']],
  ['one\ntwo', ['T:one', 'M:↵', 'T:two']],
  ['cr\rhere', ['T:cr', 'M:␍', 'T:here']],
  ['win\r\nline', ['T:win', 'M:␍↵', 'T:line']],
  ['\t x  y \r\n\n', ['M:⇥', 'T: x  y ', 'M:␍↵', 'M:↵']],
];
const LABELS: Record<string, [string, string]> = { '⇥': ['\t', 'tab'], '↵': ['\n', 'line feed'], '␍': ['\r', 'carriage return'], '␍↵': ['\r\n', 'CRLF'] };
/** Reads a cell back as tokens, asserting markers and SQL NULL carry spoken names and literal text carries no NULL styling. */
function cellTokens(td: HostNode): string[] {
  const out: string[] = [];
  const walk = (n: HostNode) => {
    if (n.type === '#text') return void (n.text && out.push(`T:${n.text}`));
    if (n.type === '#comment' || n.props.role === 'dialog') return;
    if (String(n.props.class ?? '').includes('null-value')) {
      assert.deepEqual([n.props.role, n.props['aria-label']], ['img', 'SQL NULL']);
      return void out.push(`${textContent(n)}!`);
    }
    if (n.props.class === 'invisible-marker') {
      const [raw, label] = LABELS[textContent(n)]!;
      assert.deepEqual([n.props.role, n.props['aria-label'], n.props['data-raw'], n.props['aria-hidden']], ['img', label, raw, undefined]);
      return void out.push(`M:${textContent(n)}`);
    }
    if (n.text) out.push(`T:${n.text}`);
    n.children.forEach(walk);
  };
  td.children.forEach(walk);
  // Adjacent text nodes are one visible run.
  return out.reduce<string[]>((all, t) => {
    const last = all.at(-1);
    if (t.startsWith('T:') && last?.startsWith('T:')) all[all.length - 1] = last + t.slice(2);
    else all.push(t);
    return all;
  }, []);
}
function assertInvisible(grid: HostNode, columnIndex: number, label: string) {
  const rows = findAll(grid, (n) => n.type === 'tr').slice(1, 1 + INVISIBLE.length);
  rows.forEach((r, i) => {
    const [value, tokens] = INVISIBLE[i]!;
    const td = r.children.filter((n) => n.type === 'td')[columnIndex]!;
    assert.deepEqual(cellTokens(td), tokens, `${label}: ${JSON.stringify(value)}`);
    assert.equal(td.props.title, value === null ? 'SQL NULL' : value, `${label}: raw tooltip for ${JSON.stringify(value)}`);
  });
}
test('NULL, empty strings and invisible characters render distinctly but display-only in Browse, Search and SQL result grids; untouched edits write back raw values', async () => {
  preferences.clear();
  const pinia = createPinia(), root = hostNode('root'), app = hostRenderer.createApp(App);
  app.use(pinia);
  app.mount(root);
  const { useWorkspace } = await loadModule('../../src/stores/workspace.ts');
  const store = useWorkspace(pinia);
  const names = store.table.columns.map((c: any) => c.name);
  const name = names.indexOf('name');
  INVISIBLE.forEach(([value], i) => (store.table.rows[i].name = value));
  await nextTick();
  const table = (cls: string) => findAll(root, (n) => n.type === 'table' && String(n.props.class).includes(cls))[0]!;
  const browse = table('browse-grid');
  const offset = findAll(browse, (n) => n.type === 'tr')[1]!.children.filter((n) => n.type === 'td').length - names.length;
  assertInvisible(browse, name + offset, 'Browse');

  // Inline edit is seeded from the raw value; saving it untouched writes back exactly that value.
  for (const [i, [value]] of INVISIBLE.entries()) {
    if (typeof value !== 'string') continue;
    const row = store.table.rows[i];
    const td = () => findAll(table('browse-grid'), (n) => n.type === 'tr')[i + 1]!.children.filter((n) => n.type === 'td')[name + offset]!;
    await td().props.onDblclick({});
    await nextTick();
    // Values with a line break get the floating multi-line editor; everything else keeps the input.
    const editor = findAll(root, (n) => ['input', 'textarea'].includes(n.type) && n.props['aria-label'] === 'Edit name');
    assert.equal(editor.length, 1);
    assert.equal(editor[0]!.type, /[\r\n]/.test(value) ? 'textarea' : 'input', `editor type for ${JSON.stringify(value)}`);
    assert.equal(editor[0]!.value, value.replace(/\r\n?/g, '\n'), `editor seed for ${JSON.stringify(value)}`);
    findAll(root, (n) => n.type === 'button' && n.props['aria-label'] === 'Save cell edit')[0]!.props.onClick();
    await nextTick();
    assert.equal(store.table.rows[i], row);
    assert.equal(row.name, value, `write-back for ${JSON.stringify(value)}`);
  }
  assertInvisible(table('browse-grid'), name + offset, 'Browse after save');

  const button = (label: string) => findAll(root, (n) => n.type === 'button' && textContent(n).trim() === label)[0]!;
  await button('Search').props.onClick();
  await nextTick();
  findAll(root, (n) => n.type === 'form')[0]!.props.onSubmit({ preventDefault() {} });
  await nextTick();
  assertInvisible(findAll(root, (n) => n.type === 'table' && n.props.class === 'data-grid')[0]!, name, 'Search');

  // A result set with the same shape the mock and live adapters both deliver.
  // CodeMirror needs a real DOM; its mount failure is the only error tolerated here (the grid is under test).
  const errors: unknown[] = [];
  app.config.errorHandler = (error) => void errors.push(error);
  const w = store.workspaces[store.activeId];
  w.view = 'SQL';
  await nextTick();
  assert.ok(errors.every((e) => /createElement/.test(String(e))), String(errors));
  const q = w.queries.find((q: any) => q.id === w.queryId) ?? w.queries[0];
  q.result = { columns: ['value'], rows: INVISIBLE.map(([value]) => ({ value })) };
  await nextTick();
  const result = findAll(root, (n) => n.type === 'table' && findAll(n, (b) => b.type === 'button' && textContent(b).trim() === 'value').length > 0)[0];
  assert.ok(result, 'SQL result grid');
  assertInvisible(result, 0, 'SQL result');
  app.unmount();
});

test('multi-line inline and record editors: keyboard, edited line endings, NULL and record-form detection', async () => {
  preferences.clear();
  const pinia = createPinia(), root = hostNode('root'), app = hostRenderer.createApp(App);
  app.use(pinia);
  app.mount(root);
  const { useWorkspace } = await loadModule('../../src/stores/workspace.ts');
  const store = useWorkspace(pinia);
  const names = store.table.columns.map((c: any) => c.name);
  const name = names.indexOf('name');
  const ROWS = ['win\r\nline', 'old\rmac', 'a\r\nb\nc', 'single'];
  ROWS.forEach((value, i) => (store.table.rows[i].name = value));
  await nextTick();
  const browse = () => findAll(root, (n) => n.type === 'table' && String(n.props.class).includes('browse-grid'))[0]!;
  const offset = findAll(browse(), (n) => n.type === 'tr')[1]!.children.filter((n) => n.type === 'td').length - names.length;
  const cell = (i: number) => findAll(browse(), (n) => n.type === 'tr')[i + 1]!.children.filter((n) => n.type === 'td')[name + offset]!;
  const dialog = () => findAll(root, (n) => n.props.id === 'inline-cell-editor')[0];
  const editor = () => findAll(dialog()!, (n) => ['input', 'textarea'].includes(n.type) && n.props['aria-label'] === 'Edit name')[0]!;
  const open = async (i: number) => {
    await cell(i).props.onDblclick({});
    await nextTick();
    assert.ok(dialog(), `editor open for row ${i}`);
  };
  const type = async (text: string) => {
    editor().props['onUpdate:modelValue'](text);
    await nextTick();
  };
  /** Sends Enter to the editor dialog from the given element; returns whether the default was prevented. */
  const enter = async (tagName: string, modifiers: { ctrlKey?: boolean; metaKey?: boolean } = {}) => {
    let prevented = false;
    dialog()!.props.onKeydown({ key: 'Enter', target: { tagName }, ctrlKey: false, metaKey: false, isComposing: false, ...modifiers, preventDefault: () => (prevented = true) });
    await nextTick();
    return prevented;
  };
  const rows = store.table.rows;

  // Enter inserts a newline in the textarea; Ctrl+Enter and Cmd+Enter save in the stored style.
  await open(0);
  assert.equal(editor().type, 'textarea');
  assert.match(String(dialog()!.props.class), /multiline-editor/);
  await type('win\nline two');
  assert.equal(await enter('TEXTAREA'), false);
  assert.ok(dialog(), 'plain Enter keeps editing');
  assert.equal(await enter('BUTTON'), false, 'buttons keep native Enter');
  assert.equal(await enter('TEXTAREA', { ctrlKey: true }), true);
  assert.equal(dialog(), undefined);
  assert.equal(rows[0].name, 'win\r\nline two', 'CRLF style kept');

  await open(1);
  await type('old\nmac!');
  assert.equal(await enter('TEXTAREA', { metaKey: true }), true);
  assert.equal(rows[1].name, 'old\rmac!', 'CR style kept');

  // Mixed endings: a changed line count blocks saving until a style is chosen.
  await open(2);
  await type('a\nb\nc\nd');
  await enter('TEXTAREA', { ctrlKey: true });
  assert.ok(dialog(), 'save blocked');
  assert.match(textContent(dialog()!), /choose LF, CRLF or CR/);
  assert.equal(rows[2].name, 'a\r\nb\nc');
  findAll(dialog()!, (n) => n.type === 'select' && n.props['aria-label'] === 'Line endings')[0]!.props['onUpdate:modelValue']('lf');
  await nextTick();
  await enter('TEXTAREA', { ctrlKey: true });
  assert.equal(rows[2].name, 'a\nb\nc\nd', 'explicit LF choice');

  // Setting NULL from the multi-line editor stores SQL NULL, not text.
  store.table.columns[name].nullable = true;
  rows[2].name = 'x\ny';
  await nextTick();
  await open(2);
  const nullBox = findAll(dialog()!, (n) => n.type === 'input' && n.props.type === 'checkbox' && textContent(n.parent!).includes('NULL'))[0]!;
  nullBox.props['onUpdate:modelValue'](true);
  await nextTick();
  assert.equal(editor().props.disabled, true);
  await enter('INPUT');
  assert.equal(rows[2].name, null);
  store.table.columns[name].nullable = false;

  // Single-line values keep the input, where Enter saves.
  await open(3);
  assert.equal(editor().type, 'input');
  await type('single!');
  assert.equal(await enter('INPUT'), true);
  assert.equal(rows[3].name, 'single!');

  // The record form also switches to the multi-line editor on a lone CR, and keeps CR on save.
  rows[1].name = 'old\rmac';
  await nextTick();
  const editRecord = findAll(root, (n) => n.type === 'button' && n.props['aria-label'] === `Edit record ${rows[1].id}`)[0]!;
  await editRecord.props.onClick();
  await nextTick();
  const field = () => findAll(root, (n) => n.props['aria-label'] === 'Value for name')[0]!;
  assert.equal(field().type, 'textarea');
  field().props['onUpdate:modelValue']('old\nmac 2');
  await nextTick();
  findAll(root, (n) => n.props.class === 'record-form')[0]!.props.onSubmit({ preventDefault() {} });
  await nextTick();
  assert.equal(rows[1].name, 'old\rmac 2');
  app.unmount();
});
