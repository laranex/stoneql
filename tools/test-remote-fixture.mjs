// Runs the native real-database tests (live_database, live_postgres) on this machine against
// disposable fixtures on the remote fixture host (tools/ci-fixture-host/). Used by the Windows CI
// job, whose runner cannot run the Docker fixtures. The host's harness owns the containers and
// performs every check a local run does; this client only builds and runs the test binaries.
//
// Environment (CI secrets; values are never printed):
//   STONEQL_REMOTE_HOST         user@host of the restricted fixture account
//   STONEQL_REMOTE_KEY          its private key (contents)
//   STONEQL_REMOTE_KNOWN_HOSTS  the host's pinned known_hosts line(s) (contents)
//   STONEQL_REMOTE_COMMIT       commit the host must test (default: GITHUB_SHA, else HEAD)
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { connect } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { fixtureEnvironment } from './fixture-environment.mjs';
import { READY } from './remote-fixture.mjs';

const root = resolve(import.meta.dirname, '..');
const base = fixtureEnvironment(process.env);
for (const name of ['STONEQL_REMOTE_HOST', 'STONEQL_REMOTE_KEY', 'STONEQL_REMOTE_KNOWN_HOSTS']) {
  if (!process.env[name]) throw Error(`${name} is required`);
}
const commit = process.env.STONEQL_REMOTE_COMMIT || process.env.GITHUB_SHA
  || spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).stdout.trim();
if (!/^[0-9a-f]{40}$/.test(commit)) throw Error('A full commit id is required');

function cargoTests() {
  const result = spawnSync('cargo', ['test', '--locked', '-p', 'stoneql-desktop', '--features', 'custom-protocol', '--test', 'live_database', '--test', 'live_postgres', '--no-run', '--message-format=json'], { cwd: root, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, stdio: ['ignore', 'pipe', 'inherit'] });
  if (result.status !== 0) throw Error('Building the native test binaries failed');
  const executable = (name) => result.stdout.split('\n').filter(Boolean).map((line) => JSON.parse(line))
    .find((message) => message.reason === 'compiler-artifact' && message.target.name === name && message.executable)?.executable;
  return { live_database: executable('live_database'), live_postgres: executable('live_postgres') };
}

const temp = mkdtempSync(join(tmpdir(), 'stoneql-remote-'));
const secretFile = (name, contents) => {
  const path = join(temp, name);
  writeFileSync(path, contents.replaceAll('\r', '').trimEnd() + '\n', { mode: 0o600 });
  return path;
};
// `-F none`: no user ssh_config (its identities or host settings never apply).
const ssh = [
  '-F', 'none', '-i', secretFile('key', process.env.STONEQL_REMOTE_KEY),
  '-o', `UserKnownHostsFile=${secretFile('known_hosts', process.env.STONEQL_REMOTE_KNOWN_HOSTS)}`,
  '-o', 'StrictHostKeyChecking=yes', '-o', 'IdentitiesOnly=yes', '-o', 'BatchMode=yes',
  '-o', 'ConnectTimeout=20', '-o', 'ServerAliveInterval=15',
];

function waitForPort(port) {
  const deadline = Date.now() + 30_000;
  return new Promise((done, fail) => {
    const attempt = () => {
      const socket = connect(port, '127.0.0.1');
      socket.once('connect', () => { socket.destroy(); done(); });
      socket.once('error', () => {
        socket.destroy();
        if (Date.now() > deadline) fail(Error('SSH tunnel did not open'));
        else setTimeout(attempt, 250);
      });
    };
    attempt();
  });
}

// One fixture session: `mode` is the host harness (`native`: MySQL then MariaDB; `postgres`).
async function session(mode, binary, test) {
  const host = spawn('ssh', [...ssh, process.env.STONEQL_REMOTE_HOST, `${mode} ${commit}`], { stdio: ['pipe', 'pipe', 'inherit'] });
  const exited = new Promise((done) => host.on('exit', (code) => done(code)));
  const secrets = [];
  const redact = (text) => secrets.reduce((value, secret) => value.replaceAll(secret, '[redacted]'), text);
  for await (const line of createInterface({ input: host.stdout })) {
    if (!line.startsWith(READY)) {
      console.log(redact(line));
      continue;
    }
    const fixture = JSON.parse(line.slice(READY.length));
    secrets.push(fixture.password);
    if (process.env.GITHUB_ACTIONS) console.log(`::add-mask::${fixture.password}`);
    const dir = mkdtempSync(join(temp, `${fixture.engine}-`));
    const exports = join(dir, 'exports');
    mkdirSync(exports);
    // The local port matches the host's so the run is easy to correlate; TLS verifies 127.0.0.1.
    const tunnel = spawn('ssh', [...ssh, '-N', '-o', 'ExitOnForwardFailure=yes', '-L', `127.0.0.1:${fixture.port}:127.0.0.1:${fixture.port}`, process.env.STONEQL_REMOTE_HOST], { stdio: 'ignore' });
    let reply = { passed: false };
    try {
      await waitForPort(fixture.port);
      const env = {
        ...base,
        STONEQL_FIXTURE_PASSWORD: fixture.password,
        STONEQL_SPIKE_FIXTURE_ID: fixture.marker,
        STONEQL_LIFECYCLE_ENGINE: fixture.engine,
        STONEQL_LIFECYCLE_PORT: String(fixture.port),
        STONEQL_LIFECYCLE_CA: secretFile(`${fixture.engine}-ca.pem`, fixture.ca),
        STONEQL_LIFECYCLE_BAD_CA: secretFile(`${fixture.engine}-bad-ca.pem`, fixture.badCa),
        STONEQL_EXPORT_DIR: exports,
      };
      // A further test named by the host (the RSA phases after a server restart).
      if (fixture.test) {
        if (fixture.test !== 'rsa_sign_in_without_tls_after_server_restart') throw Error('Unexpected fixture test');
        env.STONEQL_RSA_PHASE = fixture.rsaPhase;
        if (fixture.serverPublicKey) env.STONEQL_SERVER_PUBLIC_KEY = secretFile(`${fixture.engine}-${fixture.rsaPhase}-server-key.pem`, fixture.serverPublicKey);
      }
      const result = spawnSync(binary, ['--ignored', '--exact', fixture.test ?? test, '--nocapture'], { cwd: root, env, encoding: 'utf8', timeout: 600_000, maxBuffer: 64 * 1024 * 1024 });
      if (result.status === 0) {
        reply = fixture.test ? { passed: true, stdout: result.stdout } : {
          passed: true,
          stdout: result.stdout,
          exports: Object.fromEntries(['export_values.sql', 'export_values.csv'].map((name) => [name, readFileSync(join(exports, name)).toString('base64')])),
        };
      } else {
        console.error(redact(`${fixture.engine}: native test failed\n${result.stdout}\n${result.stderr}${result.error ? `\n${result.error.message}` : ''}`));
      }
    } finally {
      tunnel.kill();
    }
    host.stdin.write(JSON.stringify(reply) + '\n');
  }
  host.stdin.end();
  const code = await exited;
  if (code !== 0) throw Error(`${mode}: fixture host reported failure (exit ${code})`);
}

try {
  const binaries = cargoTests();
  if (!binaries.live_database || !binaries.live_postgres) throw Error('Native test binaries missing');
  await session('native', binaries.live_database, 'actual_database_commands_use_owned_tls_fixtures');
  await session('postgres', binaries.live_postgres, 'postgres_commands_use_owned_tls_fixture');
  console.log('Remote real-database checks passed (MySQL, MariaDB, PostgreSQL)');
} finally {
  rmSync(temp, { recursive: true, force: true });
}
