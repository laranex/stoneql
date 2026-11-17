import { test, expect } from '@playwright/test';
import { newLiveConnection } from './desktop';
// Read-write MySQL: Structure adds, renames, redefines and drops columns, and Explorer creates
// databases. The exact previewed SQL is what runs; destructive changes are confirmed; a
// privilege refusal is reported clearly; Structure and Explorer refresh after success.
const createTable = "CREATE TABLE `accounts` (\n  `id` int NOT NULL AUTO_INCREMENT,\n  `name` varchar(40) COLLATE utf8mb4_bin DEFAULT NULL COMMENT 'display; name',\n  PRIMARY KEY (`id`)\n) ENGINE=InnoDB";
for (const theme of ['light', 'dark']) test(`${theme}: live column and database changes preview their SQL and confirm destructive ones`, async ({ page }) => {
  await page.addInitScript(([theme, createTable]) => {
    const w = window as any;
    localStorage.setItem('stoneql-theme', theme);
    w.jobs = []; w.denyNext = false; w.databases = ['acme_store'];
    const col = (name: string, databaseType = 'VARCHAR') => ({ name, databaseType });
    const result = (columns: any[], rows: string[][]) => ({ index: 0, affectedRows: '0', truncated: false, columns, rows: rows.map((r) => r.map((text) => ({ kind: 'text', text }))) });
    const hex = (text: string) => Array.from(new TextEncoder().encode(text), (b) => b.toString(16).padStart(2, '0')).join('');
    let last = '';
    w.__TAURI__ = { core: { invoke: async (command: string, args: { message: string }) => {
      const request = JSON.parse(args.message), p = request.payload;
      let payload: any;
      if (command === 'host_info') payload = { runtime: 'desktop', appVersion: '0.1.0', sessionId: 's', nonce: p.nonce, simulated: true, readWriteSql: true };
      else if (command === 'contract_probe') payload = { values: p.values, simulated: true };
      else if (command === 'list_unresolved_outcomes') payload = { outcomes: [], limit: 16 };
      else if (command === 'connect_direct') payload = { handle: '00000000-0000-0000-0000-0000000000dd', engineVersion: '8.4.11', database: p.database, expiresAfterSeconds: 3600, access: 'read_write' };
      else if (command === 'database_status') payload = { connected: true };
      else if (command === 'submit_job') { last = p.sql; if (!p.sql.startsWith('SHOW')) w.jobs.push(p.sql); if (/^CREATE DATABASE `shop`/.test(p.sql) && !w.denyNext) w.databases.push('shop'); payload = { jobId: `job_${w.jobs.length}`, statementCount: 1 }; }
      else if (command === 'fetch_job') {
        const show = last.startsWith('SHOW CREATE TABLE'), denied = !show && w.denyNext;
        if (denied) w.denyNext = false;
        payload = { jobId: p.jobId, state: 'finished', outcome: denied ? 'failed' : 'succeeded', cancellation: 'none', serverCancelSent: false, deadlineExceeded: false,
          statements: [{ index: 0, status: denied ? 'failed' : 'succeeded', inTransaction: false, affectedRows: '0', resultSets: show ? 1 : 0, failure: denied ? { constraint: null, sqlState: '42000', denied: true } : null, resultsWithheld: false }],
          batch: show ? { statement: 0, result: 0, columns: [{ name: 'Table', databaseType: 'VARCHAR', nullable: null }, { name: 'Create Table', databaseType: 'BLOB', nullable: null }], rows: [[{ kind: 'text', text: 'accounts' }, { kind: 'binary', hex: hex(createTable) }]], last: true, truncated: false } : null,
          lease: null, done: true };
      } else if (command === 'read_only_query') {
        const sql: string = p.sql;
        if (sql.includes('information_schema.SCHEMATA')) payload = result([col('name')], w.databases.map((d: string) => [d]));
        else if (sql.includes('information_schema.TABLES')) payload = result([col('name'), col('kind')], [['accounts', 'BASE TABLE']]);
        else if (sql.includes('information_schema.COLUMNS')) payload = result(['name', 'type', 'nullable', 'ckey', 'dflt', 'extra'].map((n) => col(n)), [['id', 'int', 'NO', 'PRI', null as any, 'auto_increment'], ['name', 'varchar(40)', 'YES', '', null as any, '']]);
        else if (sql.includes('STATISTICS') || sql.includes('KEY_COLUMN_USAGE') || sql.includes('REFERENTIAL')) payload = result([col('name')], []);
        else if (sql.startsWith('SELECT COUNT(*)')) payload = result([col('total', 'BIGINT')], [['0']]);
        else payload = result([col('id', 'INT'), col('name')], []);
      } else throw { code: 'INVALID_REQUEST' };
      return { version: 1, requestId: request.requestId, payload };
    } } };
  }, [theme, createTable] as const);
  await page.goto('/');
  await newLiveConnection(page);
  await page.getByPlaceholder('e.g. Local development').fill('Schema');
  await page.getByRole('button', { name: 'Save & connect', exact: true }).click();
  await page.locator('.table-tree button').filter({ hasText: /^accounts/ }).click();
  await page.getByRole('tab', { name: 'Structure', exact: true }).click();
  const dialog = page.getByTestId('live-ddl-dialog');
  const preview = dialog.getByTestId('ddl-preview');
  const jobs = () => page.evaluate(() => (window as any).jobs as string[]);

  // Add: previewed SQL is exactly what runs.
  await page.getByRole('button', { name: 'Add column', exact: true }).click();
  await dialog.getByRole('textbox', { name: 'Column name' }).fill('note');
  await dialog.getByRole('textbox', { name: 'Column definition' }).fill('VARCHAR(20) NULL');
  await dialog.getByRole('combobox', { name: 'Column position' }).selectOption('after:id');
  await expect(preview).toHaveText('ALTER TABLE `accounts` ADD COLUMN `note` VARCHAR(20) NULL AFTER `id`;');
  await page.screenshot({ path: `test-results/live-structure-add-${theme}.png` });
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(await jobs()).toEqual(['ALTER TABLE `accounts` ADD COLUMN `note` VARCHAR(20) NULL AFTER `id`;']);

  // Rename only: the server definition (collation, comment) is kept via RENAME COLUMN.
  await page.getByRole('button', { name: 'Change column name' }).click();
  await expect(dialog.getByRole('textbox', { name: 'Column definition' })).toHaveValue("varchar(40) COLLATE utf8mb4_bin DEFAULT NULL COMMENT 'display; name'");
  await dialog.getByRole('textbox', { name: 'Column name' }).fill('full_name');
  await expect(preview).toHaveText('ALTER TABLE `accounts` RENAME COLUMN `name` TO `full_name`;');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect((await jobs()).at(-1)).toBe('ALTER TABLE `accounts` RENAME COLUMN `name` TO `full_name`;');

  // Redefine: confirmed as destructive before it runs.
  await page.getByRole('button', { name: 'Change column name' }).click();
  await dialog.getByRole('textbox', { name: 'Column definition' }).fill("varchar(80) COLLATE utf8mb4_bin DEFAULT NULL COMMENT 'display; name'");
  await expect(preview).toHaveText("ALTER TABLE `accounts` CHANGE COLUMN `name` `name` varchar(80) COLLATE utf8mb4_bin DEFAULT NULL COMMENT 'display; name';");
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  const confirm = page.getByRole('alertdialog');
  await expect(confirm).toContainText('can convert or truncate stored values');
  await confirm.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(await jobs()).toHaveLength(2);
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await confirm.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect((await jobs()).at(-1)).toContain('CHANGE COLUMN `name` `name` varchar(80)');

  // Drop: confirmed, with the SQL shown.
  await page.getByRole('button', { name: 'Drop column name' }).click();
  await expect(confirm).toContainText('ALTER TABLE `accounts` DROP COLUMN `name`;');
  await confirm.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect.poll(async () => (await jobs()).at(-1)).toBe('ALTER TABLE `accounts` DROP COLUMN `name`;');

  // A privilege refusal is explained in the dialog; nothing is claimed applied.
  await page.evaluate(() => { (window as any).denyNext = true; });
  await page.getByRole('button', { name: 'Add column', exact: true }).click();
  await dialog.getByRole('textbox', { name: 'Column name' }).fill('blocked');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog.getByRole('alert')).toHaveText('The server denied this account the change (it needs the ALTER privilege). Nothing was changed.');
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();

  // Create database: previewed, run, and listed in Explorer.
  await page.getByRole('button', { name: 'Create database', exact: true }).click();
  await dialog.getByRole('textbox', { name: 'Database name' }).fill('shop');
  await expect(preview).toHaveText('CREATE DATABASE `shop`;');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('.database-tree .database-row strong')).toHaveText(['acme_store', 'shop']);
});
