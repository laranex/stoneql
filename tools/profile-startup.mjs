// Release startup profile (M002 G4/G5): builds the optimized desktop host with the `smoke`
// feature (the shipped release never has it) and launches it N times. Each launch opens a real
// window, bootstraps the renderer through `host_info` + `contract_probe` (echo, ready) and exits;
// `startupMs` is host start → renderer ready, wall is spawn → process exit. Launches run
// sequentially; the first follows the build, so it is the coldest available without a cache purge.
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const launches = Number(process.argv[2] ?? 30);
if (!Number.isInteger(launches) || launches < 1 || launches > 200) throw Error('launches must be 1..200');
const build = spawnSync('cargo', ['build', '--release', '--locked', '-q', '-p', 'stoneql-desktop', '--features', 'custom-protocol,smoke'], { cwd: root, stdio: 'inherit' });
if (build.status !== 0) throw Error('profile build failed');
// Copy out so the next plain `desktop:release` cannot be confused with this binary.
const temp = mkdtempSync(join(tmpdir(), 'stoneql-profile-'));
const binary = join(temp, 'stoneql-desktop');
copyFileSync(join(root, 'target/release/stoneql-desktop'), binary);
const startup = [], wall = [], failures = [];
try {
  for (let i = 0; i < launches; i++) {
    const begin = process.hrtime.bigint();
    const run = spawnSync(binary, ['--smoke-test'], { encoding: 'utf8', timeout: 20_000 });
    const elapsed = Number(process.hrtime.bigint() - begin) / 1e6;
    const line = run.stdout.split('\n').find((text) => text.includes('nativeSmokePassed'));
    if (run.status !== 0 || !line) failures.push({ launch: i + 1, status: run.status, signal: run.signal });
    else { startup.push(JSON.parse(line).startupMs); wall.push(Math.round(elapsed)); }
  }
} finally {
  rmSync(temp, { recursive: true, force: true });
}
const stats = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const at = (q) => sorted[Math.min(sorted.length - 1, Math.ceil(q * sorted.length) - 1)];
  return { first: values[0], min: sorted[0], median: at(0.5), p95: at(0.95), max: sorted.at(-1) };
};
const report = { launches, passed: startup.length, failures, profile: 'release + smoke feature', arch: process.arch, startupMs: stats(startup), wallMs: stats(wall) };
console.log(JSON.stringify(report));
if (failures.length) process.exit(1);
