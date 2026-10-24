import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createDatabases } from '../src/data/sample.ts';
import { filterRows, selectQuery, mockTransport } from '../src/services/mockTransport.ts';
import { connections } from '../src/data/sample.ts';
test('mock SQL filters, sorts, limits, and validates columns', () => {
  const tables = createDatabases()[0]!.tables;
  const r = selectQuery(
    "SELECT id, name FROM users WHERE status = 'active' ORDER BY id DESC LIMIT 5;",
    tables,
  );
  assert.equal(r.rows.length, 5);
  assert.deepEqual(r.columns, ['id', 'name']);
  assert.ok(Number(r.rows[0]!.id) > Number(r.rows[1]!.id));
  assert.throws(() => selectQuery('SELECT missing FROM users', tables), /Unknown column/);
  assert.throws(() => selectQuery('DROP TABLE users', tables), /Unsupported SQL/);
});
test('filters are case insensitive and numeric sorts are numeric', () => {
  const rows = [
    { id: 2, name: 'Olivia' },
    { id: 10, name: 'olivia' },
  ];
  assert.equal(filterRows(rows, 'OLIVIA', 'id', true)[0]!.id, 10);
  assert.equal(rows[0]!.id, 2);
});
test('connections receive independent relational data', () => {
  const a = createDatabases(),
    b = createDatabases();
  a[0]!.tables[0]!.rows.pop();
  assert.equal(b[0]!.tables[0]!.rows.length, 128);
  assert.equal(a[0]!.tables[1]!.columns[1]!.foreign, 'users.id');
});
test('mock connection validation provides success and error states', async () => {
  assert.equal((await mockTransport.test(connections[0]!)).ok, true);
  assert.equal((await mockTransport.test({ ...connections[0]!, host: 'invalid.host' })).ok, false);
});
test('numeric string values sort exactly without losing BIGINT or DECIMAL precision', () => {
  const rows = [{ id: '9007199254740993' }, { id: '9007199254740992' }, { id: '2' }, { id: 10 }];
  assert.deepEqual(
    filterRows(rows, '', 'id', false, 'bigint').map((r) => r.id),
    ['2', 10, '9007199254740992', '9007199254740993'],
  );
  const decimals = [{ n: '1.00000000000000002' }, { n: '1.00000000000000001' }];
  assert.equal(filterRows(decimals, '', 'n', false, 'decimal(30,20)')[0]!.n, '1.00000000000000001');
});
