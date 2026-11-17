import { test, expect } from '@playwright/test';
// The explorer's expand arrow sits on the same row as the database icon, name and count.
for (const theme of ['light', 'dark']) test(`${theme}: database tree arrow, icon and name share one row`, async ({ page }) => {
  await page.addInitScript((theme) => localStorage.setItem('stoneql-theme', theme), theme);
  await page.goto('/');
  const row = page.locator('.database-row').first();
  const arrow = row.getByRole('button', { name: /^(Expand|Collapse) / });
  const name = row.locator('.database-context-link');
  const [a, n] = [await arrow.boundingBox(), await name.boundingBox()];
  expect(a && n).toBeTruthy();
  expect(Math.abs((a!.y + a!.height / 2) - (n!.y + n!.height / 2))).toBeLessThan(4);
  expect(a!.x + a!.width).toBeLessThanOrEqual(n!.x + 1);
});
