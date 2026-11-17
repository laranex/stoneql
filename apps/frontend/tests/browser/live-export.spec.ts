import { test, expect } from '@playwright/test';
import { newLiveConnection } from './desktop';
// Simulated native bridge: a read-only MySQL session exports the open table. The renderer only
// sees a grant, a transfer id and status; the outcome (including refusals) is shown verbatim.
test('live export streams through a native grant and reports refusals without creating a file', async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as any;
    w.calls = [];
    const col = (name: string, databaseType = 'VARCHAR') => ({ name, databaseType });
    const result = (columns: any[], values: (string | null)[][]) => ({ index: 0, affectedRows: '0', truncated: false, columns, rows: values.map((r) => r.map((v) => (v === null ? { kind: 'null' } : { kind: 'text', text: v }))) });
    let polls = 0;
    w.__TAURI__ = { core: { invoke: async (command: string, args: { message: string }) => {
      const request = JSON.parse(args.message), p = request.payload;
      w.calls.push({ command, payload: p });
      let payload: any;
      if (command === 'host_info') payload = { runtime: 'desktop', appVersion: '0.1.0', sessionId: 's', nonce: p.nonce, simulated: true };
      else if (command === 'contract_probe') payload = { values: p.values, simulated: true };
      else if (command === 'list_unresolved_outcomes') payload = { outcomes: [], limit: 16 };
      else if (command === 'connect_direct') payload = { handle: '00000000-0000-0000-0000-00000000000e', engineVersion: 'MySQL fixture 8.4.6', database: p.database, expiresAfterSeconds: 3600 };
      else if (command === 'database_status') payload = { connected: true };
      else if (command === 'pick_export_file') payload = { grant: `grant_${p.format}`, fileName: p.suggestedName };
      else if (command === 'start_export') { polls = 0; payload = { transferId: `transfer_${p.format}` }; }
      else if (command === 'export_status') {
        polls++;
        const base = { transferId: p.transferId, fileName: p.transferId === 'transfer_sql' ? 'accounts.sql' : 'accounts.csv', error: null, detail: null };
        payload = p.transferId === 'transfer_csv'
          ? { ...base, state: 'failed', rows: '0', bytes: '20', error: 'UNSUPPORTED_CAPABILITY', detail: 'column b holds binary data, which CSV cannot represent exactly; export as SQL' }
          : polls < 2 ? { ...base, state: 'running', rows: '7', bytes: '300' } : { ...base, state: 'succeeded', rows: '40', bytes: '4096' };
      } else if (command === 'read_only_query') {
        const sql: string = p.sql;
        if (sql.includes('information_schema.TABLES')) payload = result([col('name'), col('kind')], [['accounts', 'BASE TABLE']]);
        else if (sql.includes('information_schema.COLUMNS')) payload = result(['name', 'type', 'nullable', 'ckey', 'dflt', 'extra'].map((n) => col(n)), [['id', 'int', 'NO', 'PRI', null, '']]);
        else if (sql.includes('STATISTICS') || sql.includes('KEY_COLUMN_USAGE')) payload = result([col('name')], []);
        else if (sql.startsWith('SELECT COUNT(*)')) payload = result([col('total', 'BIGINT')], [['1']]);
        else payload = result([col('id', 'INT')], [['1']]);
      } else throw { code: 'INVALID_REQUEST' };
      return { version: 1, requestId: request.requestId, payload };
    } } };
  });
  await page.goto('/');
  await newLiveConnection(page);
  await page.getByPlaceholder('e.g. Local development').fill('Live export');
  await page.getByRole('button', { name: 'Save & connect', exact: true }).click();
  // Database level, Quick: one SQL file with every table (tables: null), FK checks off; CSV
  // needs exactly one table.
  await page.getByRole('tab', { name: 'Export', exact: true }).click();
  const panel = page.getByTestId('live-export');
  await expect(panel).toContainText('every table of acme_store');
  await expect(panel.getByRole('combobox', { name: 'Export format' }).locator('option[value="csv"]')).toBeDisabled();
  await panel.getByRole('button', { name: 'Choose file and export' }).click();
  await expect(panel).toContainText('Exported 40 rows');
  const calls = () => page.evaluate(() => (window as any).calls as { command: string; payload: any }[]);
  const last = async (command: string) => (await calls()).filter((c) => c.command === command).at(-1)!.payload;
  expect(await last('pick_export_file')).toEqual({ suggestedName: 'acme_store.sql', format: 'sql', sessionId: 's' });
  expect(await last('start_export')).toMatchObject({ tables: null, format: 'sql', structure: true, data: true, options: { dropTable: false, rowsPerInsert: 500, insertVerb: 'insert', disableForeignKeys: true, compress: false } });

  // Custom: chosen tables, Data only, one row per INSERT, REPLACE, transaction, GZIP.
  await panel.getByLabel(/^Custom/).check();
  await panel.getByRole('combobox', { name: 'Export content' }).selectOption('data');
  await expect(panel.getByLabel('Add DROP TABLE statement')).toBeDisabled();
  await panel.getByRole('button', { name: 'Deselect all' }).click();
  await panel.getByRole('button', { name: 'Choose file and export' }).click();
  await expect(panel.getByRole('alert')).toContainText('Select at least one table.');
  await panel.getByLabel('Tables to export').getByLabel('accounts').check();
  await panel.getByRole('combobox', { name: 'INSERT statement style' }).selectOption('replace');
  await panel.getByLabel('Enclose data in a transaction').check();
  await panel.getByLabel('GZIP').check();
  const starts = async () => (await calls()).filter((c) => c.command === 'start_export').length;
  await panel.getByRole('button', { name: 'Choose file and export' }).click();
  await expect.poll(starts).toBe(2);
  await expect(panel.getByRole('status')).toContainText('Exported 40 rows');
  expect(await last('pick_export_file')).toEqual({ suggestedName: 'acme_store.sql.gz', format: 'sql', compressed: true, sessionId: 's' });
  expect(await last('start_export')).toMatchObject({ tables: ['accounts'], structure: false, data: true, options: { rowsPerInsert: 500, insertVerb: 'replace', transaction: true, compress: true } });
  await page.screenshot({ path: 'test-results/live-export-custom.png', fullPage: true });
  // Back to Quick: the Custom content choice no longer applies (structure and data).
  await panel.getByLabel(/^Quick/).check();
  await panel.getByRole('button', { name: 'Choose file and export' }).click();
  await expect.poll(starts).toBe(3);
  expect(await last('start_export')).toMatchObject({ tables: null, structure: true, data: true, options: { insertVerb: 'insert', transaction: false } });

  // Table level: CSV with custom separator, enclosure and NULL text; a refusal is shown verbatim.
  await page.locator('.table-tree button').filter({ hasText: /^accounts/ }).click();
  await page.getByRole('tab', { name: 'Export', exact: true }).click();
  const table = page.getByTestId('live-export');
  await expect(table).toContainText('Live export of accounts');
  await table.getByLabel(/^Custom/).check();
  await table.getByRole('combobox', { name: 'Export format' }).selectOption('csv');
  await table.getByLabel('CSV delimiter').fill(';');
  await table.getByRole('combobox', { name: 'CSV enclosure' }).selectOption('');
  await table.getByLabel('CSV null value').fill('\\N');
  await table.getByRole('button', { name: 'Choose file and export' }).click();
  await expect(table.getByRole('alert')).toContainText('Export failed: column b holds binary data, which CSV cannot represent exactly; export as SQL. No file was created or replaced.');
  expect(await last('start_export')).toMatchObject({ tables: ['accounts'], format: 'csv', structure: false, options: { csvDelimiter: ';', csvEnclosure: null, csvNull: '\\N', csvHeader: true } });
});
