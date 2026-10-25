import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve, join, relative } from 'node:path';
const root = resolve(import.meta.dirname, '..');
const vendor = join(root, 'vendor/sqlx-mysql');
const provenance = JSON.parse(readFileSync(join(vendor, 'dbadmin-provenance.json'), 'utf8'));
const approved = JSON.parse(readFileSync(join(root, 'experiments/sqlx-receive-limit/upstream.json'), 'utf8'));
for (const key of ['registryPackageSha256', 'patchSha256', 'mitLicenseSha256']) {
  if (provenance[key] !== approved[key]) throw Error(`SQLx provenance drift: ${key}`);
}
const patchHash = createHash('sha256').update(readFileSync(join(root, 'experiments/sqlx-receive-limit/receive-limit.patch'))).digest('hex');
if (patchHash !== approved.patchSha256) throw Error('SQLx provenance drift: patch content');
const actual = {};
function visit(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) visit(path);
    else if (entry.isFile() && entry.name !== 'dbadmin-provenance.json') actual[relative(vendor,path).replaceAll('\\','/')] = createHash('sha256').update(readFileSync(path)).digest('hex');
    else if (!entry.isFile()) throw Error('Unexpected file type in SQLx source');
  }
}
visit(vendor);
if (Object.keys(actual).length !== Object.keys(provenance.files).length || Object.entries(actual).some(([name,hash]) => provenance.files[name] !== hash)) throw Error('SQLx source changed; review patch and provenance before adoption');
console.log(`SQLx source provenance verified (${Object.keys(actual).length} files).`);
