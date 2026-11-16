import test from 'node:test';
import assert from 'node:assert/strict';
import { csvCells, liveCsvImportPlan, MAX_BATCH_SQL_BYTES } from '../src/services/liveImport.ts';
import type { Column } from '../src/types/index.ts';
const columns: Column[] = [
  { name: 'id', type: 'int', nullable: false, primary: true },
  { name: 't', type: 'varchar(64)', nullable: true },
  { name: 'b', type: 'varbinary(4)', nullable: true },
];
test('CSV keeps quoting so NULL, "NULL" and "" stay distinct (StoneQL export convention)', () => {
  const cells = csvCells('﻿"id","t"\n1,NULL\n"2","NULL"\n"3",""\n"4","a ""q"", b\nc"\r\n');
  assert.deepEqual(cells.slice(1).map((r) => r[1]), [
    { value: 'NULL', quoted: false }, { value: 'NULL', quoted: true }, { value: '', quoted: true }, { value: 'a "q", b\nc', quoted: true },
  ]);
  const plan = liveCsvImportPlan('MySQL', 't', columns, '"id","t"\n"1",NULL\n"2","NULL"\n"3",""\n');
  assert.equal(plan.records, 3);
  assert.equal(plan.batches.length, 1);
  assert.equal(plan.batches[0]!.sql, "INSERT INTO `t` (`id`, `t`) VALUES\n(_utf8mb4 X'31', NULL),\n(_utf8mb4 X'32', _utf8mb4 X'4e554c4c'),\n(_utf8mb4 X'33', _utf8mb4 X'');");
  assert.equal(liveCsvImportPlan('PostgreSQL', 't', columns, '"id","t"\n"1","O\'Brien \\\\"\n').batches[0]!.sql, `INSERT INTO "t" ("id", "t") VALUES\n(E'1', E'O''Brien \\\\\\\\');`);
});
test('headers, ragged rows, binary columns and oversized records are refused with record numbers', () => {
  assert.throws(() => liveCsvImportPlan('MySQL', 't', columns, '"id","nope"\n"1","x"\n'), /CSV column nope is not a column of t/);
  assert.throws(() => liveCsvImportPlan('MySQL', 't', columns, '"id","b"\n"1","00"\n'), /binary data/);
  assert.throws(() => liveCsvImportPlan('MySQL', 't', columns, '"id","t"\n"1","x"\n"2"\n'), /CSV record 3: expected 2 fields, received 1/);
  assert.throws(() => liveCsvImportPlan('MySQL', 't', columns, '"id","id"\n'), /distinct columns/);
  assert.throws(() => liveCsvImportPlan('MySQL', 't', columns, `"id","t"\n"1","${'x'.repeat(MAX_BATCH_SQL_BYTES)}"\n`), /CSV record 2 is too large/);
});
test('batches stay under the native job size and report the record ranges they cover', () => {
  const rows = Array.from({ length: 2000 }, (_, i) => `"${i}","${'v'.repeat(40)}"`).join('\n');
  const plan = liveCsvImportPlan('MariaDB', 't', columns, `"id","t"\n${rows}\n`);
  assert.equal(plan.records, 2000);
  assert.ok(plan.batches.length > 1);
  for (const batch of plan.batches) assert.ok(batch.sql.length <= MAX_BATCH_SQL_BYTES, String(batch.sql.length));
  assert.equal(plan.batches[0]!.first, 2);
  assert.equal(plan.batches.at(-1)!.last, 2001);
  for (let i = 1; i < plan.batches.length; i++) assert.equal(plan.batches[i]!.first, plan.batches[i - 1]!.last + 1);
});
