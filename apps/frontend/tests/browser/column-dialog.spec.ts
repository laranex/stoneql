import { test, expect } from '@playwright/test';
for (const theme of ['light', 'dark'])
  test(`${theme}: active Add/Edit Column dialog saves formatted types and physical position`, async ({
    page,
  }) => {
    await page.addInitScript((value) => localStorage.setItem('stoneql-theme', value), theme);
    await page.goto('/');
    await page.getByRole('tab', { name: 'Structure', exact: true }).click();
    await page.getByRole('button', { name: 'Add column', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Add column', exact: true });
    await expect(dialog.getByLabel('Data type', { exact: true })).toHaveValue('VARCHAR');
    await dialog.getByLabel('Column name', { exact: true }).fill('nickname');
    await dialog.getByLabel('Length', { exact: true }).fill('100');
    await dialog.getByLabel('Column position', { exact: true }).selectOption('after');
    await dialog.getByLabel('After column', { exact: true }).selectOption('name');
    await page.screenshot({ path: `test-results/add-column-${theme}.png`, fullPage: true });
    await dialog.getByRole('button', { name: 'Save column', exact: true }).click();
    const row = page.locator('.structure-grid tbody tr').filter({ hasText: 'nickname' });
    await expect(row).toContainText('varchar(100)');
    await row.getByRole('button', { name: 'Change column nickname', exact: true }).click();
    const edit = page.getByRole('dialog', { name: 'Edit column', exact: true });
    await expect(edit.getByLabel('Length', { exact: true })).toHaveValue('100');
    await edit.getByLabel('Length', { exact: true }).fill('120');
    await page.screenshot({ path: `test-results/edit-column-${theme}.png`, fullPage: true });
    await edit.getByRole('button', { name: 'Save column', exact: true }).click();
    await expect(row).toContainText('varchar(120)');
    await page.getByRole('button', { name: 'Add column', exact: true }).click();
    await dialog.getByLabel('Column name', { exact: true }).fill('amount');
    await dialog.getByLabel('Data type', { exact: true }).selectOption('DECIMAL');
    await dialog.getByLabel('Precision', { exact: true }).fill('12');
    await dialog.getByLabel('Scale', { exact: true }).fill('2');
    await dialog.getByRole('button', { name: 'Save column', exact: true }).click();
    await expect(page.locator('.structure-grid')).toContainText('decimal(12,2)');
  });
