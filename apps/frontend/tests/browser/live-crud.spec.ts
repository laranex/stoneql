import { test, expect } from '@playwright/test';
import { newLiveConnection } from './desktop';
// Simulated native bridge with the read-write gate on (as only test hosts have it): a MySQL table
// with a primary key; every change arrives as one generated statement in a read-write job.
test('live read-write CRUD sends exact single-row statements and reports conflicts', async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as any;
    w.jobs = [];
    w.affected = '1';
    const col = (name: string, databaseType = 'VARCHAR') => ({ name, databaseType });
    const text = (value: string | null) => (value === null ? { kind: 'null' } : { kind: 'text', text: value });
    const result = (columns: any[], values: (string | null)[][]) => ({ index: 0, affectedRows: '0', truncated: false, columns, rows: values.map((r) => r.map(text)) });
    const rows = [['9007199254740993', 'Ada'], ['9007199254740994', null]];
    const tables = [['accounts', 'BASE TABLE']];
    w.__TAURI__ = { core: { invoke: async (command: string, args: { message: string }) => {
      const request = JSON.parse(args.message), p = request.payload;
      let payload: any;
      if (command === 'host_info') payload = { runtime: 'desktop', appVersion: '0.1.0', sessionId: 's', nonce: p.nonce, simulated: true, readWriteSql: true };
      else if (command === 'contract_probe') payload = { values: p.values, simulated: true };
      else if (command === 'list_unresolved_outcomes') payload = { outcomes: [], limit: 16 };
      else if (command === 'connect_direct') payload = { handle: '00000000-0000-0000-0000-000000000009', engineVersion: 'MySQL fixture 8.4.6', database: p.database, expiresAfterSeconds: 3600, access: p.access ?? 'read_only' };
      else if (command === 'database_status') payload = { connected: true };
      else if (command === 'disconnect_database') payload = { connected: false };
      else if (command === 'begin_transaction') { w.leases = (w.leases ?? 0) + 1; payload = { leaseId: `lease_${w.leases}`, idleSeconds: 300 }; }
      else if (command === 'end_transaction') { w.ends = [...(w.ends ?? []), p.action]; payload = { outcome: p.action === 'commit' ? 'committed' : 'rolled_back', reason: 'requested', commitSent: p.action === 'commit', rollbackSent: p.action === 'rollback' }; }
      else if (command === 'submit_job') { w.jobs.push(p.sql); w.leased = [...(w.leased ?? []), p.leaseId ?? null]; if (/^CREATE TABLE audit_log/.test(p.sql)) tables.push(['audit_log', 'BASE TABLE']); payload = { jobId: `job_${w.jobs.length}`, statementCount: 1 }; }
      else if (command === 'fetch_job') {
        const failed = w.failNext; w.failNext = false;
        const inLease = w.leased.at(-1) !== null;
        payload = { jobId: p.jobId, state: 'finished', outcome: failed ? 'failed' : 'succeeded', cancellation: 'none', serverCancelSent: false, deadlineExceeded: false, statements: [{ index: 0, status: failed ? 'failed' : 'succeeded', inTransaction: inLease, affectedRows: w.affected, resultSets: 0, failure: failed ? { constraint: 'unique', sqlState: '23000' } : null, resultsWithheld: false }], batch: null, lease: inLease ? { leaseId: w.leased.at(-1), state: failed ? 'rollback_only' : 'active' } : null, done: true };
      }
      else if (command === 'read_only_query') {
        const sql: string = p.sql;
        if (sql.includes('information_schema.TABLES')) payload = result([col('name'), col('kind')], tables);
        else if (sql.includes('information_schema.COLUMNS')) payload = result(['name', 'type', 'nullable', 'ckey', 'dflt', 'extra'].map((n) => col(n)), [['id', 'bigint(20) unsigned', 'NO', 'PRI', null, 'auto_increment'], ['name', 'varchar(40)', 'YES', '', null, '']]);
        else if (sql.includes('STATISTICS') || sql.includes('KEY_COLUMN_USAGE')) payload = result([col('name')], []);
        else if (sql.startsWith('SELECT COUNT(*)')) payload = result([col('total', 'BIGINT')], [[String(rows.length)]]);
        else payload = result([col('id', 'BIGINT UNSIGNED'), col('name')], rows);
      } else throw { code: 'INVALID_REQUEST' };
      return { version: 1, requestId: request.requestId, payload };
    } } };
  });
  await page.goto('/');
  await newLiveConnection(page);
  await page.getByPlaceholder('e.g. Local development').fill('Live editable');
  await page.getByRole('button', { name: 'Save & connect', exact: true }).click();
  await expect(page.getByTestId('live-access')).toHaveText('Read-write');
  await page.locator('.table-tree button').filter({ hasText: /^accounts/ }).click();
  await expect(page.getByTestId('live-read-only')).toHaveCount(0);
  await expect(page.locator('.view-tabs [role=tab]')).toHaveText(['Browse', 'Structure', 'SQL', 'Insert', 'Export', 'Import']);
  const jobs = () => page.evaluate(() => (window as any).jobs as string[]);

  // Cell edit: one UPDATE addressed by the exact BIGINT UNSIGNED key, non-ASCII text exact.
  await page.getByText('Ada', { exact: true }).dblclick();
  const editor = page.locator('.cell-editor').getByRole('textbox').first();
  await editor.fill('Zoë');
  await editor.press('Enter');
  await expect(page.getByText('1 row updated.')).toBeVisible();
  expect((await jobs()).at(-1)).toBe("UPDATE `accounts` SET `name` = _utf8mb4 X'5a6fc3ab' WHERE `id` = _utf8mb4 X'39303037313939323534373430393933' LIMIT 1;");

  // Stale row: the statement matched nothing; the editor stays open with the conflict.
  await page.evaluate(() => { (window as any).affected = '0'; });
  await page.getByText('Ada', { exact: true }).dblclick();
  await page.locator('.cell-editor').getByRole('textbox').first().fill('Grace');
  await page.locator('.cell-editor').getByRole('textbox').first().press('Enter');
  await expect(page.locator('.cell-editor')).toContainText('changed or deleted since it was loaded');
  // Retrying after the conflict sends the statement again only on the user's action.
  await page.evaluate(() => { (window as any).affected = '1'; });
  await page.locator('.cell-editor').getByRole('textbox').first().press('Enter');
  await expect(page.locator('.cell-editor')).toHaveCount(0);
  expect((await jobs()).length).toBe(3);

  // Edit form on an existing row: only the changed column is written.
  await page.getByRole('button', { name: 'Edit record 9007199254740993' }).click();
  const form = page.locator('form.record-form[aria-label="Edit record"]');
  await form.getByLabel('Value for name', { exact: true }).fill('Grace');
  await form.locator('button.primary').click();
  await expect(page.getByText('1 row updated.')).toBeVisible();
  expect((await jobs()).at(-1)).toBe("UPDATE `accounts` SET `name` = _utf8mb4 X'4772616365' WHERE `id` = _utf8mb4 X'39303037313939323534373430393933' LIMIT 1;");

  // Delete by primary key after confirmation.
  await page.getByRole('button', { name: 'Delete record 9007199254740994' }).click();
  await page.getByRole('button', { name: 'Delete 1 record', exact: true }).click();
  await expect(page.getByText('1 record(s) deleted.')).toBeVisible();
  expect((await jobs()).at(-1)).toBe("DELETE FROM `accounts` WHERE `id` = _utf8mb4 X'39303037313939323534373430393934' LIMIT 1;");

  // Insert: columns left on their default are omitted, so the server applies its own defaults.
  await page.getByRole('tab', { name: 'Insert', exact: true }).click();
  await page.getByRole('button', { name: /Save|Insert/ }).filter({ hasText: /^(Save|Insert)/ }).first().click();
  await expect(page.getByText('1 row inserted.')).toBeVisible();
  expect((await jobs()).at(-1)).toBe('INSERT INTO `accounts` () VALUES ();');
  expect((await jobs()).length).toBe(6);

  // CSV import: one transaction; the batch runs inside the lease and is committed.
  await page.getByRole('tab', { name: 'Import', exact: true }).click();
  await page.locator('input[type="file"]').setInputFiles({ name: 'accounts.csv', mimeType: 'text/csv', buffer: Buffer.from('"name"\n"Lin"\nNULL\n""\n') });
  await page.getByRole('button', { name: /^Import/ }).last().click();
  await expect(page.getByText('3 records imported into accounts in one transaction.')).toBeVisible();
  expect((await jobs()).at(-1)).toBe("INSERT INTO `accounts` (`name`) VALUES\n(_utf8mb4 X'4c696e'),\n(NULL),\n(_utf8mb4 X'');");
  expect(await page.evaluate(() => [(window as any).leased.at(-1), (window as any).ends.at(-1)])).toEqual(['lease_1', 'commit']);
  // A rejected batch rolls the whole import back and names the records.
  await page.evaluate(() => { (window as any).failNext = true; });
  await page.locator('input[type="file"]').setInputFiles({ name: 'more.csv', mimeType: 'text/csv', buffer: Buffer.from('"name"\n"dup"\n') });
  await page.getByRole('button', { name: /^Import/ }).last().click();
  await expect(page.getByText(/Records 2–2: The database rejected statement 1 of 1.*Import rolled back/)).toBeVisible();
  expect(await page.evaluate(() => (window as any).ends.at(-1))).toBe('rollback');

  // Metadata invalidation: a schema statement run from the SQL tab re-reads the catalog.
  await page.getByRole('tab', { name: 'SQL', exact: true }).click();
  await page.locator('.cm-content').fill('CREATE TABLE audit_log (id INT PRIMARY KEY)');
  await page.getByRole('button', { name: 'Run query', exact: true }).click();
  await expect(page.locator('.table-tree button').filter({ hasText: /^audit_log/ })).toBeVisible();
});
