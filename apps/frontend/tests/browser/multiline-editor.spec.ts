// Multi-line inline editing, accessible markers and raw copies.
// "Display what the database stores, not what the application thinks it means."
import { test, expect, type Page, type Locator } from '@playwright/test';

/** Session-only mock values set straight in the store: CR cannot be typed or pasted into a browser field. */
async function seed(page: Page, values: (string | null)[], nullable = false) {
  await page.evaluate(
    ([values, nullable]) => {
      const app = (document.querySelector('#app') as any).__vue_app__;
      const store = app.config.globalProperties.$pinia._s.get('workspace');
      store.table.columns.find((c: any) => c.name === 'name').nullable = nullable;
      values.forEach((v: string | null, i: number) => (store.table.rows[i].name = v));
    },
    [values, nullable] as const,
  );
}
async function stored(page: Page, i: number) {
  return page.evaluate((i) => {
    const app = (document.querySelector('#app') as any).__vue_app__;
    return app.config.globalProperties.$pinia._s.get('workspace').table.rows[i].name;
  }, i);
}
async function nameCells(page: Page) {
  const grid = page.locator('.browse-grid');
  const index = await grid
    .locator('thead th')
    .evaluateAll((ths) => ths.findIndex((th) => th.textContent?.trim().startsWith('name')));
  expect(index).toBeGreaterThanOrEqual(0);
  return grid.locator(`tbody tr td:nth-child(${index + 1})`);
}
const editor = (page: Page) => page.locator('#inline-cell-editor');
const field = (page: Page) => editor(page).getByLabel('Edit name', { exact: true });
async function rowHeight(cell: Locator) {
  return cell.evaluate((td) => td.parentElement!.getBoundingClientRect().height);
}
async function editorBox(page: Page) {
  return editor(page).evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, vw: innerWidth, vh: innerHeight };
  });
}
function expectInViewport(box: Awaited<ReturnType<typeof editorBox>>) {
  expect(box.left).toBeGreaterThanOrEqual(0);
  expect(box.top).toBeGreaterThanOrEqual(0);
  expect(box.right).toBeLessThanOrEqual(box.vw);
  expect(box.bottom).toBeLessThanOrEqual(box.vh);
}
/** The editor stays in the viewport, directly below its cell or, without room there, directly above. */
async function expectAnchored(page: Page, cell: Locator) {
  const box = await editorBox(page);
  const anchor = await cell.evaluate((td) => td.getBoundingClientRect().toJSON());
  expectInViewport(box);
  if (box.vh - 8 - anchor.bottom >= box.bottom - box.top) {
    expect(Math.abs(box.top - anchor.bottom)).toBeLessThanOrEqual(1);
    return 'below';
  }
  expect(Math.abs(box.bottom - anchor.top)).toBeLessThanOrEqual(1);
  return 'above';
}
async function start(page: Page, theme: string) {
  await page.addInitScript((value) => localStorage.setItem('stoneql-theme', value), theme);
  await page.goto('/');
  if (theme === 'dark') await expect(page.locator('html')).toHaveClass(/\bdark\b/);
  else await expect(page.locator('html')).not.toHaveClass(/\bdark\b/);
  await expect(page.locator('.browse-grid tbody tr').first()).toBeVisible();
}

const VALUES = ['lf\nvalue', 'cr\rvalue', 'crlf\r\nvalue', 'mix\r\ned\nends\rhere', '', 'ordinary', null];
for (const theme of ['light', 'dark']) {
  test(`${theme}: editor type follows the raw value, floats without growing rows, and untouched saves keep every character`, async ({ page }) => {
    await start(page, theme);
    await seed(page, VALUES, true);
    const cells = await nameCells(page);
    for (const [i, value] of VALUES.entries()) {
      const cell = cells.nth(i), before = await rowHeight(cell);
      await cell.dblclick();
      const multiline = typeof value === 'string' && /[\r\n]/.test(value);
      // SQL NULL disables the value field, so focus starts on the NULL toggle instead.
      await expect(value === null ? editor(page).getByLabel('NULL', { exact: true }) : field(page)).toBeFocused();
      expect(await field(page).evaluate((el) => el.tagName)).toBe(multiline ? 'TEXTAREA' : 'INPUT');
      if (multiline) {
        // The textarea shows LF text; the line-ending choice shows what will be written.
        expect(await field(page).inputValue()).toBe(value!.replace(/\r\n?/g, '\n'));
        await expect(editor(page).getByLabel('Line endings')).toHaveValue(
          ['lf', 'cr', 'crlf', 'keep'][i]!,
        );
      } else expect(await field(page).inputValue()).toBe(value ?? '');
      expect(await rowHeight(cell)).toBe(before);
      expect(await expectAnchored(page, cell)).toBe('below');
      await page.screenshot({ path: `test-results/multiline-editor-${theme}-${i}.png` });
      // Untouched Escape closes without a discard prompt and returns focus to the cell.
      await page.keyboard.press('Escape');
      await expect(editor(page)).toHaveCount(0);
      await expect(page.getByRole('alertdialog')).toHaveCount(0);
      await expect(cell).toBeFocused();
      // Untouched save writes back exactly the stored value.
      await page.keyboard.press('Enter');
      await expect(editor(page)).toHaveCount(1);
      await editor(page).getByRole('button', { name: 'Save cell edit' }).click();
      await expect(editor(page)).toHaveCount(0);
      expect(await stored(page, i)).toBe(value);
      await expect(cell).toHaveAttribute('title', value === null ? 'SQL NULL' : value);
    }
  });

  test(`${theme}: edited multi-line values keep their line-ending style; keyboard saves and cancels`, async ({ page }) => {
    await start(page, theme);
    await seed(page, VALUES);
    const cells = await nameCells(page);

    // LF: Enter inserts a newline, Ctrl+Enter saves, focus returns to the cell.
    await cells.nth(0).dblclick();
    await field(page).fill('lf\nvalue!');
    await field(page).press('Enter');
    await expect(editor(page)).toHaveCount(1);
    await field(page).pressSequentially('x');
    await field(page).press('Control+Enter');
    await expect(editor(page)).toHaveCount(0);
    expect(await stored(page, 0)).toBe('lf\nvalue!\nx');
    await expect(cells.nth(0)).toBeFocused();

    // CR stays CR; Cmd+Enter (Meta) saves as well.
    await cells.nth(1).dblclick();
    await field(page).fill('cr\nvalue!');
    await field(page).press('Enter');
    await field(page).pressSequentially('y');
    await field(page).press('Meta+Enter');
    await expect(editor(page)).toHaveCount(0);
    expect(await stored(page, 1)).toBe('cr\rvalue!\ry');

    // CRLF stays CRLF, including newly typed lines.
    await cells.nth(2).dblclick();
    await field(page).fill('crlf\nvalue'); // same text; leaves the caret at the end
    await field(page).press('Enter');
    await field(page).pressSequentially('z');
    await field(page).press('Control+Enter');
    expect(await stored(page, 2)).toBe('crlf\r\nvalue\r\nz');

    // Mixed: same number of breaks keeps each original ending in place.
    await cells.nth(3).dblclick();
    await field(page).fill('MIX\nED\nENDS\nHERE');
    await field(page).press('Control+Enter');
    expect(await stored(page, 3)).toBe('MIX\r\nED\nENDS\rHERE');
    // A changed number of breaks needs an explicit, visible choice before saving.
    await cells.nth(3).dblclick();
    await field(page).fill('MIX\nED\nENDS\nHERE\nMORE');
    await field(page).press('Control+Enter');
    await expect(editor(page).getByRole('alert')).toContainText('choose LF, CRLF or CR');
    expect(await stored(page, 3)).toBe('MIX\r\nED\nENDS\rHERE');
    await editor(page).getByLabel('Line endings').selectOption('crlf');
    await editor(page).getByRole('button', { name: 'Save cell edit' }).click();
    await expect(editor(page)).toHaveCount(0);
    expect(await stored(page, 3)).toBe('MIX\r\nED\r\nENDS\r\nHERE\r\nMORE');

    // Escape on an edited value asks before discarding; the stored value is untouched.
    await cells.nth(2).dblclick();
    await field(page).fill('changed');
    await page.keyboard.press('Escape');
    await page.getByRole('alertdialog').getByRole('button', { name: 'Discard changes' }).click();
    await expect(editor(page)).toHaveCount(0);
    expect(await stored(page, 2)).toBe('crlf\r\nvalue\r\nz');

    // Single-line values keep Enter-to-save.
    await cells.nth(5).dblclick();
    await field(page).fill('ordinary!');
    await field(page).press('Enter');
    await expect(editor(page)).toHaveCount(0);
    expect(await stored(page, 5)).toBe('ordinary!');

    // The record form uses the same editor for a lone CR and keeps it.
    await seed(page, ['', 'cr\rvalue']);
    const id = await page.evaluate(() => {
      const app = (document.querySelector('#app') as any).__vue_app__;
      return app.config.globalProperties.$pinia._s.get('workspace').table.rows[1].id;
    });
    await page.getByTitle(`Edit record ${id}`, { exact: true }).click();
    const record = page.getByLabel('Value for name', { exact: true });
    expect(await record.evaluate((el) => el.tagName)).toBe('TEXTAREA');
    await expect(page.getByLabel('Line endings', { exact: true })).toHaveValue('cr');
    await record.fill('cr\nvalue 2');
    await page.getByRole('button', { name: 'Save changes' }).click();
    expect(await stored(page, 1)).toBe('cr\rvalue 2');
  });

  test(`${theme}: long values and narrow viewports keep the floating editor on screen`, async ({ page }) => {
    await page.setViewportSize({ width: 480, height: 620 });
    await start(page, theme);
    const long = Array.from({ length: 40 }, (_, i) => `line ${i} ${'x'.repeat(120)}`).join('\r\n');
    await seed(page, Array.from({ length: 30 }, () => long));
    const cells = await nameCells(page);
    // Rows fully visible inside the grid's own scroll area, without scrolling anything.
    const visible = await cells.evaluateAll((tds) => {
      const area = tds[0]!.closest('.data-grid-wrap')!.getBoundingClientRect();
      return tds
        .map((td, i) => ({ i, rect: td.getBoundingClientRect() }))
        .filter(({ rect }) => rect.top >= area.top && rect.bottom <= Math.min(area.bottom, innerHeight))
        .map(({ i }) => i);
    });
    expect(visible.length).toBeGreaterThan(1);
    for (const i of [visible[0]!, visible.at(-1)!]) {
      const cell = cells.nth(i), before = await rowHeight(cell);
      await cell.dblclick();
      await expect(field(page)).toBeVisible();
      const box = await editorBox(page);
      expect(await expectAnchored(page, cell)).toBe('above');
      expect(box.right - box.left).toBeLessThanOrEqual(box.vw - 24 + 1);
      // The textarea scrolls its own content instead of stretching the editor or the row.
      expect(await field(page).evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
      expect(await rowHeight(cell)).toBe(before);
      await page.keyboard.press('Escape');
      await expect(editor(page)).toHaveCount(0);
      expect(await stored(page, i)).toBe(long);
    }
  });

  test(`${theme}: assistive names separate SQL NULL from 'NULL' and name each marker, while copies, filters and truncation use raw values`, async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await start(page, theme);
    const marked = 'one\r\ntwo\tend\nlf\rcr';
    const long = `${'wide text '.repeat(30)}\n${'more '.repeat(30)}`;
    await seed(page, [marked, null, 'NULL', long], true);
    const cells = await nameCells(page);

    await expect(cells.nth(1).getByRole('img', { name: 'SQL NULL', exact: true })).toBeVisible();
    await expect(cells.nth(2).getByRole('img')).toHaveCount(0);
    await expect(cells.nth(2)).toHaveText('NULL');
    const names = await cells.nth(0).getByRole('img').evaluateAll((els) => els.map((e) => e.getAttribute('aria-label')));
    expect(names).toEqual(['CRLF', 'tab', 'line feed', 'carriage return']);
    // No hidden description text: what is visible is all the cell's text.
    expect(await cells.nth(0).evaluate((td) => [td.textContent, (td as HTMLElement).innerText])).toEqual([
      'one␍↵two⇥end↵lf␍cr',
      'one␍↵two⇥end↵lf␍cr',
    ]);
    await expect(cells.nth(0)).toHaveAttribute('title', marked);

    // Copying selected grid text yields the stored characters only.
    const copy = async (select: string) => {
      await page.evaluate(() => navigator.clipboard.writeText('sentinel'));
      await cells.nth(0).evaluate((td, how) => {
        const range = document.createRange();
        if (how === 'cell') range.selectNodeContents(td);
        else {
          const walker = document.createTreeWalker(td, NodeFilter.SHOW_TEXT);
          while (!walker.nextNode()!.nodeValue);
          range.setStart(walker.currentNode, 1);
          range.setEnd(td.querySelector('span')!, td.querySelector('span')!.childNodes.length);
        }
        getSelection()!.removeAllRanges();
        getSelection()!.addRange(range);
      }, select);
      await page.keyboard.press('ControlOrMeta+c');
      return page.evaluate(() => navigator.clipboard.readText());
    };
    expect(await copy('cell')).toBe(marked);
    expect(await copy('partial')).toBe(marked.slice(1));
    // Copying ordinary text keeps the browser's own behaviour.
    await cells.nth(2).evaluate((td) => {
      const range = document.createRange();
      range.selectNodeContents(td);
      getSelection()!.removeAllRanges();
      getSelection()!.addRange(range);
    });
    await page.keyboard.press('ControlOrMeta+c');
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('NULL');

    // A long marked value still truncates with an ellipsis inside its column.
    const clip = await cells.nth(3).evaluate((td) => ({
      overflow: td.scrollWidth > td.clientWidth,
      textOverflow: getComputedStyle(td).textOverflow,
      whiteSpace: getComputedStyle(td).whiteSpace,
      markers: td.querySelectorAll('.invisible-marker').length,
    }));
    expect(clip).toEqual({ overflow: true, textOverflow: 'ellipsis', whiteSpace: 'nowrap', markers: 1 });

    // Filtering matches the raw characters, not marker glyphs or spoken names.
    await page.getByLabel('Filter rows').fill('two\tend');
    await expect(page.locator('.browse-grid tbody tr')).toHaveCount(1);
    await expect((await nameCells(page)).first()).toHaveAttribute('title', marked);
    for (const text of ['two⇥end', 'line feed']) {
      await page.getByLabel('Filter rows').fill(text);
      await expect(page.locator('.browse-grid tbody td.empty-state')).toHaveCount(1);
    }
  });
}
