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
  // Database level: one SQL file with every table (table: null); CSV is not offered.
  await page.getByRole('tab', { name: 'Export', exact: true }).click();
  const whole = page.getByTestId('live-export');
  await expect(whole).toContainText('every table of acme_store');
  await expect(whole.getByLabel('CSV')).toHaveCount(0);
  await whole.getByRole('button', { name: 'Choose file and export' }).click();
  await expect(whole).toContainText('Exported 40 rows');
  const first = await page.evaluate(() => (window as any).calls.filter((c: any) => ['pick_export_file', 'start_export'].includes(c.command)).map((c: any) => c.payload));
  expect(first[0]).toEqual({ suggestedName: 'acme_store.sql', format: 'sql', sessionId: 's' });
  expect(first[1]).toMatchObject({ table: null, format: 'sql', structure: true, data: true });
  await page.evaluate(() => { (window as any).calls.length = 0; });
  await page.locator('.table-tree button').filter({ hasText: /^accounts/ }).click();
  await page.getByRole('tab', { name: 'Export', exact: true }).click();
  const panel = page.getByTestId('live-export');
  await panel.getByRole('button', { name: 'Choose file and export' }).click();
  await expect(panel).toContainText('Exported 40 rows (4096 bytes) to accounts.sql.');
  const calls = () => page.evaluate(() => (window as any).calls as { command: string; payload: any }[]);
  const start = (await calls()).find((c) => c.command === 'start_export')!.payload;
  expect(start).toEqual({ handle: '00000000-0000-0000-0000-00000000000e', grant: 'grant_sql', table: 'accounts', format: 'sql', structure: true, data: true, sessionId: 's' });
  expect((await calls()).find((c) => c.command === 'pick_export_file')!.payload).toEqual({ suggestedName: 'accounts.sql', format: 'sql', sessionId: 's' });
  await panel.getByLabel('CSV').check();
  await panel.getByRole('button', { name: 'Choose file and export' }).click();
  await expect(panel.getByRole('alert')).toContainText('Export failed: column b holds binary data, which CSV cannot represent exactly; export as SQL. No file was created or replaced.');
});
