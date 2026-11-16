import test from 'node:test';
import assert from 'node:assert/strict';
import { liveLiteral, liveEditBlocker, liveEditableColumn, liveUpdateSql, liveDeleteSql, liveInsertSql, liveMutationOutcome } from '../src/services/liveMutations.ts';
import type { Column } from '../src/types/index.ts';
const columns: Column[] = [
  { name: 'id', type: 'bigint unsigned', nullable: false, primary: true },
  { name: 'na`me', type: 'varchar(40)', nullable: true },
  { name: 'blob', type: 'varbinary(4)', nullable: true },
];
test('literals are exact regardless of server escaping settings', () => {
  assert.equal(liveLiteral('MySQL', "O'Brien \\ é"), "_utf8mb4 X'4f27427269656e205c20c3a9'");
  assert.equal(liveLiteral('MariaDB', ''), "_utf8mb4 X''");
  assert.equal(liveLiteral('PostgreSQL', "O'Brien \\ é"), "E'O''Brien \\\\ é'");
  assert.equal(liveLiteral('MySQL', null), 'NULL');
  assert.equal(liveLiteral('PostgreSQL', { default: true }), 'DEFAULT');
  assert.equal(liveLiteral('MySQL', '18446744073709551615'), "_utf8mb4 X'3138343436373434303733373039353531363135'");
});
test('rows are addressed by their full primary key; binary columns and keyless tables are refused', () => {
  const row = { id: '18446744073709551615', 'na`me': 'x', blob: '0x00ff' };
  assert.equal(liveUpdateSql('MySQL', 't', columns, row, { 'na`me': null }), "UPDATE `t` SET `na``me` = NULL WHERE `id` = _utf8mb4 X'3138343436373434303733373039353531363135' LIMIT 1;");
  assert.equal(liveUpdateSql('PostgreSQL', 't', columns, row, { 'na`me': 'é' }), `UPDATE "t" SET "na\`me" = E'é' WHERE "id" = E'18446744073709551615';`);
  assert.equal(liveDeleteSql('MariaDB', 't', columns, row), "DELETE FROM `t` WHERE `id` = _utf8mb4 X'3138343436373434303733373039353531363135' LIMIT 1;");
  assert.throws(() => liveUpdateSql('MySQL', 't', columns, row, { blob: '0x01' }), /cannot be edited live/);
  assert.equal(liveEditableColumn(columns[2]!), false);
  assert.match(liveEditBlocker('MySQL', [{ name: 'a', type: 'int', nullable: false }]) ?? '', /no primary key/);
  assert.match(liveEditBlocker('MySQL', [{ name: 'k', type: 'binary(16)', nullable: false, primary: true }]) ?? '', /binary primary key/);
  assert.match(liveEditBlocker('MySQL', columns, 'view') ?? '', /Views are read-only/);
  assert.equal(liveEditBlocker('PostgreSQL', columns), null);
  assert.throws(() => liveDeleteSql('MySQL', 't', columns, { id: null }), /no value/);
});
test('inserts omit defaulted columns and keep explicit NULL', () => {
  assert.equal(liveInsertSql('MySQL', 't', columns, { id: { default: true }, 'na`me': null }), 'INSERT INTO `t` (`na``me`) VALUES (NULL);');
  assert.equal(liveInsertSql('PostgreSQL', 't', columns, {}), 'INSERT INTO "t" DEFAULT VALUES;');
  assert.equal(liveInsertSql('MariaDB', 't', columns, {}), 'INSERT INTO `t` () VALUES ();');
});
test('success is claimed only for exactly one affected row; conflicts and unknowns are explicit', () => {
  const job = (outcome: any, affectedRows: string, status = 'succeeded') => ({ outcome, cancellation: 'none', serverCancelSent: false, deadlineExceeded: false, lease: null, resultSets: [], statements: [{ index: 0, status, inTransaction: false, affectedRows, resultSets: 0, failure: null, resultsWithheld: false }] }) as any;
  assert.deepEqual(liveMutationOutcome(job('succeeded', '1'), 'updated'), { ok: true, inTransaction: false, text: '1 row updated.' });
  const inLease = job('succeeded', '1');
  inLease.statements[0].inTransaction = true;
  assert.equal(liveMutationOutcome(inLease, 'inserted').text, '1 row inserted in the open transaction; Browse shows it after you commit.');
  assert.equal(liveMutationOutcome(job('succeeded', '0'), 'deleted').ok, false);
  assert.match(liveMutationOutcome(job('succeeded', '0'), 'deleted').text, /changed or deleted since it was loaded/);
  assert.match(liveMutationOutcome(job('unknown', '0', 'unknown'), 'updated').text, /Outcome unknown/);
  assert.match(liveMutationOutcome(job('succeeded', '2'), 'deleted').text, /expected exactly one/);
});
test('renderer literals match the vectors shared with the native exporter', async () => {
  const { readFileSync } = await import('node:fs');
  const vectors = JSON.parse(readFileSync(new URL('../../../crates/stoneql-runtime/tests/fixtures/sql-literals.json', import.meta.url), 'utf8'));
  assert.ok(vectors.length >= 10);
  for (const vector of vectors) {
    assert.equal(liveLiteral('MySQL', vector.value), vector.mysql, vector.value);
    assert.equal(liveLiteral('PostgreSQL', vector.value), vector.postgresql, vector.value);
  }
});
