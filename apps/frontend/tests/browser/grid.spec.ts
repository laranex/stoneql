import { test, expect } from '@playwright/test';
for (const theme of ['light', 'dark'])
  test(`${theme}: aligned borders, hover grip, resize, auto-fit, reset and persisted widths`, async ({
    page,
  }) => {
    await page.addInitScript((value) => localStorage.setItem('dbadmin-theme', value), theme);
    await page.goto('/');
    const handle = page.getByRole('separator', { name: 'Resize role column', exact: true });
    const header = page
      .locator('.browse-grid th')
      .filter({ has: page.getByRole('button', { name: 'role', exact: true }) });
    const body = page.locator('.browse-grid tbody tr').first().locator('td').nth(5);
    await expect(header).toHaveCSS('border-left-width', '1px');
    await expect(body).toHaveCSS('border-left-width', '1px');
    expect(Math.abs((await header.boundingBox())!.x - (await body.boundingBox())!.x)).toBeLessThan(
      1,
    );
    expect(await handle.evaluate((el) => getComputedStyle(el, '::after').opacity)).toBe('0');
    await page.screenshot({ path: `test-results/grid-${theme}-normal.png`, fullPage: true });
    await handle.hover();
    expect(await handle.evaluate((el) => getComputedStyle(el, '::after').opacity)).toBe('1');
    await page.screenshot({ path: `test-results/grid-${theme}-hover.png`, fullPage: true });
    const before = (await header.boundingBox())!.width,
      box = (await handle.boundingBox())!;
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + 60, box.y + box.height / 2, { steps: 8 });
    await page.mouse.up();
    expect((await header.boundingBox())!.width).toBeGreaterThan(before + 50);
    await handle.dblclick();
    const fitted = (await header.boundingBox())!.width;
    expect(fitted).toBeGreaterThanOrEqual(64);
    expect(fitted).toBeLessThanOrEqual(600);
    await page.reload();
    await expect(
      page.getByRole('separator', { name: 'Resize role column', exact: true }),
    ).toBeVisible();
    expect(Math.abs((await header.boundingBox())!.width - fitted)).toBeLessThan(2);
    await header.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Reset Column Widths' }).click();
    expect((await header.boundingBox())!.width).toBeGreaterThan(100);
    await page.setViewportSize({ width: 720, height: 700 });
    await page
      .locator('.data-grid-wrap')
      .first()
      .evaluate((el) => {
        el.scrollLeft = 300;
      });
    expect(
      await page
        .locator('.data-grid-wrap')
        .first()
        .evaluate((el) => el.scrollLeft),
    ).toBeGreaterThan(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
    await page.getByRole('tab', { name: 'SQL', exact: true }).click();
    await page.getByRole('button', { name: 'Run query', exact: true }).click();
    await expect(
      page.getByRole('separator', { name: 'Resize role column', exact: true }),
    ).toBeVisible();
  });
