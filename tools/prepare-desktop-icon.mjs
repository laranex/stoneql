// Derive the desktop and web icons from the 1024px master (user-supplied artwork, already cropped to Apple's icon grid).
import { readFile, writeFile } from 'node:fs/promises';
import { chromium } from '@playwright/test';
const master = new URL('../apps/desktop/src-tauri/icons/icon-source.png', import.meta.url);
const outputs = [
  [new URL('../apps/desktop/src-tauri/icons/icon.png', import.meta.url), 512],
  [new URL('../apps/frontend/public/icon.png', import.meta.url), 256],
];
const data = (await readFile(master)).toString('base64');
const browser = await chromium.launch({ chromiumSandbox: true });
try {
  const page = await browser.newPage();
  for (const [target, size] of outputs) {
    const png = await page.evaluate(async ([b64, size]) => {
      const img = new Image(); img.src = `data:image/png;base64,${b64}`; await img.decode();
      const canvas = new OffscreenCanvas(size, size), context = canvas.getContext('2d');
      context.imageSmoothingQuality = 'high';
      context.drawImage(img, 0, 0, size, size);
      const bytes = new Uint8Array(await (await canvas.convertToBlob({ type: 'image/png' })).arrayBuffer());
      let text = ''; for (const byte of bytes) text += String.fromCharCode(byte);
      return btoa(text);
    }, [data, size]);
    await writeFile(target, Buffer.from(png, 'base64'));
  }
} finally { await browser.close(); }
