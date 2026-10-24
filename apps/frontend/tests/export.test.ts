import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createDatabases } from '../src/data/sample.ts';
import { defaultExport, capabilities } from '../src/types/export.ts';
import {
  effectiveExport,
  exportSummary,
  validateExport,
  simulatedExportTransport,
} from '../src/services/exportService.ts';
const db = createDatabases()[0]!;
function config() {
  const c = defaultExport();
  c.tables = ['users'];
  return c;
}
test('Quick defaults to SQL, structure + data, and an accurate table summary', () => {
  const c = effectiveExport(config(), 'MySQL');
  assert.equal(c.format, 'SQL');
  assert.equal(c.content, 'both');
  assert.equal(c.create, true);
  assert.equal(validateExport(c, db).length, 0);
  const s = exportSummary(c, db, 'MySQL');
  assert.equal(s.rows, 128);
  assert.equal(s.structures, 1);
  assert.equal(s.tables, 1);
});
test('database table selection supports all, none, and individual selections', () => {
  const c = config();
  c.scope = 'database';
  c.tables = db.tables.map((t) => t.name);
  assert.equal(exportSummary(c, db, 'MySQL').tables, 7);
  assert.equal(exportSummary(c, db, 'MySQL').rows, 1058);
  c.tables = [];
  assert.match(validateExport(c, db).join(' '), /at least one/);
  c.tables = ['orders', 'products'];
  assert.equal(exportSummary(c, db, 'MySQL').rows, 294);
});
test('Custom structure-only and data-only produce matching counts and options', () => {
  const c = config();
  c.mode = 'custom';
  c.content = 'structure';
  let e = effectiveExport(c, 'MySQL');
  assert.equal(exportSummary(e, db, 'MySQL').rows, 0);
  assert.equal(e.transaction, false);
  c.content = 'data';
  c.drop = true;
  e = effectiveExport(c, 'MySQL');
  assert.equal(exportSummary(e, db, 'MySQL').structures, 0);
  assert.equal(e.drop, false);
  assert.equal(e.create, false);
});
test('CSV validates incompatible content, separator, and enclosure settings', () => {
  const c = config();
  c.mode = 'custom';
  c.format = 'CSV';
  assert.match(validateExport(effectiveExport(c, 'MySQL'), db).join(' '), /Data only/);
  c.content = 'data';
  assert.equal(validateExport(c, db).length, 0);
  c.csv.delimiter = '::';
  assert.match(validateExport(c, db).join(' '), /delimiter/);
  c.csv.delimiter = '"';
  assert.match(validateExport(c, db).join(' '), /delimiter/);
});
test('Quick always uses structure and data, with CSV validation and JSON support', () => {
  const c = config();
  c.format = 'CSV';
  assert.equal(effectiveExport(c, 'MySQL').content, 'both');
  assert.match(validateExport(effectiveExport(c, 'MySQL'), db).join(' '), /Data only/);
  c.format = 'JSON';
  const e = effectiveExport(c, 'MySQL');
  assert.equal(e.content, 'both');
  assert.equal(e.create, false);
  c.json.metadata = true;
  c.json.pretty = false;
  assert.equal(effectiveExport(c, 'MySQL').json.metadata, true);
});
test('compression and filename templates appear in the simulated summary', () => {
  const c = config();
  c.filename = '@DATABASE@_@TABLE@_@DATE@';
  for (const compression of ['none', 'zip', 'gzip'] as const) {
    c.compression = compression;
    const s = exportSummary(c, db, 'MySQL');
    assert.equal(s.compression, compression);
    assert.match(
      s.filename,
      new RegExp(
        compression === 'gzip'
          ? '\\.sql\\.gz$'
          : compression === 'zip'
            ? '\\.sql\\.zip$'
            : '\\.sql$',
      ),
    );
    assert.match(s.filename, /^acme_store_users_/);
  }
  c.filename = '../backup';
  assert.match(validateExport(c, db).join(' '), /filename/);
});
test('engine capabilities strip unsupported options and insert styles', () => {
  const c = config();
  c.mode = 'custom';
  c.events = true;
  c.routines = true;
  c.insertStyle = 'replace';
  const pg = effectiveExport(c, 'PostgreSQL');
  assert.equal(pg.events, false);
  assert.equal(pg.foreignKeys, false);
  assert.equal(pg.autoIncrement, false);
  assert.equal(pg.insertStyle, 'extended');
  const sqlite = effectiveExport(c, 'SQLite');
  assert.equal(sqlite.events, false);
  assert.equal(sqlite.routines, false);
  assert.equal(sqlite.insertStyle, 'replace');
  assert.equal(capabilities.MariaDB.events, true);
});
test('missing tables and absent CREATE TABLE are validated', () => {
  const c = config();
  c.tables = ['gone'];
  assert.match(validateExport(c, db).join(' '), /no longer exist/);
  c.tables = ['users'];
  c.create = false;
  assert.match(validateExport(c, db).join(' '), /CREATE TABLE/);
});
test('successful simulation reports progress and does not mutate database', async () => {
  const before = JSON.stringify(db),
    values: number[] = [];
  const result = await simulatedExportTransport.run(
    config(),
    db,
    'MySQL',
    new AbortController().signal,
    (value) => values.push(value),
  );
  assert.equal(result.rows, 128);
  assert.equal(values.at(-1), 100);
  assert.equal(JSON.stringify(db), before);
});
test('running simulation can be cancelled without a completed summary', async () => {
  const controller = new AbortController();
  const task = simulatedExportTransport.run(config(), db, 'MySQL', controller.signal, () => {});
  controller.abort();
  await assert.rejects(task, { name: 'AbortError' });
});
test('simulated failure rejects with a useful error', async () => {
  await assert.rejects(
    simulatedExportTransport.run(
      config(),
      db,
      'MySQL',
      new AbortController().signal,
      () => {},
      true,
    ),
    /destination unavailable/,
  );
});
test('invalid jobs fail before starting progress', async () => {
  const c = config();
  c.tables = [];
  let called = false;
  await assert.rejects(
    simulatedExportTransport.run(c, db, 'MySQL', new AbortController().signal, () => {
      called = true;
    }),
    /at least one/,
  );
  assert.equal(called, false);
});

test('Quick defaults are independent of saved Custom content and table selections', () => {
  const c = config();
  c.mode = 'custom';
  c.content = 'structure';
  c.scope = 'database';
  c.tables = ['users'];
  c.compression = 'gzip';
  const before = structuredClone(c);
  c.mode = 'quick';
  const effective = effectiveExport(
    c,
    'MySQL',
    db.tables.map((t) => t.name),
  );
  assert.equal(effective.content, 'both');
  assert.equal(effective.tables.length, db.tables.length);
  assert.equal(effective.compression, 'none');
  assert.equal(c.content, 'structure');
  assert.deepEqual(c.tables, ['users']);
  c.mode = 'custom';
  assert.deepEqual(c, before);
  assert.equal(effectiveExport(c, 'MySQL').content, 'structure');
});
