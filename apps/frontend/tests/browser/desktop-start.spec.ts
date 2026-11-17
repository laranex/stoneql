import { test, expect } from '@playwright/test';
// The desktop app shows real connections only: sample and mock profiles saved by earlier builds
// are dropped, there is no Mock mode, the manager can always be closed, and with no connections
// the window still closes normally (it used to fail and fall back to "not responding").
for (const theme of ['light', 'dark']) test(`${theme}: desktop start has no samples or Mock mode and closes with no connections`, async ({ page }) => {
  await page.addInitScript((theme) => {
    const w = window as any;
    localStorage.setItem('stoneql-theme', theme);
    if (!sessionStorage.getItem('seeded')) {
      sessionStorage.setItem('seeded', '1');
      localStorage.setItem('stoneql-connections', JSON.stringify({ version: 1, groups: [], connections: [
        { id: 'sample-1', name: 'Local MySQL', engine: 'MySQL', host: '127.0.0.1', port: 3306, user: 'root', database: 'acme_store', transport: { type: 'direct' } },
        { id: 'mock-1', name: 'Old mock', mode: 'mock', engine: 'PostgreSQL', host: '127.0.0.1', port: 5432, user: 'root', database: 'acme_store', transport: { type: 'direct' } },
      ] }));
    }
    w.closeActions = [];
    w.__TAURI__ = { core: { invoke: async (command: string, args: { message: string }) => {
      const request = JSON.parse(args.message), p = request.payload;
      const payload = command === 'host_info' ? { runtime: 'desktop', appVersion: '0.1.0', sessionId: 's', nonce: p.nonce, simulated: true }
        : command === 'resolve_close' ? (w.closeActions.push(p.action), { closed: p.action === 'confirm' })
        : command === 'list_unresolved_outcomes' ? { outcomes: [], limit: 16 }
        : { simulated: true, values: p.values };
      return { version: 1, requestId: request.requestId, payload };
    } } };
  }, theme);
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Welcome to StoneQL' })).toBeVisible();
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  await expect(page.locator('.footer-session')).not.toContainText(/mock/i);
  expect(JSON.parse((await page.evaluate(() => localStorage.getItem('stoneql-connections')))!).connections).toEqual([]);

  await page.getByTitle('Manage connections', { exact: true }).click();
  const manager = page.getByRole('dialog', { name: 'Connection manager' });
  await expect(manager).toContainText('No connections yet');
  await expect(manager).not.toContainText(/Prototype mode|simulated/);
  await manager.getByRole('button', { name: 'New connection', exact: true }).click();
  await expect(page.getByLabel('Connection mode', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('textbox', { name: 'Username' })).toHaveValue('');
  await expect(page.getByRole('textbox', { name: 'Default database' })).toHaveValue('');
  await expect(page.getByTestId('tls-option')).toBeVisible();
  await page.screenshot({ path: `test-results/desktop-start-${theme}.png` });
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await manager.getByRole('button', { name: 'Close connection manager', exact: true }).click();
  await expect(manager).toHaveCount(0);

  // Closing the window with nothing open or unsaved is confirmed without a prompt.
  await page.evaluate(() => window.dispatchEvent(new Event('stoneql-close-requested')));
  await expect.poll(() => page.evaluate(() => (window as any).closeActions)).toEqual(['prompted', 'confirm']);
  expect(errors).toEqual([]);
});
for (const theme of ['light', 'dark']) test(`${theme}: names are typed exactly and a successful test offers the server's databases`, async ({ page }) => {
  await page.addInitScript((theme) => {
    const w = window as any;
    localStorage.setItem('stoneql-theme', theme);
    w.calls = [];
    w.__TAURI__ = { core: { invoke: async (command: string, args: { message: string }) => {
      const request = JSON.parse(args.message), p = request.payload;
      w.calls.push({ command, payload: p });
      const payload = command === 'host_info' ? { runtime: 'desktop', appVersion: '0.1.0', sessionId: 's', nonce: p.nonce, simulated: true, readWriteSql: true }
        : command === 'list_unresolved_outcomes' ? { outcomes: [], limit: 16 }
        : command === 'connect_direct' ? { handle: '00000000-0000-0000-0000-00000000000c', engineVersion: '8.4.11', database: p.database, expiresAfterSeconds: 3600, access: p.access ?? 'read_only' }
        : command === 'read_only_query' ? { index: 0, affectedRows: '0', truncated: false, columns: [{ name: 'name', databaseType: 'VARCHAR' }], rows: [[{ kind: 'text', text: 'information_schema' }], [{ kind: 'text', text: 'moodle' }], [{ kind: 'text', text: 'shop' }]] }
        : command === 'disconnect_database' ? { connected: false }
        : { simulated: true, values: p.values };
      return { version: 1, requestId: request.requestId, payload };
    } } };
  }, theme);
  await page.goto('/');
  await page.getByTitle('Manage connections', { exact: true }).click();
  await page.getByRole('button', { name: 'New connection', exact: true }).click();
  const database = page.getByRole('textbox', { name: 'Default database' });
  for (const field of [page.getByRole('textbox', { name: 'Host' }), page.getByRole('textbox', { name: 'Username' }), database]) {
    await expect(field).toHaveAttribute('autocapitalize', 'off');
    await expect(field).toHaveAttribute('autocorrect', 'off');
    await expect(field).toHaveAttribute('spellcheck', 'false');
  }
  await page.getByPlaceholder('e.g. Local development').fill('Local');
  await page.getByRole('textbox', { name: 'Username' }).fill('thixpin');
  await expect(page.getByRole('combobox', { name: 'Databases on this server' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Test connection', exact: true }).click();
  await expect(page.getByText(/Choose the default database below\./)).toBeVisible();
  const choose = page.getByRole('combobox', { name: 'Databases on this server' });
  await choose.selectOption('moodle');
  await expect(database).toHaveValue('moodle');
  await page.screenshot({ path: `test-results/database-choice-${theme}.png` });
  await page.getByRole('button', { name: 'Save & connect', exact: true }).click();
  await expect(page.locator('.footer-session')).toContainText('Live read-write · connected');
  const connects = await page.evaluate(() => (window as any).calls.filter((c: any) => c.command === 'connect_direct').map((c: any) => [c.payload.database, c.payload.access ?? null]));
  expect(connects).toEqual([['information_schema', null], ['moodle', 'read_write']]);
});
