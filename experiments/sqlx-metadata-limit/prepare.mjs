// Recreate the disposable driver/ copy: verified approved vendor source + metadata-limit.patch.
import { spawnSync } from 'node:child_process';
import { cpSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
const here = import.meta.dirname;
const root = resolve(here, '../..');
await import(resolve(root, 'tools/check-sqlx-vendor.mjs'));
const driver = resolve(here, 'driver');
rmSync(driver, { recursive: true, force: true });
cpSync(resolve(root, 'vendor/sqlx-mysql'), driver, { recursive: true });
const applied = spawnSync('patch', ['-p1', '--forward', '--silent', '-i', resolve(here, 'metadata-limit.patch')], { cwd: driver, encoding: 'utf8' });
if (applied.status !== 0) throw Error(`metadata-limit.patch did not apply: ${applied.stdout}${applied.stderr}`);
console.log('Disposable patched driver regenerated under experiments/sqlx-metadata-limit/driver (ignored).');
