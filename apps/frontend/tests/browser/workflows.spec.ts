import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('stoneql-theme', 'light'));
  await page.goto('/');
});
async function openExport(page: import('@playwright/test').Page) {
  await page.getByRole('tab', { name: 'Export', exact: true }).click();
}

test('Quick export succeeds without downloading and preferences survive navigation', async ({
  page,
}) => {
  let downloads = 0;
  page.on('download', () => downloads++);
  await openExport(page);
  await expect(page.getByLabel('Export format')).toHaveValue('SQL');
  await page.getByRole('button', { name: 'Simulate export', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Export simulation completed' })).toBeVisible();
  await page.getByLabel('Custom', { exact: false }).check();
  await page.getByLabel('ZIP', { exact: true }).check();
  await page.getByLabel('Export filename').fill('test_@TABLE@');
  await page.getByRole('button', { name: 'Simulate export', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Export simulation completed' })).toBeVisible();
  await expect(page.locator('.export-summary')).toContainText('test_users.sql.zip');
  await expect(page.locator('.simulation-reminder')).toContainText('No backup was created');
  expect(downloads).toBe(0);
  await page.getByRole('tab', { name: 'Browse', exact: true }).click();
  await openExport(page);
  await expect(page.getByLabel('Export filename')).toHaveValue('test_@TABLE@');
  await expect(page.getByLabel('ZIP', { exact: true })).toBeChecked();
});

test('Custom database selection, structures, CSV, JSON, and validation', async ({ page }) => {
  await openExport(page);
  await page.getByLabel('Custom', { exact: false }).check();
  await page.getByLabel('Database', { exact: true }).check();
  await expect(page.locator('.export-count')).toHaveText('7 of 7 selected');
  await page.getByRole('button', { name: 'Deselect all', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Simulate export', exact: true })).toBeDisabled();
  await page.locator('.export-table-list').getByLabel('users', { exact: false }).check();
  await page.getByLabel('Export content').selectOption('structure');
  await page.getByRole('button', { name: 'Simulate export', exact: true }).click();
  await expect(page.locator('.export-summary')).toContainText('1 / 1 / 0');
  await page.getByLabel('Export format').selectOption('CSV');
  await expect(page.getByLabel('Export content')).toHaveValue('structure');
  await expect(page.getByRole('alert')).toContainText('CSV supports Data only');
  await page.getByLabel('Export content').selectOption('data');
  await page.getByLabel('CSV delimiter').fill(';');
  await page.getByLabel('GZIP', { exact: true }).check();
  await page.getByLabel('Export content').selectOption('both');
  await expect(page.getByRole('alert')).toContainText('CSV supports Data only');
  await page.getByLabel('Export content').selectOption('data');
  await page.getByLabel('Export format').selectOption('JSON');
  await page.getByLabel('Include database and engine metadata').check();
  await expect(page.getByRole('button', { name: 'Simulate export', exact: true })).toBeEnabled();
});

test('export cancellation, failure, and retry', async ({ page }) => {
  await openExport(page);
  await page.getByRole('button', { name: 'Simulate export', exact: true }).click();
  await page.getByRole('button', { name: 'Cancel simulation', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Export simulation cancelled' })).toBeVisible();
  await page.getByLabel('Custom', { exact: false }).check();
  await page.getByText('Prototype testing controls', { exact: true }).click();
  await page.getByLabel('Simulate an export failure', { exact: false }).check();
  await page.getByRole('button', { name: 'Simulate export', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Export simulation failed' })).toBeVisible();
  await page.getByLabel('Simulate an export failure', { exact: false }).uncheck();
  await page.getByRole('button', { name: 'Retry simulation', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Export simulation completed' })).toBeVisible();
});

test('theme toggle preserves export controls and PostgreSQL capabilities', async ({ page }) => {
  await page.getByRole('button', { name: 'Switch to dark theme', exact: true }).click();
  await expect(page.locator('html')).toHaveClass('dark');
  await page.locator('.connection-tab > button').filter({ hasText: 'Staging server' }).click();
  await openExport(page);
  await page.getByLabel('Custom', { exact: false }).check();
  await expect(
    page.getByText('Global foreign key disabling is unavailable', { exact: false }),
  ).toBeVisible();
  await expect(page.getByLabel('Events', { exact: false })).toBeDisabled();
  await expect(page.getByLabel('INSERT statement style').locator('option')).toHaveCount(2);
  await page.screenshot({ path: 'test-results/export-dark.png', fullPage: true });
  await page.getByRole('button', { name: 'Switch to light theme', exact: true }).click();
  await expect(page.locator('html')).not.toHaveClass('dark');
  await page.screenshot({ path: 'test-results/export-light.png', fullPage: true });
});

test('CRUD stays consistent and connection workspaces are independent', async ({ page }) => {
  await page.getByRole('tab', { name: 'Insert', exact: true }).click();
  await page.getByLabel('Value for name', { exact: true }).fill('Test Person');
  await page.getByLabel('Value for email', { exact: true }).fill('test@example.com');
  await page.getByRole('button', { name: 'Insert record', exact: true }).click();
  await page.getByLabel('Filter rows', { exact: true }).fill('Test Person');
  await expect(page.locator('.browse-grid tbody tr')).toHaveCount(1);
  await page.locator('.connection-tab > button').filter({ hasText: 'Staging server' }).click();
  await page.getByLabel('Filter rows', { exact: true }).fill('Test Person');
  await expect(page.getByText('No records found', { exact: true })).toBeVisible();
  await page.locator('.connection-tab > button').filter({ hasText: 'Local development' }).click();
  await expect(page.locator('.browse-grid tbody')).toContainText('Test Person');
  await page.locator('.browse-grid').getByRole('button', { name: 'Delete record 129', exact: true }).click();
  await page.getByRole('button', { name: 'Delete 1 record', exact: true }).click();
  await expect(page.getByText('No records found', { exact: true })).toBeVisible();
});
