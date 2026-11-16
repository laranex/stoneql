import { test, expect } from '@playwright/test';
// Simulated native bridge: a MySQL catalog with one table and one view, and a 40-row table whose
// ids exceed JavaScript's safe integer range. Only read_only_query is ever used for browsing.
for (const [engine, theme] of [['MySQL', 'light'], ['MySQL', 'dark'], ['PostgreSQL', 'light']])
  test(`${theme}: live ${engine} catalog lists tables and browses server pages read-only`, async ({ page }) => {
    const postgres = engine === 'PostgreSQL';
    await page.addInitScript(([theme, postgres]) => {
      localStorage.setItem('stoneql-theme', theme);
      const w = window as any;
      w.liveSql = [];
      const col = (name: string, databaseType = 'VARCHAR') => ({ name, databaseType });
      const text = (value: string | null) => (value === null ? { kind: 'null' } : { kind: 'text', text: value });
      const rows = Array.from({ length: 40 }, (_, i) => [String(9007199254740993n + BigInt(i)), i === 3 ? null : `name ${i}`]);
      const result = (columns: any[], values: (string | null)[][]) => ({ index: 0, affectedRows: '0', truncated: false, columns, rows: values.map((r) => r.map(text)) });
      w.__TAURI__ = { core: { invoke: async (command: string, args: { message: string }) => {
        const request = JSON.parse(args.message), p = request.payload;
        let payload: any;
        if (command === 'host_info') payload = { runtime: 'desktop', appVersion: '0.1.0', sessionId: 's', nonce: p.nonce, simulated: true };
        else if (command === 'contract_probe') payload = { values: p.values, simulated: true };
        else if (command === 'list_unresolved_outcomes') payload = { outcomes: [], limit: 16 };
        else if (command === 'connect_direct') payload = { handle: '00000000-0000-0000-0000-000000000001', engineVersion: 'MySQL fixture 8.4.6', database: p.database, expiresAfterSeconds: 3600 };
        else if (command === 'database_status') payload = { connected: true };
        else if (command === 'disconnect_database') payload = { connected: false };
        else if (command === 'read_only_query') {
          const sql: string = p.sql;
          w.liveSql.push(sql);
          if (/information_schema\.tables/i.test(sql)) payload = result([col('name'), col('kind')], [['accounts', 'BASE TABLE'], ['active_accounts', 'VIEW']]);
          else if (postgres && sql.includes('information_schema.columns')) payload = result(['name', 'type', 'udt', 'len', 'prec', 'scale', 'dprec', 'nullable', 'dflt', 'ident', 'pk'].map((n) => col(n)), [['id', 'bigint', 'int8', null, '64', '0', null, 'NO', null, 'YES', '1'], ['name', 'character varying', 'varchar', '40', null, null, null, 'YES', null, 'NO', '0']]);
          else if (sql.includes('information_schema.STATISTICS')) payload = result([col('name'), col('nonunique'), col('col'), col('type')], [['PRIMARY', '0', 'id', 'BTREE']]);
          else if (sql.includes('pg_catalog.pg_indexes')) payload = result([col('name'), col('definition')], [['accounts_pkey', 'CREATE UNIQUE INDEX accounts_pkey ON public.accounts USING btree (id)']]);
          else if (sql.includes('KEY_COLUMN_USAGE') || sql.includes('pg_catalog.pg_constraint')) payload = result(['name', 'col', 'ref_table', 'ref_col', 'on_delete', 'on_update', 'pos'].map((n) => col(n)), [['accounts_owner', 'name', 'owners', 'login', postgres ? 'c' : 'CASCADE', postgres ? 'a' : 'NO ACTION', '2']]);
          else if (sql.includes('information_schema.COLUMNS')) payload = result([col('name'), col('type'), col('nullable'), col('ckey'), col('dflt'), col('extra')], [['id', 'bigint(20) unsigned', 'NO', 'PRI', null, 'auto_increment'], ['name', 'varchar(40)', 'YES', '', null, '']]);
          else {
            const filter = /LIKE '%(.*?)%'/.exec(sql)?.[1];
            let matching = filter ? rows.filter((r) => (r[1] ?? '').includes(filter)) : rows;
            if (/[`"]id[`"] DESC/.test(sql)) matching = [...matching].reverse();
            if (sql.startsWith('SELECT COUNT(*)') && filter === 'name 1') throw { code: 'DEADLINE_EXCEEDED' };
            if (sql.startsWith('SELECT COUNT(*)')) payload = result([col('total', 'BIGINT')], [[String(matching.length)]]);
            else {
              const mysql = /LIMIT (\d+), (\d+);$/.exec(sql), pg = /LIMIT (\d+) OFFSET (\d+);$/.exec(sql);
              const [offset, limit] = mysql ? [Number(mysql[1]), Number(mysql[2])] : [Number(pg![2]), Number(pg![1])];
              payload = result([col('id', 'BIGINT UNSIGNED'), col('name')], matching.slice(offset, offset + limit));
            }
          }
        } else throw { code: 'INVALID_REQUEST' };
        return { version: 1, requestId: request.requestId, payload };
      } } };
    }, [theme, postgres] as const);
    await page.goto('/');
    await page.getByTitle('Manage connections', { exact: true }).click();
    await page.getByRole('button', { name: 'New connection', exact: true }).click();
    await page.getByLabel('Connection mode', { exact: true }).selectOption('live');
    if (postgres) await page.getByLabel('Database engine', { exact: true }).selectOption('PostgreSQL');
    await page.getByPlaceholder('e.g. Local development').fill('Live catalog');
    await page.getByRole('button', { name: 'Save & connect', exact: true }).click();
    await expect(page.locator('.footer-session')).toContainText('Live read-only · connected');

    const tree = page.locator('.table-tree button');
    await expect(tree).toHaveText([/accounts/, /active_accounts\s*view/]);
    await tree.filter({ hasText: /^accounts/ }).click();
    await expect(page.locator('.view-tabs [role=tab]')).toHaveText(['Browse', 'Structure', 'SQL']);
    await expect(page.getByText('9007199254740993', { exact: true })).toBeVisible();
    await expect(page.locator('.browse-content')).toContainText('(40 total · live, read-only)');
    await expect(page.locator('.browse-content .row-actions, .browse-content .check-cell')).toHaveCount(0);
    await expect(page.locator('.selection-toolbar')).toHaveCount(0);
    await expect(page.locator('.sql-preview, .browse-content')).toContainText(postgres ? 'ORDER BY "id" ASC\nLIMIT 25 OFFSET 0;' : 'ORDER BY `id` ASC\nLIMIT 0, 25;');
    await page.getByText('9007199254740993', { exact: true }).dblclick();
    await expect(page.locator('.cell-editor')).toHaveCount(0);

    await page.getByRole('button', { name: /Next page/ }).first().click();
    await expect(page.getByText(String(9007199254740993n + 25n), { exact: true })).toBeVisible();
    await page.getByLabel(/Filter rows/i).fill('name 3');
    await expect(page.locator('.browse-content')).toContainText('(10 total · live, read-only)');
    const sql: string[] = await page.evaluate(() => (window as any).liveSql);
    expect(sql.every((s) => s.startsWith('SELECT '))).toBe(true);
    expect(sql.filter((s) => s.includes("LIKE '%name 3%'")).length).toBe(2);
    await page.getByLabel(/Filter rows/i).fill('name 1');
    await expect(page.locator('.browse-content')).toContainText('total unavailable: Operation timed out');
    await expect(page.getByText('name 10', { exact: true })).toBeVisible();

    await page.getByRole('tab', { name: 'Structure', exact: true }).click();
    await expect(page.locator('.structure-grid tbody tr')).toHaveCount(2);
    await expect(page.locator('.structure-grid')).toContainText(postgres ? 'character varying(40)' : 'bigint(20) unsigned');
    await expect(page.getByRole('button', { name: /Add column|Change|Drop/ })).toHaveCount(0);
    await expect(page.getByTestId('live-indexes')).toContainText(postgres ? 'CREATE UNIQUE INDEX accounts_pkey ON public.accounts USING btree (id)' : 'PRIMARY');
    await expect(page.getByTestId('live-foreign-keys')).toContainText('owners(login)');
    await expect(page.getByTestId('live-foreign-keys')).toContainText('CASCADE');
  });
