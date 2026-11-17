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
