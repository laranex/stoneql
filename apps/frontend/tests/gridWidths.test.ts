import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  clampWidth,
  defaultColumnWidth,
  queryWidthKey,
  readGridWidths,
  saveGridWidths,
} from '../src/services/gridWidths.ts';
test('column width defaults and bounds are appropriate for schema types', () => {
  assert.equal(clampWidth(1), 64);
  assert.equal(clampWidth(9999), 600);
  assert.equal(clampWidth(NaN), 160);
  assert.ok(
    defaultColumnWidth({ name: 'id', type: 'int' }) <
      defaultColumnWidth({ name: 'note', type: 'text' }),
  );
  assert.equal(defaultColumnWidth({ name: 'check', width: 36, resizable: false }), 36);
});
test('width persistence isolates connections, databases, tables, columns, and SQL queries', () => {
  const memory = new Map<string, string>(),
    storage = {
      getItem: (key: string) => memory.get(key) ?? null,
      setItem: (key: string, value: string) => {
        memory.set(key, value);
      },
    };
  saveGridWidths(storage, 'browse:a:db:users', { id: 90, name: 300 });
  saveGridWidths(storage, 'browse:b:db:users', { id: 120 });
  assert.deepEqual(readGridWidths(storage, 'browse:a:db:users'), { id: 90, name: 300 });
  assert.equal(readGridWidths(storage, 'browse:b:db:users').id, 120);
  saveGridWidths(storage, 'browse:a:db:users', null);
  assert.deepEqual(readGridWidths(storage, 'browse:a:db:users'), {});
  assert.equal(readGridWidths(storage, 'browse:b:db:users').id, 120);
  assert.notEqual(
    queryWidthKey('SELECT id FROM users', ['id']),
    queryWidthKey('SELECT name FROM users', ['name']),
  );
});
test('invalid width preferences are bounded or surfaced without saving any data values', () => {
  assert.deepEqual(
    readGridWidths({ getItem: () => '{"grid":{"id":1,"name":9999,"bad":"secret"}}' }, 'grid'),
    { id: 64, name: 600 },
  );
  assert.throws(() => readGridWidths({ getItem: () => '{bad' }, 'grid'));
});
