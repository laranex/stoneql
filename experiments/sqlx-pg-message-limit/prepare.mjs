// Recreate the disposable driver/ copy: registry sqlx-postgres 0.9.0 + message-limit.patch.
// `cargo fetch --locked` (root workspace) must have unpacked the registry source first.
import { spawnSync } from 'node:child_process';
import { cpSync, chmodSync, readdirSync, rmSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
const here = import.meta.dirname;
const home = process.env.CARGO_HOME || join(process.env.HOME, '.cargo');
const registry = join(home, 'registry/src');
const source = readdirSync(registry).map((index) => join(registry, index, 'sqlx-postgres-0.9.0')).find(existsSync);
if (!source) throw Error('sqlx-postgres 0.9.0 registry source missing; run cargo fetch --locked at the repository root');
const driver = resolve(here, 'driver');
rmSync(driver, { recursive: true, force: true });
cpSync(source, driver, { recursive: true });
const writable = (path) => { chmodSync(path, 0o755); for (const entry of readdirSync(path, { withFileTypes: true })) { const child = join(path, entry.name); if (entry.isDirectory()) writable(child); else chmodSync(child, 0o644); } };
writable(driver);
const applied = spawnSync('patch', ['-p1', '--forward', '--silent', '-i', resolve(here, 'message-limit.patch')], { cwd: driver, encoding: 'utf8' });
if (applied.status !== 0) throw Error(`message-limit.patch did not apply: ${applied.stdout}${applied.stderr}`);
console.log('Disposable patched driver regenerated under experiments/sqlx-pg-message-limit/driver (ignored).');
