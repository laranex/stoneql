// Recreate the disposable driver/ copy from the verified vendor source (metadata-limit.patch adopted).
import { spawnSync } from 'node:child_process';
import { cpSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
const here = import.meta.dirname;
const root = resolve(here, '../..');
await import(resolve(root, 'tools/check-sqlx-vendor.mjs'));
const driver = resolve(here, 'driver');
rmSync(driver, { recursive: true, force: true });
cpSync(resolve(root, 'vendor/sqlx-mysql'), driver, { recursive: true });
// Adopted 2026-11-16 (A6): the verified vendor source already contains metadata-limit.patch,
// so the copy is used as is; `patch --dry-run -R` confirms the patch is present.
const applied = spawnSync('patch', ['-p1', '-R', '--dry-run', '--silent', '-i', resolve(here, 'metadata-limit.patch')], { cwd: driver, encoding: 'utf8' });
if (applied.status !== 0) throw Error(`vendor source does not contain metadata-limit.patch: ${applied.stdout}${applied.stderr}`);
console.log('Disposable patched driver regenerated under experiments/sqlx-metadata-limit/driver (ignored).');
