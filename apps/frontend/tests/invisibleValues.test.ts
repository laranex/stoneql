import { test } from 'node:test';
import assert from 'node:assert/strict';
import { invisibleSegments, rawSelectionText } from '../src/services/invisibleValues.ts';

const TAB = { marker: '⇥', raw: '\t', label: 'tab' };
const LF = { marker: '↵', raw: '\n', label: 'line feed' };
const CR = { marker: '␍', raw: '\r', label: 'carriage return' };
const CRLF = { marker: '␍↵', raw: '\r\n', label: 'CRLF' };

test('ordinary values, NULL and non-strings take the fast path with no segments', () => {
  for (const value of ['', 'NULL', 'plain text', '  spaced  out  ', null, undefined, 42, 12345678901234567890n, true])
    assert.equal(invisibleSegments(value), null, String(value));
});
test('tab, LF, CR and CRLF become single display-only markers with their raw character and spoken label', () => {
  assert.deepEqual(invisibleSegments('a\tb'), [{ text: 'a' }, TAB, { text: 'b' }]);
  assert.deepEqual(invisibleSegments('a\nb'), [{ text: 'a' }, LF, { text: 'b' }]);
  assert.deepEqual(invisibleSegments('a\rb'), [{ text: 'a' }, CR, { text: 'b' }]);
  assert.deepEqual(invisibleSegments('a\r\nb'), [{ text: 'a' }, CRLF, { text: 'b' }]);
  assert.deepEqual(invisibleSegments('\n\r'), [LF, CR]);
  assert.deepEqual(invisibleSegments('\t\t'), [TAB, TAB]);
  assert.deepEqual(invisibleSegments(' x \r\n\r\n y '), [{ text: ' x ' }, CRLF, CRLF, { text: ' y ' }]);
});
test('text runs and marker raw characters reassemble the exact stored value', () => {
  for (const value of ['mix\ted\r\nlines\nand\rcr', '\t', 'tail\n', 'NULL\tNULL', 'ünïcode\t✓\n', '\r\r\n\n\r'])
    assert.equal(invisibleSegments(value)!.map((s) => s.text ?? s.raw).join(''), value);
});
test('marker glyphs and labels never appear in text segments', () => {
  for (const s of invisibleSegments('a\tb\nc\rd\r\ne')!)
    if (s.text !== undefined) assert.doesNotMatch(s.text, /[⇥↵␍\t\n\r]|tab|line feed|carriage|CRLF/);
});

// Minimal DOM nodes: enough of Node/Element for the selection serializer.
type FakeNode = { nodeType: number; nodeValue?: string; tagName?: string; childNodes: FakeNode[]; previousElementSibling?: FakeNode | null; getAttribute?: (name: string) => string | null };
const text = (value: string): FakeNode => ({ nodeType: 3, nodeValue: value, childNodes: [] });
function el(tagName: string, children: FakeNode[] = [], attrs: Record<string, string> = {}): FakeNode {
  const node: FakeNode = { nodeType: 1, tagName, childNodes: children, getAttribute: (name) => attrs[name] ?? null };
  children.filter((c) => c.nodeType === 1).forEach((c, i, all) => (c.previousElementSibling = all[i - 1] ?? null));
  return node;
}
function fragment(children: FakeNode[]) {
  const all = (n: FakeNode): FakeNode[] => [n, ...n.childNodes.flatMap(all)];
  return {
    nodeType: 11,
    childNodes: children,
    querySelector: () => children.flatMap(all).find((n) => n.getAttribute?.('data-raw') != null) ?? null,
  } as unknown as DocumentFragment;
}
const marker = (raw: string, glyph: string) => el('SPAN', [text(glyph)], { 'data-raw': raw, class: 'invisible-marker' });

test('copied selections replace markers with the characters they stand for', () => {
  assert.equal(rawSelectionText(fragment([text('plain'), el('I', [text('NULL')])])), null, 'no marker: browser copy');
  assert.equal(
    rawSelectionText(fragment([text('one'), marker('\r\n', '␍↵'), text('two'), marker('\t', '⇥'), text('end')])),
    'one\r\ntwo\tend',
  );
  const row = (...cells: FakeNode[][]) => el('TR', cells.map((c) => el('TD', c)));
  assert.equal(
    rawSelectionText(fragment([el('TBODY', [row([text('a'), marker('\n', '↵'), text('b')], [el('I', [text('NULL')])]), row([marker('\r', '␍')], [text('x')])])])),
    'a\nb\tNULL\n\r\tx',
  );
});
