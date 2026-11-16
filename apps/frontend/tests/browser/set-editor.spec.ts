import { test, expect, type Locator, type Page } from '@playwright/test';
// MySQL/MariaDB SET values are edited with one labeled checkbox per member, in the record form and
// the inline editor: saved members are prechecked, each toggles independently by click or keyboard,
// the stored value stays comma-separated in member order, NULL/Default disable the group, and ENUM
// keeps its single-select dropdown.
async function columnCells(page: Page, grid: Locator, name: string) {
  const index = await grid
    .locator('thead th')
    .evaluateAll((ths, n) => ths.findIndex((th) => th.textContent?.trim().startsWith(n)), name);
  expect(index).toBeGreaterThanOrEqual(0);
  return grid.locator(`tbody tr td:nth-child(${index + 1})`);
}
const states = (group: Locator) =>
  Promise.all(['read', 'write', 'export'].map((name) => group.getByRole('checkbox', { name, exact: true }).isChecked()));

for (const theme of ['light', 'dark'] as const)
  test(`${theme}: SET members are prechecked, toggled by click or keyboard, saved and reopened`, async ({ page }) => {
    await page.addInitScript((value) => localStorage.setItem('stoneql-theme', value), theme);
    await page.goto('/');
    if (theme === 'dark') await expect(page.locator('html')).toHaveClass(/\bdark\b/);
    const browse = page.locator('.browse-grid');
    const permissions = await columnCells(page, browse, 'permissions');
    await expect(permissions.nth(0)).toHaveText('read,write');

    // Record form: prechecked from the saved value; click one member off, keyboard another on.
    await page.getByTitle('Edit record 1', { exact: true }).click();
    const form = page.getByRole('group', { name: 'Value for permissions' });
    await expect(form.getByRole('checkbox')).toHaveCount(3);
    expect(await states(form)).toEqual([true, true, false]);
    await form.getByRole('checkbox', { name: 'write', exact: true }).click();
    await form.getByRole('checkbox', { name: 'export', exact: true }).focus();
    await page.keyboard.press('Space');
    expect(await states(form)).toEqual([true, false, true]);
    await page.getByRole('button', { name: 'Save changes' }).click();
    await expect(permissions.nth(0)).toHaveText('read,export');
    // Reopened: the saved members are checked again.
    await page.getByTitle('Edit record 1', { exact: true }).click();
    expect(await states(page.getByRole('group', { name: 'Value for permissions' }))).toEqual([true, false, true]);
    // NULL and Default disable the group without changing the members.
    const nullBox = page.getByRole('checkbox', { name: 'NULL for permissions' });
    await nullBox.check();
    await expect(page.getByRole('group', { name: 'Value for permissions' }).getByRole('checkbox').first()).toBeDisabled();
    await nullBox.uncheck();
    await expect(page.getByRole('group', { name: 'Value for permissions' }).getByRole('checkbox').first()).toBeEnabled();
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();

    // Inline editor: focus starts on the first member; keyboard toggles; Enter saves.
    await permissions.nth(1).dblclick();
    const inline = page.locator('#inline-cell-editor').getByRole('group', { name: 'Edit permissions' });
    expect(await states(inline)).toEqual([true, false, false]);
    await expect(inline.getByRole('checkbox', { name: 'read', exact: true })).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(inline.getByRole('checkbox', { name: 'write', exact: true })).toBeFocused();
    await page.keyboard.press('Space');
    await inline.getByRole('checkbox', { name: 'export', exact: true }).click();
    expect(await states(inline)).toEqual([true, true, true]);
    await page.keyboard.press('Enter');
    await expect(page.locator('#inline-cell-editor')).toHaveCount(0);
    await expect(permissions.nth(1)).toHaveText('read,write,export');
    // Reopened inline: all three checked; unchecking every member saves the empty set.
    await permissions.nth(1).dblclick();
    const again = page.locator('#inline-cell-editor').getByRole('group', { name: 'Edit permissions' });
    expect(await states(again)).toEqual([true, true, true]);
    for (const name of ['read', 'write', 'export']) await again.getByRole('checkbox', { name, exact: true }).click();
    await page.getByRole('button', { name: 'Save cell edit' }).click();
    await expect(permissions.nth(1)).toHaveText('');
    await expect(permissions.nth(1)).not.toHaveText('NULL');

    // ENUM keeps its single-select dropdown.
    const status = await columnCells(page, browse, 'status');
    await status.nth(0).dblclick();
    await expect(page.locator('#inline-cell-editor').getByRole('combobox', { name: 'Edit status' })).toBeVisible();
    await page.keyboard.press('Escape');
  });
