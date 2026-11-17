// Linux Secret Service provider against a disposable unlocked keyring on a private bus
// (tools/secret-service-fixture.mjs), under a per-run service name. The no-bus and
// no-Secret-Service refusals run in the ordinary unit tests.
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { mkdtempSync, existsSync, statSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { startSecretService } from './secret-service-fixture.mjs';

if (process.platform !== 'linux') {
  console.log('test:secret-service skipped: the Secret Service provider is Linux-only');
  process.exit(0);
}
const root = resolve(import.meta.dirname, '..');
const build = spawnSync('cargo', ['test', '--locked', '-p', 'stoneql-desktop', '--features', 'custom-protocol', '--lib', '--no-run', '--message-format=json'], { cwd: root, encoding: 'utf8', maxBuffer: 1 << 28 });
if (build.status !== 0) throw Error(`build failed\n${build.stderr.slice(-4000)}`);
const binary = build.stdout.split('\n').filter(Boolean).map(line => JSON.parse(line))
  .find(message => message.reason === 'compiler-artifact' && message.target.name === 'stoneql_desktop' && message.profile?.test && message.executable)?.executable;
if (!binary) throw Error('test binary missing');

const userKeyrings = join(homedir(), '.local/share/keyrings');
const before = existsSync(userKeyrings) ? statSync(userKeyrings).mtimeMs : null;
const service = `me.thixpin.stoneql.test-${randomBytes(6).toString('hex')}`;
const fixture = await startSecretService(mkdtempSync(join(tmpdir(), 'stoneql-secret-service-')), process.env);
let remaining;
try {
  for (const name of ['secrets::secret_service::tests::items_round_trip_in_the_session_secret_service', 'secrets::tests::a_present_secret_service_never_selects_the_vault']) {
    const result = spawnSync(binary, ['--ignored', '--exact', name, '--test-threads=1'], { cwd: root, env: { ...fixture.env, STONEQL_KEYCHAIN_SERVICE: service }, encoding: 'utf8', timeout: 120_000 });
    if (result.status !== 0 || result.error || !/1 passed/.test(result.stdout)) throw Error(`${name} failed\n${(result.stdout + result.stderr).slice(-4000)}`);
  }
  remaining = fixture.itemsRemaining(service);
} finally {
  fixture.purge(service);
  await fixture.stop();
}
// A present Secret Service without a usable collection must fail closed, never use the vault.
const unusable = await startSecretService(mkdtempSync(join(tmpdir(), 'stoneql-secret-service-')), process.env, { unlock: false });
try {
  const name = 'secrets::tests::an_unusable_secret_service_fails_closed_without_the_vault';
  const result = spawnSync(binary, ['--ignored', '--exact', name, '--test-threads=1'], { cwd: root, env: unusable.env, encoding: 'utf8', timeout: 120_000 });
  if (result.status !== 0 || result.error || !/1 passed/.test(result.stdout)) throw Error(`${name} failed\n${(result.stdout + result.stderr).slice(-4000)}`);
} finally {
  await unusable.stop();
}
const after = existsSync(userKeyrings) ? statSync(userKeyrings).mtimeMs : null;
if (remaining) throw Error(`${remaining} test item(s) remained`);
if (before !== after) throw Error('The user keyring directory changed');
console.log(`test:secret-service passed (round trip, replace, label and attributes, delete, present service never selects the vault, an unusable one fails closed without the vault; 0 items left; user keyrings ${before === null ? 'absent' : 'unchanged'})`);
