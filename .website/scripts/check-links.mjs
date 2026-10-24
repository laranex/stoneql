import { readdir, readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';
import { sidebar } from '../site.mjs';
const root = fileURLToPath(new URL('../../docs/', import.meta.url));
export async function pages(dir = root) {
  const entries = await readdir(dir, { withFileTypes: true });
  return (await Promise.all(entries.map(entry => entry.isDirectory() ? pages(resolve(dir, entry.name)) : entry.name.endsWith('.md') ? [resolve(dir, entry.name)] : []))).flat();
}
async function exists(path) { try { return (await stat(path)).isFile(); } catch { return false; } }
async function check(link, source) {
  const target = decodeURIComponent(link.split('#')[0].split('?')[0]);
  if (!target || /^(https?:|mailto:)/.test(target)) return;
  const path = target.startsWith('/') ? resolve(root, `.${target}`) : resolve(dirname(source), target);
  if (!(await Promise.all([path, `${path}.md`, resolve(path, 'index.md')].map(exists))).some(Boolean)) throw new Error(`Broken documentation link in ${source}: ${link}`);
}
for (const group of sidebar) for (const item of group.items) await check(item.link, resolve(root, 'index.md'));
for (const page of await pages()) {
  const source = (await readFile(page, 'utf8')).replace(/```[\s\S]*?```/g, '');
  for (const match of source.matchAll(/\[[^\]]*\]\(([^) ]+)(?: +[^)]*)?\)/g)) await check(match[1], page);
}
console.log('Documentation source links and navigation verified.');
