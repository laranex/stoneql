import { test, expect } from '@playwright/test';

for (const theme of ['light', 'dark'] as const) {
  test(`${theme}: readable typography, compact workspace, and persistent footer`, async ({
    page,
  }) => {
    await page.addInitScript((value) => localStorage.setItem('dbadmin-theme', value), theme);
    await page.goto('/');
    await expect(page.locator('.titlebar')).toHaveCount(0);
    await expect(page.locator('.connection-tabs')).toHaveJSProperty('offsetTop', 0);
    await expect(page.locator('body')).toHaveCSS('font-size', '14px');
    await expect(page.locator('.browse-grid tbody td').first()).toHaveCSS('font-size', '14px');
    await expect(page.locator('.browse-grid th').first()).toHaveCSS('font-size', '13px');
    const normalTable = page.locator('.table-tree button:not(.selected)').filter({
      has: page.getByText('orders', { exact: true }),
    });
    const selectedTable = page.locator('.table-tree button.selected').filter({
      has: page.getByText('users', { exact: true }),
    });
    await expect(normalTable).toHaveCount(1);
    await expect(selectedTable).toHaveCount(1);
    await expect(normalTable).toHaveCSS('font-size', '14px');
    await expect(selectedTable).toHaveCSS('font-size', '14px');
    await expect(normalTable).toHaveCSS('font-weight', '500');
    await expect(selectedTable).toHaveCSS('font-weight', '600');
    await expect(page.locator('.table-heading')).toHaveCount(0);
    await expect(page.locator('.main-workspace h1')).toHaveCount(0);
    await expect(page.locator('.breadcrumb + .view-tabs')).toHaveCount(1);
    await expect(page.getByRole('group', { name: 'Table metadata' })).toContainText('Rows: 128');
    await expect(page.getByRole('group', { name: 'Table metadata' })).toContainText('InnoDB');
    await expect(page.locator('.breadcrumb')).toHaveCSS('font-size', '13px');
    await page.getByLabel('Select row 1', { exact: true }).check();
    await expect(page.locator('.row-selected')).toHaveCount(1);
    await page.screenshot({ path: `test-results/appearance-${theme}-browse.png`, fullPage: true });
    for (const view of ['Structure', 'SQL', 'Search', 'Insert', 'Export', 'Import', 'Operations']) {
      await page.getByRole('tab', { name: view, exact: true }).click();
      if (view === 'SQL') await expect(page.locator('.cm-editor')).toHaveCSS('font-size', '14px');
      await page.locator('.content-area').evaluate((el) => {
        el.scrollTop = el.scrollHeight;
      });
      await expect(page.getByRole('button', { name: 'Settings', exact: true })).toBeInViewport();
      await expect(page.locator('.footer-theme-toggle')).toBeInViewport();
      await page.screenshot({
        path: `test-results/appearance-${theme}-${view.toLowerCase()}.png`,
        fullPage: true,
      });
    }
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await expect(
      page.getByRole('dialog', { name: 'Settings', exact: true }).locator('select').first(),
    ).toHaveCSS('font-size', '14px');
    await page.screenshot({
      path: `test-results/appearance-${theme}-settings.png`,
      fullPage: true,
    });
    await page.getByRole('button', { name: 'Done', exact: true }).click();
    await page.locator('.connection-tabs > .subtle').click();
    await page.getByRole('button', { name: 'New connection', exact: true }).click();
    await expect(page.getByPlaceholder('e.g. Local development')).toHaveCSS('font-size', '14px');
    await page.getByRole('button', { name: 'SSH Tunnel', exact: true }).click();
    await page.screenshot({
      path: `test-results/appearance-${theme}-connection.png`,
      fullPage: true,
    });
    await page.getByRole('button', { name: 'Close connection setup', exact: true }).click();
    await page.getByRole('alertdialog').getByRole('button', { name: 'Discard changes', exact: true }).click();
    await page.getByRole('button', { name: 'Close connection manager', exact: true }).click();
    await page.setViewportSize({ width: 720, height: 600 });
    await expect(page.getByRole('button', { name: 'Help', exact: true })).toBeInViewport();
    await expect(page.getByRole('button', { name: 'Settings', exact: true })).toBeInViewport();
    await page.locator('.footer-theme-toggle').click();
    if (theme === 'light') await expect(page.locator('html')).toHaveClass('dark');
    else await expect(page.locator('html')).not.toHaveClass('dark');
  });
}
