import { test, expect } from '@playwright/test';
import { newLiveConnection } from './desktop';
// MySQL/MariaDB: Explorer lists every database the account can see; the profile's default
// database is the initial one. Opening another database switches the session (same handle),
// so Browse and SQL run against it. A refused switch changes nothing and says why.
for (const theme of ['light', 'dark']) test(`${theme}: Explorer lists databases and switching retargets Browse and SQL`, async ({ page }) => {
  await page.addInitScript((theme) => {
    const w = window as any;
    localStorage.setItem('stoneql-theme', theme);
    w.calls = []; w.current = 'acme_store';
    const col = (name: string, databaseType = 'VARCHAR') => ({ name, databaseType });
    const result = (columns: any[], rows: string[][]) => ({ index: 0, affectedRows: '0', truncated: false, columns, rows: rows.map((r) => r.map((text) => ({ kind: 'text', text }))) });
    const tables: Record<string, string> = { acme_store: 'orders', analytics: 'events' };
    w.__TAURI__ = { core: { invoke: async (command: string, args: { message: string }) => {
      const request = JSON.parse(args.message), p = request.payload;
      w.calls.push({ command, payload: p, database: w.current });
      let payload: any;
      if (command === 'host_info') payload = { runtime: 'desktop', appVersion: '0.1.0', sessionId: 's', nonce: p.nonce, simulated: true, readWriteSql: true };
      else if (command === 'contract_probe') payload = { values: p.values, simulated: true };
      else if (command === 'list_unresolved_outcomes') payload = { outcomes: [], limit: 16 };
      else if (command === 'connect_direct') { w.current = p.database; payload = { handle: '00000000-0000-0000-0000-0000000000db', engineVersion: '8.4.11', database: p.database, expiresAfterSeconds: 3600, access: 'read_write' }; }
      else if (command === 'database_status') payload = { connected: true };
      else if (command === 'switch_database') {
        if (p.database === 'secret') throw { code: 'PERMISSION_DENIED', message: 'The server denied this account access; nothing was changed.' };
        w.current = p.database; payload = { database: p.database };
      } else if (command === 'read_only_query') {
        const sql: string = p.sql;
        if (sql.includes('information_schema.SCHEMATA')) payload = result([col('name')], [['acme_store'], ['analytics'], ['secret']]);
        else if (sql.includes('information_schema.TABLES')) payload = result([col('name'), col('kind')], [[tables[w.current] ?? 'none', 'BASE TABLE']]);
        else if (sql.includes('information_schema.COLUMNS')) payload = result(['name', 'type', 'nullable', 'ckey', 'dflt', 'extra'].map((n) => col(n)), [['id', 'int', 'NO', 'PRI', '', ''], ['source', 'varchar(20)', 'YES', '', '', '']]);
        else if (sql.includes('STATISTICS') || sql.includes('KEY_COLUMN_USAGE') || sql.includes('REFERENTIAL')) payload = result([col('name')], []);
        else if (sql.startsWith('SELECT COUNT(*)')) payload = result([col('total', 'BIGINT')], [['1']]);
        else payload = result([col('id', 'INT'), col('source')], [['1', `row from ${w.current}`]]);
      } else throw { code: 'INVALID_REQUEST' };
      return { version: 1, requestId: request.requestId, payload };
    } } };
  }, theme);
  await page.goto('/');
  await newLiveConnection(page);
  await page.getByPlaceholder('e.g. Local development').fill('Two databases');
  await page.getByRole('button', { name: 'Save & connect', exact: true }).click();
  const tree = page.locator('.database-tree');
  const row = (name: string) => tree.getByRole('button', { name: new RegExp(`^${name}, `) });
  // Every visible database; the default one is current and listed with its tables.
  await expect(tree.locator('.database-row strong')).toHaveText(['acme_store', 'analytics', 'secret']);
  await expect(row('acme_store')).toHaveAttribute('aria-expanded', 'true');
  await expect(page.locator('.table-tree button')).toHaveText([/orders/]);

  // Open analytics: the session switches; its tables, Browse and SQL use it.
  await row('analytics').click();
  await expect(row('analytics')).toHaveAttribute('aria-expanded', 'true');
  await expect(page.getByRole('navigation', { name: 'Database breadcrumb' })).toContainText('analytics');
  const switches = await page.evaluate(() => (window as any).calls.filter((c: any) => c.command === 'switch_database').map((c: any) => c.payload.database));
  expect(switches).toEqual(['analytics']);
  await page.locator('.table-tree button').filter({ hasText: /^events/ }).click();
  await expect(page.getByText('row from analytics', { exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'SQL', exact: true }).click();
  await page.locator('.cm-content').fill('SELECT id, source FROM events');
  await page.getByRole('button', { name: 'Run query', exact: true }).click();
  const ran = await page.evaluate(() => (window as any).calls.filter((c: any) => ['read_only_query', 'submit_job'].includes(c.command)).at(-1));
  expect(ran.database).toBe('analytics');
  expect(ran.payload.handle).toBe('00000000-0000-0000-0000-0000000000db');

  // A refused switch: clear message, nothing changes.
  await row('secret').click();
  await expect(page.getByText('The server refused secret for this account (no access, or it does not exist); you are still using analytics.')).toBeVisible();
  await expect(row('secret')).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByRole('navigation', { name: 'Database breadcrumb' })).toContainText('analytics');
  await page.screenshot({ path: `test-results/live-databases-${theme}.png`, fullPage: true });
});
