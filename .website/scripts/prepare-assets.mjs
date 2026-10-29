import { copyFile, mkdir, writeFile, rm } from 'node:fs/promises';
const target = new URL('../public/', import.meta.url);
await mkdir(target, { recursive: true });
await copyFile(new URL('../../apps/frontend/public/icon.png', import.meta.url), new URL('icon.png', target));
const domain = process.env.DOCS_CUSTOM_DOMAIN;
if (domain) {
  if (!/^(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]*[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$/.test(domain)) throw new Error('Invalid custom domain');
  if (process.env.DOCS_BASE && process.env.DOCS_BASE !== '/') throw new Error('Custom domains require DOCS_BASE=/');
  await writeFile(new URL('CNAME', target), `${domain}\n`);
} else await rm(new URL('CNAME', target), { force: true });
