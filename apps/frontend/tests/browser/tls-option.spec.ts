import { test, expect } from '@playwright/test';
import { newLiveConnection } from './desktop';
// First launch is Light even when the OS is dark. The live connection dialog offers "Use TLS"
// for direct MySQL/MariaDB (off for new connections, local and remote, with a warning for
// another computer); other engines require verified TLS.
// With TLS off, "Password without TLS" chooses how a full sign-in's password is protected.
test('first launch is Light with a dark OS; Use TLS and the password-without-TLS choice are offered for direct MySQL and sent as chosen', async ({ page }) => {
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

  await newLiveConnection(page);
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
  // Another host: offered and off too (new connections), with a warning that the network path
  // is unprotected.
  await host.fill('db.example.com');
  await expect(useTls).not.toBeChecked();
  await expect(ca).toHaveCount(0);
  const rsa = page.getByTestId('rsa-option');
  await expect(option).toContainText('TLS off: queries, results and the server’s identity are not protected on the network.');
  await expect(rsa).toContainText('RSA encryption protects only the password; queries and results are still sent unencrypted.');
  await expect(rsa.getByRole('radio', { name: 'Don’t send it without TLS' })).toBeChecked();
  await rsa.getByRole('radio', { name: 'Fetch the key from the server without verifying it' }).check();
  await expect(rsa).toContainText('anyone who can impersonate this server on the network can send their own key and read your password');
  await rsa.getByRole('radio', { name: 'Encrypt it with the server’s public key (trusted)' }).check();
  await expect(rsa.getByLabel('Server RSA public key')).toBeVisible();
  await useTls.check();
  await expect(rsa).toHaveCount(0);
  await expect(ca).toHaveCount(1);
  // PostgreSQL to this computer: no TLS option at all.
  await host.fill('127.0.0.1');
  await page.getByLabel('Database engine').selectOption('PostgreSQL');
  await expect(option).toHaveCount(0);
  await page.getByLabel('Database engine').selectOption('MySQL');

  // Connecting locally with TLS off and a trusted key sends tls:false, the key (MySQL's trailing
  // NUL byte trimmed) and no CA; the notice says so.
  await expect(useTls).toBeChecked();
  await useTls.uncheck();
  await expect(option).toContainText('TLS off: traffic to this computer is not encrypted.');
  // The earlier password choice is kept while TLS is toggled.
  await expect(rsa.getByRole('radio', { name: 'Encrypt it with the server’s public key (trusted)' })).toBeChecked();
  const key = '-----BEGIN PUBLIC KEY-----\nMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAsynthetic\n-----END PUBLIC KEY-----';
  await rsa.getByLabel('Server RSA public key').fill(`${key}\n\u0000`);
  await page.getByPlaceholder('e.g. Local development').fill('Local plain');
  await page.getByRole('button', { name: 'Save & connect', exact: true }).click();
  await expect(page.getByText('Live read-only connection opened without TLS (this computer).')).toBeVisible();
  const connect = await page.evaluate(() => (window as any).calls.find((c: any) => c.command === 'connect_direct').payload);
  expect(connect).toMatchObject({ host: '127.0.0.1', tls: false, rootCertificatePem: null, serverPublicKeyPem: key });
  expect(connect.allowPublicKeyRetrieval).toBeUndefined();
});
