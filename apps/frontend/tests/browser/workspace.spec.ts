import { test, expect } from '@playwright/test';

for (const theme of ['light', 'dark']) {
  test(`${theme}: compact workspace, independent notifications, preview preference, and footer`, async ({
    page,
  }) => {
    await page.addInitScript((value) => localStorage.setItem('dbadmin-theme', value), theme);
    await page.goto('/');
    const footer = page.locator('.bottom-status');
    const footerHeight = (await footer.boundingBox())!.height;
    await expect(page.locator('.table-heading')).toHaveCount(0);
    await expect(page.locator('.breadcrumb + .view-tabs')).toHaveCount(1);
    await expect(page.getByRole('group', { name: 'Table metadata' })).toContainText('Rows: 128');
    await expect(page.getByRole('group', { name: 'Table metadata' })).toContainText('InnoDB');
    await page.getByRole('button', { name: 'Collapse SQL query preview', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect(page.locator('.sql-query-preview')).toHaveClass(/collapsed/);
    await expect(page.locator('.sql-query-preview .preview-edit')).toHaveCount(0);
    const previewTop = (await page.locator('.sql-query-preview').boundingBox())!.y;
    await page.getByRole('button', { name: 'Dismiss query notification', exact: true }).click();
    expect((await page.locator('.sql-query-preview').boundingBox())!.y).toBeLessThan(previewTop);
    await expect(page.locator('.browse-grid tbody tr')).toHaveCount(25);
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await expect(
      page.getByRole('button', { name: 'Dismiss query notification', exact: true }),
    ).toBeVisible();
    await expect(page.locator('.sql-query-preview')).toHaveClass(/collapsed/);
    await page.locator('.table-tree button').filter({ hasText: 'orders' }).click();
    await expect(page.locator('.sql-query-preview')).toHaveClass(/collapsed/);
    await page.getByRole('tab', { name: 'SQL', exact: true }).click();
    await page.getByRole('button', { name: 'Run query', exact: true }).click();
    await expect(page.locator('.result-notice')).toContainText('rows returned');
    const count = await page.locator('.content-area .data-grid tbody tr').count();
    await page.getByRole('button', { name: 'Dismiss query notification', exact: true }).click();
    await expect(page.locator('.content-area .data-grid tbody tr')).toHaveCount(count);
    await page.getByRole('button', { name: 'Query history', exact: false }).click();
    await expect(page.locator('.history-panel')).toContainText('Success');
    await page.locator('.cm-content').fill('SELECT invalid FROM orders;');
    await page.getByRole('button', { name: 'Run query', exact: true }).click();
    await expect(page.locator('.result-notice.error')).toBeVisible();
    await page.getByRole('button', { name: 'Dismiss query notification', exact: true }).focus();
    await page.keyboard.press('Space');
    await expect(page.locator('.result-notice')).toHaveCount(0);
    await page.getByRole('tab', { name: 'Browse', exact: true }).click();
    await expect(page.locator('.sql-query-preview')).toHaveClass(/collapsed/);
    await page.getByRole('button', { name: 'Expand SQL query preview', exact: true }).click();
    await expect(
      page.getByRole('button', { name: 'Collapse SQL query preview', exact: true }),
    ).toHaveAttribute('aria-expanded', 'true');
    await expect(page.locator('.sql-query-preview .preview-edit')).toBeVisible();
    await page.screenshot({ path: `test-results/workspace-${theme}-expanded.png`, fullPage: true });
    await page.locator('.connection-tab > button').filter({ hasText: 'Staging server' }).click();
    await expect(page.getByRole('group', { name: 'Table metadata' })).not.toContainText('InnoDB');
    await expect(page.getByRole('group', { name: 'Table metadata' })).not.toContainText('utf8mb4');
    for (const width of [1024, 720, 400]) {
      await page.setViewportSize({ width, height: 700 });
      await page
        .locator('.breadcrumb-item')
        .nth(1)
        .evaluate((el) => {
          el.textContent = 'a_very_long_database_name_'.repeat(6);
        });
      await page
        .locator('.breadcrumb-item')
        .nth(2)
        .evaluate((el) => {
          el.textContent = 'a_very_long_table_name_'.repeat(6);
        });
      expect(
        await page.locator('.breadcrumb').evaluate((el) => el.scrollWidth <= el.clientWidth),
      ).toBe(true);
      await page.locator('.content-area').evaluate((el) => {
        el.scrollTop = el.scrollHeight;
      });
      await expect(footer).toBeInViewport();
      expect((await footer.boundingBox())!.height).toBe(footerHeight);
    }
    await page.getByRole('button', { name: 'Collapse SQL query preview', exact: true }).click();
    await page.screenshot({
      path: `test-results/workspace-${theme}-collapsed.png`,
      fullPage: true,
    });
  });
}
