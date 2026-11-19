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
// so the copy is used as is; `patch --dry-run -R` confirms the patch is present. The later
// rsa-key.patch (2026-11-19) touches the same field lists, so it is reversed on a scratch copy
// first.
const check = resolve(here, 'driver-check');
rmSync(check, { recursive: true, force: true });
cpSync(driver, check, { recursive: true });
const later = spawnSync('patch', ['-p1', '-R', '--silent', '-i', resolve(root, 'experiments/sqlx-rsa-key/rsa-key.patch')], { cwd: check, encoding: 'utf8' });
if (later.status !== 0) throw Error(`vendor source does not contain rsa-key.patch: ${later.stdout}${later.stderr}`);
const applied = spawnSync('patch', ['-p1', '-R', '--dry-run', '--silent', '-i', resolve(here, 'metadata-limit.patch')], { cwd: check, encoding: 'utf8' });
rmSync(check, { recursive: true, force: true });
if (applied.status !== 0) throw Error(`vendor source does not contain metadata-limit.patch: ${applied.stdout}${applied.stderr}`);
console.log('Disposable patched driver regenerated under experiments/sqlx-metadata-limit/driver (ignored).');
