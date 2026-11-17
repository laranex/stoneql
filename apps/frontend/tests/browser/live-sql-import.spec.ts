import { test, expect } from '@playwright/test';
import { newLiveConnection } from './desktop';
// Simulated native bridge: a read-write MySQL session imports a SQL file through a native open
// grant. The renderer sends only the handle and the one-shot grant; refusals and the first
// failure are shown with their statement number and line, in both themes.
for (const colorScheme of ['light', 'dark'] as const) {
  test(`${colorScheme}: live SQL import uses one-shot native grants and reports refusals and the first failure`, async ({ page }) => {
    await page.emulateMedia({ colorScheme });
    await page.addInitScript(() => {
      const w = window as any;
      w.calls = [];
      const col = (name: string, databaseType = 'VARCHAR') => ({ name, databaseType });
      const result = (columns: any[], values: (string | null)[][]) => ({ index: 0, affectedRows: '0', truncated: false, columns, rows: values.map((r) => r.map((v) => (v === null ? { kind: 'null' } : { kind: 'text', text: v }))) });
      let grants = 0, polls = 0;
      w.__TAURI__ = { core: { invoke: async (command: string, args: { message: string }) => {
        const request = JSON.parse(args.message), p = request.payload;
        w.calls.push({ command, payload: p });
        let payload: any;
        if (command === 'host_info') payload = { runtime: 'desktop', appVersion: '0.1.0', sessionId: 's', nonce: p.nonce, simulated: true, readWriteSql: true };
        else if (command === 'contract_probe') payload = { values: p.values, simulated: true };
        else if (command === 'list_unresolved_outcomes') payload = { outcomes: [], limit: 16 };
        else if (command === 'connect_direct') payload = { handle: '00000000-0000-0000-0000-00000000000a', engineVersion: 'MySQL fixture 8.4.6', database: p.database, expiresAfterSeconds: 3600, access: p.access ?? 'read_only' };
        else if (command === 'database_status') payload = { connected: true };
        else if (command === 'pick_import_file') { grants++; payload = { grant: `grant_${grants}`, fileName: grants === 1 ? 'dump.sql' : 'other.sql' }; }
        else if (command === 'start_sql_import') {
          if (p.grant === 'grant_2') throw { code: 'UNSUPPORTED_SQL', message: 'Statement 2 (line 2): USE is not accepted in an import; it would move the following statements to another database. Nothing was run.', retryable: false, outcome: 'rejected' };
          polls = 0; payload = { importId: '00000000-0000-4000-8000-0000000000aa', statementCount: 3, bytes: '120' };
        } else if (command === 'sql_import_status') {
          polls++;
          const base = { importId: p.importId, cancellation: 'none', serverCancelSent: false, deadlineExceeded: false, statementCount: 3, unknown: null };
          payload = polls < 2
            ? { ...base, state: 'running', outcome: null, completed: 1, affectedRows: '1', failed: null, done: false }
            : { ...base, state: 'finished', outcome: 'failed', completed: 2, affectedRows: '2', failed: { index: 2, line: 5, failure: { constraint: 'unique', sqlState: '23000' } }, done: true };
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
    await page.getByPlaceholder('e.g. Local development').fill('Live import');
    await page.getByRole('button', { name: 'Save & connect', exact: true }).click();
    await expect(page.getByTestId('live-access')).toHaveText('Read-write');
    // A SQL file imports into the database: Import is offered at database level as well.
    await expect(page.locator('.view-tabs [role=tab]')).toHaveText(['SQL', 'Export', 'Import']);
    await page.getByRole('tab', { name: 'Import', exact: true }).click();
    await expect(page.getByTestId('database-import')).toContainText('Import a SQL file into');
    await expect(page.getByTestId('database-import').getByRole('button', { name: /Choose/ })).toBeVisible();
    await page.locator('.table-tree button').filter({ hasText: /^accounts/ }).click();
    await page.getByRole('tab', { name: 'Import', exact: true }).click();
    await page.getByRole('combobox', { name: 'Format' }).selectOption('SQL');
    const panel = page.getByTestId('live-sql-import');
    await expect(page.locator('input[type="file"]')).toHaveCount(0);
    await expect(panel.getByRole('button', { name: 'Import', exact: true })).toBeDisabled();
    await panel.getByRole('button', { name: 'Choose SQL file…' }).click();
    await expect(panel.getByTestId('sql-import-file')).toHaveText('dump.sql');
    await panel.getByRole('button', { name: 'Import', exact: true }).click();
    await expect(panel.getByTestId('sql-import-result')).toHaveText('Statement 3 (line 5) failed (SQLSTATE 23000). 2 earlier statements were committed; later statements were not run.');
    const calls = () => page.evaluate(() => (window as any).calls as { command: string; payload: any }[]);
    expect((await calls()).find((c) => c.command === 'start_sql_import')!.payload).toEqual({ handle: '00000000-0000-0000-0000-00000000000a', grant: 'grant_1', sessionId: 's' });
    // The grant was one-shot: Import is disabled until another file is chosen.
    await expect(panel.getByRole('button', { name: 'Import', exact: true })).toBeDisabled();
    await panel.getByRole('button', { name: 'Choose SQL file…' }).click();
    await panel.getByRole('button', { name: 'Import', exact: true }).click();
    await expect(panel.getByRole('alert')).toContainText('Statement 2 (line 2): USE is not accepted in an import; it would move the following statements to another database. Nothing was run.');
    expect((await calls()).filter((c) => c.command === 'start_sql_import').map((c) => c.payload.grant)).toEqual(['grant_1', 'grant_2']);
  });
}
