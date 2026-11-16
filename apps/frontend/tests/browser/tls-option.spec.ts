import { test, expect } from '@playwright/test';
// First launch is Light even when the OS is dark. The live connection dialog offers "Use TLS"
// only for direct MySQL/MariaDB to this computer (default off for new connections); other hosts
// and engines require verified TLS. The renderer sends `tls: false` only for that local choice.
test('first launch is Light with a dark OS; Use TLS is offered only for local MySQL and sent as chosen', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.addInitScript(() => {
    const w = window as any;
    w.calls = [];
    w.__TAURI__ = { core: { invoke: async (command: string, args: { message: string }) => {
      const request = JSON.parse(args.message), p = request.payload;
      w.calls.push({ command, payload: p });
      let payload: any;
      if (command === 'host_info') payload = { runtime: 'desktop', appVersion: '0.1.0', sessionId: 's', nonce: p.nonce, simulated: true };
      else if (command === 'contract_probe') payload = { values: p.values, simulated: true };
      else if (command === 'list_unresolved_outcomes') payload = { outcomes: [], limit: 16 };
      else if (command === 'connect_direct') payload = { handle: '00000000-0000-0000-0000-00000000000b', engineVersion: 'MySQL fixture 8.4.6', database: p.database, expiresAfterSeconds: 3600, access: 'read_only' };
      else if (command === 'disconnect_database') payload = { connected: false };
      else if (command === 'database_status') payload = { connected: true };
      else if (command === 'read_only_query') payload = { index: 0, affectedRows: '0', truncated: false, columns: [{ name: 'name', databaseType: 'VARCHAR' }, { name: 'kind', databaseType: 'VARCHAR' }], rows: [] };
      else throw { code: 'INVALID_REQUEST' };
      return { version: 1, requestId: request.requestId, payload };
    } } };
  });
  await page.goto('/');
  await expect(page.locator('html')).not.toHaveClass(/\bdark\b/);
  expect(await page.evaluate(() => localStorage.getItem('stoneql-theme'))).toBeNull();

  await page.getByTitle('Manage connections', { exact: true }).click();
  await page.getByRole('button', { name: 'New connection', exact: true }).click();
  await page.getByLabel('Connection mode', { exact: true }).selectOption('live');
  const option = page.getByTestId('tls-option');
  const useTls = option.getByRole('checkbox', { name: 'Use TLS' });
  const ca = page.getByLabel('Trusted CA certificate');
  const host = page.getByRole('textbox', { name: 'Host' });
  // New connection to 127.0.0.1: offered, off, no CA field.
  await expect(useTls).not.toBeChecked();
  await expect(option).toContainText('TLS off: traffic to this computer is not encrypted.');
  await expect(ca).toHaveCount(0);
  await useTls.check();
  await expect(option).toContainText('The server certificate and host name are verified.');
  await expect(ca).toHaveCount(1);
  await useTls.uncheck();
  // localhost and ::1 are this computer too.
  for (const local of ['localhost', '::1']) {
    await host.fill(local);
    await expect(useTls).toBeVisible();
  }
  // Another host: no checkbox, verified TLS required.
  await host.fill('db.example.com');
  await expect(useTls).toHaveCount(0);
  await expect(option).toHaveText('Verified TLS is required for connections to other computers.');
  await expect(ca).toHaveCount(1);
  // PostgreSQL to this computer: no TLS option at all.
  await host.fill('127.0.0.1');
  await page.getByLabel('Database engine').selectOption('PostgreSQL');
  await expect(option).toHaveCount(0);
  await page.getByLabel('Database engine').selectOption('MySQL');

  // Connecting locally with TLS off sends tls:false and no CA; the notice says so.
  await page.getByPlaceholder('e.g. Local development').fill('Local plain');
  await page.getByRole('button', { name: 'Save & connect', exact: true }).click();
  await expect(page.getByText('Live read-only connection opened without TLS (this computer only).')).toBeVisible();
  const connect = await page.evaluate(() => (window as any).calls.find((c: any) => c.command === 'connect_direct').payload);
  expect(connect).toMatchObject({ host: '127.0.0.1', tls: false, rootCertificatePem: null });
});
