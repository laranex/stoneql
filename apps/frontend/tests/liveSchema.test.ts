import test from 'node:test';
import assert from 'node:assert/strict';
import { catalogRows, liveBrowseQuery, liveColumnsSql, liveTablesSql, toLiveColumns, toLiveTables, labelledRows, liveCatalogSupported } from '../src/services/liveSchema.ts';
import type { SqlResult } from '../src/types/index.ts';
const result = (labels: string[], rows: (string | null)[][]): SqlResult => {
  const columns = labels.map((_, i) => `__live_${i}`);
  return { columns, columnLabels: Object.fromEntries(columns.map((c, i) => [c, labels[i]!])), rows: rows.map((r) => Object.fromEntries(columns.map((c, i) => [c, r[i]!]))) };
};
test('catalog SQL stays inside the native read-only subset and quotes names', () => {
  for (const sql of [liveTablesSql("o'brien"), liveColumnsSql('shop', 'odd`name')]) {
    assert.match(sql, /^SELECT /);
    assert.doesNotMatch(sql, /\\|DATABASE\(|CAST\(|;.+/);
  }
  assert.ok(liveTablesSql("o'brien").includes("'o''brien'"));
  assert.throws(() => liveTablesSql('a\\b'), /backslash/);
  assert.equal(liveCatalogSupported('MySQL'), true);
  assert.equal(liveCatalogSupported('PostgreSQL'), false);
});
test('tables and columns keep the server spelling, keys, NULL and default distinctions', () => {
  assert.deepEqual(toLiveTables(result(['name', 'kind'], [['users', 'BASE TABLE'], ['v', 'VIEW']])).map((t) => [t.name, t.kind]), [['users', 'table'], ['v', 'view']]);
  const columns = toLiveColumns(result(['name', 'type', 'nullable', 'ckey', 'dflt', 'extra'], [
    ['id', 'bigint(20) unsigned', 'NO', 'PRI', null, 'auto_increment'],
    ['note', 'varchar(20)', 'YES', '', null, ''],
    ['state', "enum('a','b')", 'NO', 'MUL', 'a', ''],
    ['empty', 'varchar(5)', 'NO', 'UNI', '', ''],
  ]), 'MySQL');
  assert.deepEqual(columns.map((c) => [c.type, c.primary, c.autoIncrement, c.unsigned, c.defaultKind, c.default]), [
    ['bigint(20) unsigned', true, true, true, 'none', undefined],
    ['varchar(20)', false, false, false, 'null', undefined],
    ["enum('a','b')", false, false, false, 'custom', 'a'],
    ['varchar(5)', false, false, false, 'custom', ''],
  ]);
  const maria = toLiveColumns(result(['name', 'type', 'nullable', 'ckey', 'dflt', 'extra'], [['n', 'int(11)', 'YES', '', 'NULL', ''], ['s', 'varchar(4)', 'YES', '', "'NULL'", '']]), 'MariaDB');
  assert.deepEqual(maria.map((c) => [c.defaultKind, c.default]), [['null', undefined], ['custom', "'NULL'"]]);
});
test('labelled rows keep raw values and NULL', () => {
  assert.deepEqual(labelledRows(result(['id', 'n'], [['18446744073709551615', null]])), [{ id: '18446744073709551615', n: null }]);
});
test('browse page and count share the escaped filter; sort ignores unknown columns', () => {
  const columns = [{ name: 'id', type: 'bigint', nullable: false }, { name: 'na`me', type: 'text', nullable: true }];
  const plan = liveBrowseQuery({ table: 'users', columns, filter: '50%_off!', sort: 'id', descending: true, offset: 30, limit: 15 });
  assert.equal(plan.sql, "SELECT * FROM `users`\nWHERE LOWER(COALESCE(`id`, '')) LIKE '%50!%!_off!!%' ESCAPE '!' OR LOWER(COALESCE(`na``me`, '')) LIKE '%50!%!_off!!%' ESCAPE '!'\nORDER BY `id` DESC\nLIMIT 30, 15;");
  assert.equal(liveBrowseQuery({ table: 't', columns, filter: '', sort: 'na`me', descending: false, offset: 0, limit: 25 }).sql, 'SELECT * FROM `t`\nORDER BY `na``me` IS NULL ASC, `na``me` ASC\nLIMIT 0, 25;');
  assert.equal(plan.countSql, "SELECT COUNT(*) AS total FROM `users`\nWHERE LOWER(COALESCE(`id`, '')) LIKE '%50!%!_off!!%' ESCAPE '!' OR LOWER(COALESCE(`na``me`, '')) LIKE '%50!%!_off!!%' ESCAPE '!';");
  assert.equal(liveBrowseQuery({ table: 't', columns, filter: '', sort: 'missing', descending: false, offset: 0, limit: 25 }).sql, 'SELECT * FROM `t`\nLIMIT 0, 25;');
  assert.throws(() => liveBrowseQuery({ table: 't', columns, filter: 'a\\b', sort: '', descending: false, offset: 0, limit: 25 }), /backslash/);
});

test('MySQL 8 binary-charset catalog cells decode as UTF-8 text (captured from MySQL 8.4.11)', () => {
  const hex = (t: string) => Buffer.from(t, 'utf8').toString('hex');
  const raw = { index: 0, affectedRows: '0', truncated: false,
    columns: [{ name: 'name', databaseType: 'VARCHAR' }, { name: 'type', databaseType: 'BLOB' }, { name: 'nullable', databaseType: 'VARCHAR' }, { name: 'ckey', databaseType: 'BINARY' }, { name: 'dflt', databaseType: 'BLOB' }, { name: 'extra', databaseType: 'VARCHAR' }],
    rows: [
      [{ kind: 'text', text: 'id' }, { kind: 'binary', hex: '626967696e7420756e7369676e6564' }, { kind: 'text', text: 'NO' }, { kind: 'binary', hex: '505249' }, { kind: 'null' }, { kind: 'text', text: 'auto_increment' }],
      [{ kind: 'text', text: 'état' }, { kind: 'binary', hex: hex("enum('é','b')") }, { kind: 'text', text: 'NO' }, { kind: 'binary', hex: '' }, { kind: 'binary', hex: hex('é') }, { kind: 'text', text: '' }],
    ] } as any;
  const columns = toLiveColumns({ columns: [], rows: [], raw }, 'MySQL');
  assert.deepEqual(columns.map((c) => [c.name, c.type, c.primary, c.unsigned, c.defaultKind, c.default]), [
    ['id', 'bigint unsigned', true, true, 'none', undefined],
    ['état', "enum('é','b')", false, false, 'custom', 'é'],
  ]);
  const tables = toLiveTables({ columns: [], rows: [], raw: { ...raw, columns: [{ name: 'name', databaseType: 'VARCHAR' }, { name: 'kind', databaseType: 'BINARY' }], rows: [[{ kind: 'text', text: 'v' }, { kind: 'binary', hex: '56494557' }]] } });
  assert.deepEqual(tables.map((t) => [t.name, t.kind]), [['v', 'view']]);
  assert.throws(() => catalogRows({ columns: [], rows: [], raw: { ...raw, columns: [{ name: 'x', databaseType: 'BLOB' }], rows: [[{ kind: 'binary', hex: 'ff' }]] } }), TypeError);
});
