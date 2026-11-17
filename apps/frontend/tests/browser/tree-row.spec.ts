import { test, expect } from '@playwright/test';
// The explorer's database row is one expand/collapse control: arrow, icon, name and count sit on
// one line, any of them toggles the same table list, and it is one focus target (Enter/Space).
for (const theme of ['light', 'dark']) test(`${theme}: a database row is one toggle for its table list`, async ({ page }) => {
  await page.addInitScript((theme) => localStorage.setItem('stoneql-theme', theme), theme);
  await page.goto('/');
  const group = page.locator('.database-group').filter({ hasText: 'analytics' });
  const row = group.getByRole('button', { name: /^analytics, \d+ tables$/ });
  const tables = group.locator('.table-tree');
  await expect(group.getByRole('button')).toHaveCount(1);
  // One line: arrow, icon and name share a vertical centre.
  const boxes = await row.locator('svg, strong').evaluateAll((els) => els.map((el) => { const r = el.getBoundingClientRect(); return r.top + r.height / 2; }));
  expect(Math.max(...boxes) - Math.min(...boxes)).toBeLessThan(4);

  await expect(row).toHaveAttribute('aria-expanded', 'false');
  await expect(tables).toHaveCount(0);
  await row.locator('strong').click();
  await expect(row).toHaveAttribute('aria-expanded', 'true');
  await expect(tables.getByRole('button').first()).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Database breadcrumb' })).toContainText('analytics');
  await row.locator('strong').click();
  await expect(row).toHaveAttribute('aria-expanded', 'false');
  await expect(tables).toHaveCount(0);
  // The arrow and the icon toggle the same list.
  await row.locator('svg').first().click();
  await expect(row).toHaveAttribute('aria-expanded', 'true');
  await row.locator('svg').nth(1).click();
  await expect(row).toHaveAttribute('aria-expanded', 'false');
  // Keyboard: one focus target, Enter and Space toggle.
  await row.focus();
  await page.keyboard.press('Enter');
  await expect(row).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('Space');
  await expect(row).toHaveAttribute('aria-expanded', 'false');
  await page.screenshot({ path: `test-results/tree-row-${theme}.png` });
});
