// "Display what the database stores, not what the application thinks it means."
import { test, expect, type Page, type Locator } from '@playwright/test';
const VALUES = ['active', 'inactive', 'admin', 'customer', 'suspended', 'enabled'];
const SEMANTIC = '.enum-value, .enum-values, .status-badge, .role-badge, .success, .warning, .inactive';
async function columnCells(page: Page, grid: Locator, name: string) {
  const index = await grid
    .locator('thead th')
    .evaluateAll((ths, n) => ths.findIndex((th) => th.textContent?.trim().startsWith(n)), name);
  expect(index).toBeGreaterThanOrEqual(0);
  return grid.locator(`tbody tr td:nth-child(${index + 1})`);
}
/** Same text as stored, same markup (a single unclassed span), and identical computed styling. */
async function expectPlain(cells: Locator, values: string[]) {
  const seen = await cells.evaluateAll((tds, count) =>
    tds.slice(0, count).map((td) => {
      const span = td.querySelector(':scope > span');
      const style = span && getComputedStyle(span);
      return {
        text: td.textContent?.trim(),
        markup: td.innerHTML.replace(/>[^<]*</g, '><').replace(/>[^<]*$/, '>'),
        look: style && [style.color, style.backgroundColor, style.borderStyle, style.fontSize].join('|'),
      };
    }), values.length);
  expect(seen.map((s) => s.text)).toEqual(values);
  expect(new Set(seen.map((s) => s.markup)).size).toBe(1);
  expect(new Set(seen.map((s) => s.look)).size).toBe(1);
  expect(seen[0]!.look).toContain('rgba(0, 0, 0, 0)');
}
for (const theme of ['light', 'dark'])
  test(`${theme}: VARCHAR and ENUM values render as identical plain text in Browse and SQL result grids`, async ({ page }) => {
    await page.addInitScript((value) => localStorage.setItem('stoneql-theme', value), theme);
    await page.goto('/');
    const browse = page.locator('.browse-grid');
    // Session-only mock edits give the VARCHAR role column the six arbitrary values.
    const roles = await columnCells(page, browse, 'role');
    for (const [i, value] of VALUES.entries()) {
      await roles.nth(i).dblclick();
      await page.getByLabel('Edit role', { exact: true }).fill(value);
      await page.getByRole('button', { name: 'Save cell edit' }).click();
      await expect(roles.nth(i)).toHaveText(value);
    }
    await expectPlain(roles, VALUES);
    const status = await columnCells(page, browse, 'status');
    const statusValues = await status.evaluateAll((tds) => tds.slice(0, 7).map((td) => td.textContent!.trim()));
    expect(new Set(statusValues)).toEqual(new Set(['active', 'inactive']));
    await expectPlain(status, statusValues);
    await expect(browse.locator(SEMANTIC)).toHaveCount(0);

    await page.getByRole('tab', { name: 'SQL', exact: true }).click();
    await page.getByRole('button', { name: 'Run query', exact: true }).click();
    await expect(page.getByText(/rows returned/)).toBeVisible();
    const result = page.locator('.data-grid').filter({ has: page.getByRole('button', { name: 'status' }) }).last();
    await expectPlain(await columnCells(page, result, 'role'), VALUES);
    await expectPlain(await columnCells(page, result, 'status'), statusValues);
    await expect(result.locator(SEMANTIC)).toHaveCount(0);
  });

// Stored strings with invisible characters; markers are display-only and must never reach the value.
const MULTI = 'line one\nline two\tend';
/** Cell as the user sees it, the raw value behind it, and its row height. */
async function cellView(cell: Locator) {
  return cell.evaluate((td) => ({
    visible: (td as HTMLElement).innerText,
    title: td.getAttribute('title'),
    markers: [...td.querySelectorAll('.invisible-marker')].map((m) => [m.textContent, m.getAttribute('role'), m.getAttribute('aria-label'), getComputedStyle(m).userSelect]),
    rowHeight: td.parentElement!.getBoundingClientRect().height,
    // A row of the same grid without markers.
    plainHeight: [...td.closest('tbody')!.rows]
      .find((tr) => !tr.querySelector('.invisible-marker'))!
      .getBoundingClientRect().height,
  }));
}
async function expectMarked(cell: Locator) {
  const view = await cellView(cell);
  expect(view.title).toBe(MULTI);
  expect(view.visible).toBe('line one↵line two⇥end');
  expect(view.markers).toEqual([['↵', 'img', 'line feed', 'none'], ['⇥', 'img', 'tab', 'none']]);
  expect(view.rowHeight).toBe(view.plainHeight);
}
for (const theme of ['light', 'dark'])
  test(`${theme}: NULL, empty and invisible characters display distinctly while edits, copies and other grids keep raw values`, async ({ page }) => {
    await page.addInitScript((value) => localStorage.setItem('stoneql-theme', value), theme);
    await page.goto('/');
    await page.locator('.connection-tab > button').filter({ hasText: 'Staging server' }).click();
    if (theme === 'dark') await expect(page.locator('html')).toHaveClass(/\bdark\b/);
    else await expect(page.locator('html')).not.toHaveClass(/\bdark\b/);
    const browse = page.locator('.browse-grid');
    const names = await columnCells(page, browse, 'name');
    const permissions = await columnCells(page, browse, 'permissions');
    const saveInline = async (cell: Locator, fill: (input: Locator) => Promise<void>) => {
      await cell.dblclick();
      await fill(page.locator('#inline-cell-editor input:not([type=checkbox])'));
      await page.getByRole('button', { name: 'Save cell edit' }).click();
      await expect(page.locator('#inline-cell-editor')).toHaveCount(0);
    };

    // Literal 'NULL', SQL NULL and '' are three different stored values.
    await saveInline(names.nth(1), (input) => input.fill('NULL'));
    await saveInline(names.nth(2), (input) => input.fill(''));
    await saveInline(permissions.nth(1), () => page.locator('#inline-cell-editor').getByLabel('NULL').check());
    const looks = await Promise.all([names.nth(1), permissions.nth(1), names.nth(2)].map((cell) =>
      cell.evaluate((td) => {
        const child = td.firstElementChild!, style = getComputedStyle(child);
        return { title: td.getAttribute('title'), text: td.textContent, tag: child.tagName, style: style.fontStyle, color: style.color, children: child.childElementCount };
      })));
    const muted = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--muted').trim());
    expect(looks[0]).toMatchObject({ title: 'NULL', text: 'NULL', tag: 'SPAN', style: 'normal' });
    expect(looks[1]).toMatchObject({ title: 'SQL NULL', text: 'NULL', tag: 'I', style: 'italic' });
    expect(looks[1]!.color).not.toBe(looks[0]!.color);
    expect(await permissions.nth(1).locator('i').evaluate((i, m) => {
      const probe = document.createElement('span');
      probe.style.color = m;
      document.body.append(probe);
      const same = getComputedStyle(probe).color === getComputedStyle(i).color;
      probe.remove();
      return same;
    }, muted)).toBe(true);
    expect(looks[2]).toMatchObject({ title: '', text: '', tag: 'SPAN', children: 0 });

    // A multi-line value enters through the record form's textarea; tabs through the inline editor.
    await page.getByTitle('Edit record 1', { exact: true }).click();
    await page.getByLabel('Value for permissions').fill(MULTI);
    await page.getByRole('button', { name: 'Save changes' }).click();
    await expect(permissions.first()).toHaveAttribute('title', MULTI);
    await expectMarked(permissions.first());
    await saveInline(names.nth(3), (input) => input.fill('a\tb'));
    const tabbed = await cellView(names.nth(3));
    expect(tabbed).toMatchObject({ title: 'a\tb', visible: 'a⇥b', rowHeight: tabbed.plainHeight });

    await browse.screenshot({ path: `test-results/invisible-values-${theme}.png` });
    // Saving untouched cells writes back the exact raw value, never the markers.
    await saveInline(permissions.first(), async () => {});
    await saveInline(names.nth(3), async () => {});
    await expectMarked(permissions.first());
    expect(await names.nth(3).getAttribute('title')).toBe('a\tb');
    await page.getByTitle('Edit record 1', { exact: true }).click();
    expect(await page.getByLabel('Value for permissions').inputValue()).toBe(MULTI);
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();

    // Copying a record seeds the form from the raw value and stores it unchanged.
    await page.getByTitle('Copy record 1', { exact: true }).click();
    expect(await page.getByLabel('Value for permissions').inputValue()).toBe(MULTI);
    await page.getByLabel('Value for email').fill('copied@example.com');
    await page.getByRole('button', { name: 'Insert record' }).click();
    // Filtering matches the raw text, not the markers.
    await page.getByLabel('Filter rows').fill('two\tend');
    await expect(browse.locator('tbody tr')).toHaveCount(2);
    for (const i of [0, 1]) await expect(permissions.nth(i)).toHaveAttribute('title', MULTI);
    await page.getByLabel('Filter rows').fill('two⇥');
    await expect(browse.locator('tbody tr td.empty-state')).toHaveCount(1);
    await page.getByTitle('Clear filter').click();

    await page.getByRole('tab', { name: 'Search', exact: true }).click();
    await page.getByRole('button', { name: 'Search records', exact: true }).click();
    const search = page.locator('.data-grid').filter({ has: page.locator('th', { hasText: 'permissions' }) }).last();
    await expectMarked((await columnCells(page, search, 'permissions')).first());
    expect(await (await columnCells(page, search, 'name')).nth(1).getAttribute('title')).toBe('NULL');
    expect(await (await columnCells(page, search, 'permissions')).nth(1).getAttribute('title')).toBe('SQL NULL');

    await page.getByRole('tab', { name: 'SQL', exact: true }).click();
    await page.getByRole('button', { name: 'Run query', exact: true }).click();
    await expect(page.getByText(/rows returned/)).toBeVisible();
    const result = page.locator('.data-grid').filter({ has: page.getByRole('button', { name: 'permissions' }) }).last();
    const resultPermissions = await columnCells(page, result, 'permissions');
    await expectMarked(resultPermissions.first());
    expect(await cellView(resultPermissions.nth(1))).toMatchObject({ title: 'SQL NULL', visible: 'NULL' });
    expect(await cellView((await columnCells(page, result, 'name')).nth(1))).toMatchObject({ title: 'NULL', visible: 'NULL' });
  });
