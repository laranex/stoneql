import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createDatabases } from '../src/data/sample.ts';
import {
  deleteRecords,
  updateRecord,
  insertRecords,
  copyRecords,
  renameMockTable,
  removeMockTable,
  removeMockColumn,
} from '../src/services/recordMutations.ts';
const fixture = () => {
  const db = createDatabases()[0]!;
  return { db, users: db.tables[0]!, orders: db.tables[1]! };
};
test('C2 RESTRICT prevents parent deletion, parent key edits, truncation and dropping referenced schema', () => {
  const { db, users } = fixture(),
    row = users.rows[0]!,
    before = JSON.stringify(db);
  assert.throws(() => deleteRecords(db, users, [row]), /orders.user_id/);
  assert.throws(() => updateRecord(db, users, row, { id: '999' }, 'MySQL'), /RESTRICT/);
  assert.throws(() => removeMockTable(db, users, 'MySQL', true), /RESTRICT/);
  assert.throws(() => removeMockTable(db, users, 'PostgreSQL', false), /RESTRICT/);
  assert.equal(JSON.stringify(db), before);
});
test('C2 unreferenced records delete and table renames update all references', () => {
  const { db, users, orders } = fixture();
  insertRecords(
    db,
    users,
    [
      {
        name: 'New',
        email: 'new@example.test',
        role: 'customer',
        status: 'active',
        permissions: 'read',
        created_at: '2026-10-24 12:00:00',
      },
    ],
    'MySQL',
  );
  const row = users.rows.at(-1)!;
  deleteRecords(db, users, [row]);
  assert.equal(users.rows.length, 128);
  renameMockTable(db, users, 'customers');
  assert.equal(orders.columns[1]!.foreign, 'customers.id');
  assert.throws(() => deleteRecords(db, users, [users.rows[0]!]), /customers.id/);
});
test('C3 bulk copy is atomic and cannot duplicate UNIQUE values', () => {
  const { db, users } = fixture(),
    before = JSON.stringify(users);
  assert.throws(
    () => copyRecords(db, users, [users.rows[0]!, users.rows[1]!], 'MySQL'),
    /key value already exists/,
  );
  assert.equal(JSON.stringify(users), before);
});
test('the mutation boundary validates every insertion and rejects invalid batches atomically', () => {
  const { db, orders } = fixture(),
    before = JSON.stringify(orders);
  assert.throws(
    () =>
      insertRecords(
        db,
        orders,
        [
          { user_id: '1', total: '10.00', status: 'pending' },
          { user_id: '999999', total: '10.00', status: 'pending' },
        ],
        'MySQL',
      ),
    /referenced record/,
  );
  assert.equal(JSON.stringify(orders), before);
  assert.throws(
    () => insertRecords(db, orders, [{ user_id: '1', total: '1.999', status: 'pending' }], 'MySQL'),
    /fractional digits/,
  );
});
test('referenced non-primary columns cannot be dropped', () => {
  const { db, users } = fixture();
  users.columns.find((c) => c.name === 'email')!.unique = true;
  db.tables.push({
    name: 'mail',
    columns: [{ name: 'email', type: 'varchar(255)', nullable: true, foreign: 'users.email' }],
    rows: [],
  });
  assert.throws(
    () =>
      removeMockColumn(
        db,
        users,
        users.columns.find((c) => c.name === 'email')!,
      ),
    /referenced by mail.email/,
  );
});
