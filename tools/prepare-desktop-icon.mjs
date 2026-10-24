// Rasterize the original repository SVG; no external graphics or network assets.
import { readFile, mkdir } from 'node:fs/promises';
import { chromium } from '@playwright/test';
const target = new URL('../apps/desktop/src-tauri/icons/', import.meta.url);
await mkdir(target, {recursive: true});
const svg = await readFile(new URL('../apps/frontend/public/icon.svg', import.meta.url), 'utf8');
const browser = await chromium.launch({chromiumSandbox: true});
try {
  const page = await browser.newPage({viewport: {width: 256, height: 256}});
  await page.setContent(`<style>html,body{margin:0;width:256px;height:256px}svg{width:100%;height:100%}</style>${svg}`);
  await page.screenshot({path: new URL('icon.png', target).pathname, omitBackground: true});
} finally {await browser.close();}
