import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  createColumnDraft,
  formatColumnType,
  validateColumn,
  applyColumnDefinition,
  parseValues,
  columnCapabilities,
} from '../src/services/columnDefinition.ts';
import { createDatabases } from '../src/data/sample.ts';
const table = () => createDatabases()[0]!.tables[0]!;
test('VARCHAR(100) adds AFTER name and renames without changing metadata or values', () => {
  const t = table(),
    d = createColumnDraft('MySQL');
  Object.assign(d, {
    name: 'nickname',
    dataType: 'VARCHAR',
    length: 100,
    position: 'after',
    after: 'name',
    collation: 'utf8mb4_bin',
    comment: 'Display name',
    defaultKind: 'custom',
    defaultValue: 'Guest',
  });
  applyColumnDefinition(t, d, 'MySQL');
  assert.equal(t.columns[2]!.name, 'nickname');
  assert.equal(t.columns[2]!.type, 'varchar(100)');
  assert.equal(t.rows[0]!.nickname, 'Guest');
  const edited = createColumnDraft('MySQL', t.columns[2]);
  edited.name = 'display_name';
  applyColumnDefinition(t, edited, 'MySQL', 'nickname');
  assert.equal(t.columns[2]!.name, 'display_name');
  assert.equal(t.rows[0]!.display_name, 'Guest');
  assert.equal(t.rows[0]!.nickname, undefined);
  assert.equal(t.columns[2]!.comment, 'Display name');
  assert.equal(t.columns[2]!.collation, 'utf8mb4_bin');
});
test('DECIMAL(12,2), custom defaults, FIRST and AT END reflect in mock structure', () => {
  const t = table(),
    d = createColumnDraft('MySQL');
  Object.assign(d, {
    name: 'balance',
    dataType: 'DECIMAL',
    precision: 12,
    scale: 2,
    defaultKind: 'custom',
    defaultValue: '0.00',
    position: 'first',
    nullable: false,
  });
  applyColumnDefinition(t, d, 'MySQL');
  assert.equal(t.columns[0]!.type, 'decimal(12,2)');
  assert.equal(t.rows[0]!.balance, '0.00');
  const e = createColumnDraft('MySQL', t.columns[0]);
  e.position = 'end';
  applyColumnDefinition(t, e, 'MySQL', 'balance');
  assert.equal(t.columns.at(-1)!.name, 'balance');
});
test('ENUM and SET values are parsed, escaped, and round-tripped', () => {
  const d = createColumnDraft('MySQL');
  Object.assign(d, {
    name: 'state',
    dataType: 'ENUM',
    values: "'in progress','done','can''t'",
    defaultKind: 'custom',
    defaultValue: 'done',
  });
  assert.equal(formatColumnType(d, 'MySQL'), "enum('in progress','done','can''t')");
  const t = table();
  applyColumnDefinition(t, d, 'MySQL');
  const e = createColumnDraft('MySQL', t.columns.at(-1));
  assert.equal(e.values, d.values);
  assert.deepEqual(parseValues("'one,two','three'"), ['one,two', 'three']);
  assert.throws(() => parseValues("'missing"), /unclosed/);
  assert.throws(() => parseValues('one,one'), /unique/);
  d.dataType = 'SET';
  d.values = 'one,two';
  d.defaultValue = 'one,two';
  assert.deepEqual(validateColumn(d, 'MySQL', table()), []);
});
test('validates length, decimals, incompatible defaults, and generated keys', () => {
  const t = table(),
    d = createColumnDraft('MySQL');
  d.name = 'extra';
  d.length = 0;
  assert.match(validateColumn(d, 'MySQL', t).join(' '), /length/);
  d.dataType = 'DECIMAL';
  d.precision = 12;
  d.scale = 13;
  assert.match(validateColumn(d, 'MySQL', t).join(' '), /precision/);
  d.scale = 2;
  d.nullable = false;
  d.defaultKind = 'null';
  assert.match(validateColumn(d, 'MySQL', t).join(' '), /NULL default/);
  d.nullable = true;
  d.autoIncrement = true;
  assert.match(validateColumn(d, 'MySQL', t).join(' '), /indexed integer/);
});
test('PostgreSQL and SQLite reject MySQL-only options and physical positioning', () => {
  for (const engine of ['PostgreSQL', 'SQLite'] as const) {
    const d = createColumnDraft(engine);
    d.name = 'extra';
    d.position = 'after';
    d.after = 'id';
    d.unsigned = true;
    assert.match(validateColumn(d, engine, table()).join(' '), /UNSIGNED/);
    assert.match(validateColumn(d, engine, table()).join(' '), /positioning/);
    assert.equal(
      columnCapabilities[engine].types.some((t) => t.name === 'ENUM' || t.name === 'SET'),
      false,
    );
  }
});
test('SQLite shows only appropriate type parameters and preserves edit order', () => {
  const t = table(),
    d = createColumnDraft('SQLite', t.columns[1]);
  assert.equal(d.dataType, 'TEXT');
  assert.equal(
    columnCapabilities.SQLite.types.find((t) => t.name === d.dataType)?.parameters,
    undefined,
  );
  d.comment = 'unsupported';
  assert.match(validateColumn(d, 'SQLite', t, 'name').join(' '), /comments/);
  d.comment = '';
  d.position = 'end';
  applyColumnDefinition(t, d, 'SQLite', 'name');
  assert.equal(t.columns[1]!.name, 'name');
});
test('NULL and expression defaults populate new mock columns appropriately', () => {
  const t = table(),
    d = createColumnDraft('MySQL');
  d.name = 'optional';
  d.defaultKind = 'null';
  applyColumnDefinition(t, d, 'MySQL');
  assert.equal(t.rows[0]!.optional, null);
  const e = createColumnDraft('MySQL');
  Object.assign(e, {
    name: 'updated_at',
    dataType: 'TIMESTAMP',
    defaultKind: 'expression',
    expression: 'CURRENT_TIMESTAMP',
    nullable: false,
  });
  applyColumnDefinition(t, e, 'MySQL');
  assert.match(String(t.rows[0]!.updated_at), /^\d{4}-\d{2}-\d{2} /);
});
test('invalid data changes are atomic and keep existing structure and rows intact', () => {
  const t = table(),
    before = JSON.stringify(t),
    d = createColumnDraft('MySQL', t.columns[1]);
  d.length = 1;
  assert.throws(() => applyColumnDefinition(t, d, 'MySQL', 'name'), /exceed/);
  assert.equal(JSON.stringify(t), before);
  const e = createColumnDraft('MySQL');
  Object.assign(e, { name: 'required', nullable: false });
  assert.throws(() => applyColumnDefinition(t, e, 'MySQL'), /default/);
  assert.equal(JSON.stringify(t), before);
});
