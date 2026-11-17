import { chromium } from 'playwright';
const base = 'https://stoneace.github.io/stoneql/';
const pages = ['architecture/overview', 'architecture/connection-management', 'architecture/roadmap', 'architecture/import-export', 'development/documentation'];
const browser = await chromium.launch();
let failed = 0;
for (const scheme of ['light', 'dark']) {
  const ctx = await browser.newContext({ colorScheme: scheme });
  for (const p of pages) {
    const page = await ctx.newPage(); const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    const res = await page.goto(base + p + '.html', { waitUntil: 'networkidle' });
    const blocks = await page.locator('figure.diagram').count();
    await page.waitForFunction((n) => document.querySelectorAll('figure.diagram > div > svg').length + document.querySelectorAll('figure.diagram p[role=status]').length >= n, blocks, { timeout: 20000 }).catch(() => {});
    const svgs = await page.locator('figure.diagram > div > svg').count();
    const rawLeft = await page.locator('figure.diagram p[role=status]').count();
    const ok = res.status() === 200 && blocks > 0 && svgs === blocks && rawLeft === 0 && errors.length === 0;
    if (!ok) failed++;
    console.log(`${scheme} ${p}: http ${res.status()} containers ${blocks} svg ${svgs} raw ${rawLeft} errors ${errors.length} ${ok ? 'OK' : 'FAIL'}`);
    await page.close();
  }
  await ctx.close();
}
await browser.close();
process.exit(failed ? 1 : 0);
