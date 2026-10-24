import { readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { relative, resolve } from 'node:path';
import { pages } from './check-links.mjs';
import { siteBase } from '../site.mjs';
const docs = fileURLToPath(new URL('../../docs/', import.meta.url));
const output = fileURLToPath(new URL('../dist/', import.meta.url));
try {
  await stat(resolve(output, '.ref'));
  throw new Error('Internal planning directory leaked into site');
} catch (error) { if (error.code !== 'ENOENT') throw error; }
for (const page of await pages()) {
  const html = await readFile(resolve(output, relative(docs, page).replace(/\.md$/, '.html')), 'utf8');
  for (const match of html.matchAll(/(?:href|src)="([^"#?]+)(?:[?#][^"]*)?"/g)) {
    let link = match[1];
    if (/^(https?:|data:|mailto:)/.test(link)) continue;
    if (!link.startsWith('/')) continue;
    if (!link.startsWith(siteBase())) throw new Error(`Asset escapes site base: ${link}`);
    link = decodeURIComponent(link.slice(siteBase().length));
    const target = resolve(output, link.endsWith('/') || !link ? `${link}index.html` : link);
    await stat(target);
  }
}
console.log('Generated documentation pages and local asset links verified.');
