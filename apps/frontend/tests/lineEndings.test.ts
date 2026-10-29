// Line-ending fidelity for multi-line editors: a textarea reports LF only, so the stored string is
// rebuilt from the original value and an explicit choice, never silently normalized.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { defineComponent, h, nextTick, ref } from 'vue';
import { hostRenderer, hostNode, findAll, loadVue } from './helpers/vueHost.ts';
import {
  applyLineEndings,
  defaultLineEnding,
  hasLineBreak,
  lineEndingStyle,
  normalizeLineEndings,
} from '../src/services/lineEndings.ts';

test('line breaks are detected for LF, CR and CRLF only', () => {
  for (const value of ['a\nb', 'a\rb', 'a\r\nb', '\n', '\r']) assert.equal(hasLineBreak(value), true, JSON.stringify(value));
  for (const value of ['', 'NULL', 'a\tb', null, undefined, 42]) assert.equal(hasLineBreak(value), false, JSON.stringify(value));
  assert.deepEqual(['x', 'a\nb', 'a\r\nb\r\n', 'a\rb', 'a\r\nb\nc', 'a\rb\nc'].map(lineEndingStyle), [null, 'lf', 'crlf', 'cr', 'mixed', 'mixed']);
  assert.equal(normalizeLineEndings('a\r\nb\rc\nd\r\r\n'), 'a\nb\nc\nd\n\n');
});
test('untouched text returns the original string exactly', () => {
  for (const original of ['', 'plain', 'a\nb', 'a\rb', 'a\r\nb', 'a\r\nb\nc\rd', '\r\n\r\n', 'tab\there\r'])
    assert.equal(applyLineEndings(normalizeLineEndings(original), defaultLineEnding(original), original), original, JSON.stringify(original));
});
test('edited text keeps a consistent original style; values without breaks get LF', () => {
  assert.equal(applyLineEndings('a\nb\nnew', 'crlf', 'a\r\nb'), 'a\r\nb\r\nnew');
  assert.equal(applyLineEndings('a\nbX\nnew', 'cr', 'a\rb'), 'a\rbX\rnew');
  assert.equal(applyLineEndings('aX\nb', 'lf', 'a\nb'), 'aX\nb');
  assert.equal(defaultLineEnding('plain'), 'lf');
  assert.equal(applyLineEndings('one\ntwo', defaultLineEnding('plain'), 'plain'), 'one\ntwo');
});
test('mixed endings are kept per position while the break count is unchanged and need a choice otherwise', () => {
  const original = 'a\r\nb\nc\rd';
  assert.equal(defaultLineEnding(original), 'keep');
  assert.equal(applyLineEndings('A\nB\nC\nD', 'keep', original), 'A\r\nB\nC\rD');
  assert.equal(applyLineEndings('a\nb\nc', 'keep', original), null, 'a removed break cannot be mapped');
  assert.equal(applyLineEndings('a\nb\nc\nd\ne', 'keep', original), null, 'an added break cannot be guessed');
  assert.equal(applyLineEndings('a\nb\nc\nd\ne', 'crlf', original), 'a\r\nb\r\nc\r\nd\r\ne');
  assert.equal(applyLineEndings(normalizeLineEndings(original), 'lf', original), 'a\nb\nc\nd', 'explicit LF converts');
});

// Vue's v-model directives consult these browser globals.
Object.assign(globalThis, { Document: class {}, ShadowRoot: class {}, document: { activeElement: null } });
const RawTextarea = await loadVue('../../src/components/RawTextarea.vue');
async function mount(initial: string) {
  const value = ref(initial), issue = ref('');
  const root = hostNode('root');
  const app = hostRenderer.createApp(
    defineComponent(() => () =>
      h(RawTextarea, {
        label: 'Edit note',
        modelValue: value.value,
        'onUpdate:modelValue': (v: string) => (value.value = v),
        issue: issue.value,
        'onUpdate:issue': (v: string) => (issue.value = v),
      }),
    ),
  );
  app.mount(root);
  await nextTick();
  const textarea = () => findAll(root, (n) => n.type === 'textarea')[0]!;
  const select = () => findAll(root, (n) => n.type === 'select')[0];
  // What a browser does on input: the textarea reports LF-only text through v-model.
  const type = async (text: string) => {
    textarea().props['onUpdate:modelValue'](normalizeLineEndings(text));
    await nextTick();
  };
  const choose = async (choice: string) => {
    select()!.props['onUpdate:modelValue'](choice);
    await nextTick();
  };
  return { value, issue, textarea, select, type, choose, app };
}

test('the editor shows LF text, starts from the stored style and writes edits back in that style', async () => {
  const crlf = await mount('one\r\ntwo');
  assert.equal(crlf.textarea().props.value ?? crlf.textarea().value, 'one\ntwo');
  assert.equal(crlf.select()!.props['aria-label'], 'Line endings');
  assert.deepEqual(crlf.select()!.children.filter((n) => n.type === 'option').map((o) => o.props.value), ['lf', 'crlf', 'cr']);
  assert.equal(crlf.value.value, 'one\r\ntwo', 'mounting emits nothing');
  await crlf.type('one\ntwo!\nthree');
  assert.equal(crlf.value.value, 'one\r\ntwo!\r\nthree');
  await crlf.type('one\ntwo');
  assert.equal(crlf.value.value, 'one\r\ntwo', 'typing back the original restores it exactly');
  await crlf.choose('lf');
  assert.equal(crlf.value.value, 'one\ntwo', 'explicit LF choice converts');
  crlf.app.unmount();

  const cr = await mount('a\rb');
  await cr.type('a\nbX');
  assert.equal(cr.value.value, 'a\rbX');
  cr.app.unmount();

  const plain = await mount('plain');
  assert.equal(plain.select(), undefined, 'no line-ending choice without line breaks');
  await plain.type('plain\nmore');
  assert.equal(plain.value.value, 'plain\nmore');
  assert.ok(plain.select(), 'choice appears once a break exists');
  plain.app.unmount();
});
test('mixed endings: per-position while structure is unchanged, explicit choice required otherwise', async () => {
  const mixed = await mount('a\r\nb\nc');
  assert.equal(mixed.select()!.children[0]!.props.value, 'keep');
  await mixed.type('A\nB\nC');
  assert.equal(mixed.value.value, 'A\r\nB\nC');
  assert.equal(mixed.issue.value, '');
  await mixed.type('A\nB\nC\nD');
  assert.match(mixed.issue.value, /choose LF, CRLF or CR/);
  assert.notEqual(mixed.value.value, 'a\r\nb\nc', 'the edit is dirty');
  await mixed.choose('crlf');
  assert.equal(mixed.issue.value, '');
  assert.equal(mixed.value.value, 'A\r\nB\r\nC\r\nD');
  mixed.app.unmount();
});
test('an external value resets the editor to that value and its style', async () => {
  const editor = await mount('x\r\ny');
  await editor.type('x\ny\nz');
  editor.value.value = 'p\rq';
  await nextTick();
  await editor.type('p\nq!');
  assert.equal(editor.value.value, 'p\rq!');
  editor.app.unmount();
});
test('a no-op line-ending choice never hides a later external value', async () => {
  const editor = await mount('a\nb');
  await editor.type('ab');
  await editor.choose('crlf');
  assert.equal(editor.value.value, 'ab', 'CRLF on text without breaks changes nothing');
  editor.value.value = 'p\nq';
  await nextTick();
  editor.value.value = 'ab';
  await nextTick();
  assert.equal(editor.textarea().props.value ?? editor.textarea().value, 'ab', 'the editor shows the model');
  await editor.type('abZ');
  assert.equal(editor.value.value, 'abZ');
  editor.app.unmount();
});
