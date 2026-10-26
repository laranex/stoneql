// Legitimate-query metadata measurements on owned, disposable MySQL 8.4.6 / MariaDB 11.4.8
// fixtures. Reuses the repository fixture compose file, Docker-context guard and RSS parser.
// Writes safe JSON (no credentials, no row data) to .ref/planning/m003-metadata-limit/.
import { spawnSync } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir, cpus, totalmem, release, arch } from 'node:os';
import { resolve, join } from 'node:path';
import { fixtureEnvironment, assertLocalFixtureDocker } from '../../tools/fixture-environment.mjs';
import { parseMaximumRss } from '../../tools/resource-report.mjs';

if (!['darwin', 'linux'].includes(process.platform)) throw Error('Requires macOS or Linux');
const root = resolve(import.meta.dirname, '../..');
const base = fixtureEnvironment(process.env);
const output = resolve(root, '.ref/planning/m003-metadata-limit');
const binary = resolve(root, 'target/metadata-limit/debug/examples/live_metadata');
const engines = process.argv.slice(2).length ? process.argv.slice(2) : ['mysql', 'mariadb'];
if (engines.some(engine => !['mysql', 'mariadb'].includes(engine))) throw Error('Unknown engine');

function run(command, args, env = base, timeout = 240_000) {
  const result = spawnSync(command, args, { cwd: root, env, encoding: 'utf8', timeout });
  if (result.status !== 0 || result.error) throw Error(`${command} fixture operation failed; raw errors withheld`);
  return result.stdout.trim();
}
assertLocalFixtureDocker(base, context => run('docker', ['context', 'inspect', ...(context ? [context] : []), '--format', '{{.Endpoints.docker.Host}}']));
run('cargo', ['build', '--locked', '--offline', '--manifest-path', 'experiments/sqlx-metadata-limit/Cargo.toml', '--target-dir', 'target/metadata-limit', '--example', 'live_metadata']);
mkdirSync(output, { recursive: true });

function measure(env, args) {
  const measured = spawnSync('/usr/bin/time', [process.platform === 'darwin' ? '-l' : '-v', binary, ...args], { cwd: root, env, encoding: 'utf8', timeout: 120_000 });
  const line = measured.stdout.trim().split('\n').at(-1) || '';
  if (measured.status !== 0 || measured.error || !line.startsWith('{')) {
    return { args, failed: true, exitStatus: measured.status, signal: measured.signal };
  }
  return { ...JSON.parse(line), processMaxRssBytes: parseMaximumRss(measured.stderr, process.platform) };
}

for (const engine of engines) {
  const temp = mkdtempSync(join(tmpdir(), 'stoneql-metadata-'));
  const init = join(temp, 'init');
  mkdirSync(init);
  const project = `stoneql-m003-meta-${randomBytes(6).toString('hex')}`;
  const password = randomBytes(24).toString('hex');
  const marker = randomUUID();
  const env = { ...base, STONEQL_FIXTURE_PASSWORD: password, STONEQL_FIXTURE_INIT: init, STONEQL_SPIKE_FIXTURE_ID: marker };
  const args = ['compose', '-f', resolve(root, 'experiments/sqlx-spike/fixtures/compose.yml'), '-p', project];
  writeFileSync(join(init, '001-fixture.sql'), `CREATE TABLE stoneql_fixture_marker(id VARCHAR(64) PRIMARY KEY); INSERT INTO stoneql_fixture_marker VALUES ('${marker}');`);
  let started = false;
  const cleanup = { attempted: false, remainingContainers: null, remainingNetworks: null, temporaryRemoved: false };
  try {
    started = true;
    run('docker', [...args, 'up', '-d', engine], env);
    const cli = engine === 'mysql'
      ? 'MYSQL_PWD="$MYSQL_ROOT_PASSWORD" exec mysql -uroot -N -e "SELECT id FROM stoneql_m001_fixture.stoneql_fixture_marker"'
      : 'MYSQL_PWD="$MARIADB_ROOT_PASSWORD" exec mariadb -uroot -N -e "SELECT id FROM stoneql_m001_fixture.stoneql_fixture_marker"';
    const deadline = Date.now() + 120_000;
    let ready = false;
    while (!ready && Date.now() < deadline) {
      const probe = spawnSync('docker', [...args, 'exec', '-T', engine, 'sh', '-c', cli], { env, encoding: 'utf8', timeout: 10_000 });
      const tcp = probe.status === 0 && probe.stdout.trim() === marker
        && spawnSync('docker', [...args, 'exec', '-T', engine, 'bash', '-c', 'exec 3<>/dev/tcp/127.0.0.1/3306'], { env, timeout: 10_000 }).status === 0;
      if (tcp) ready = true; else await new Promise(done => setTimeout(done, 500));
    }
    if (!ready) throw Error(`${engine}: disposable fixture not ready`);
    const port = run('docker', [...args, 'port', engine, '3306'], env);
    if (!/^127\.0\.0\.1:\d+$/.test(port)) throw Error('Loopback fixture required');
    env.STONEQL_METADATA_PORT = port.split(':')[1];

    const setup = JSON.parse(run(binary, ['setup'], env).split('\n').at(-1));
    const samples = [];
    // Probe the largest projected-expression SELECT each engine accepts (unpatched driver).
    for (const columns of [1, 4096, 4097, 8192, 16384, 32768, 65535, 65536]) samples.push(measure(env, ['baseline', 'expr', String(columns)]));
    const accepted = samples.filter(sample => sample.outcome === 'ok').map(sample => sample.requestedColumns);
    const maxAccepted = Math.max(...accepted);
    // Unpatched vs proposed limits for legitimate shapes, including over-cap requests.
    const shapes = [['expr', 512], ['expr', 2048], ['expr', 4096], ['alias64', 4096], ['expr', 4097], ['expr', maxAccepted]];
    if (setup.wideTableColumns > 0) shapes.push(['table', setup.wideTableColumns]);
    for (const mode of ['baseline', 'limited']) {
      for (const [kind, columns] of shapes) samples.push(measure(env, [mode, kind, String(columns)]));
    }
    const report = {
      label: 'Local debug characterization; not a release budget. Standalone client process RSS only (excludes Docker server, Tauri, WebView).',
      engine, setup, maxAcceptedProbe: maxAccepted, samples,
      machine: { platform: process.platform, arch: arch(), osRelease: release(), cpu: cpus()[0]?.model, logicalCpus: cpus().length, totalMemoryBytes: totalmem(), node: process.version },
      recordedAt: new Date().toISOString(),
    };
    writeFileSync(join(output, `live-${engine}.json`), JSON.stringify(report, null, 2) + '\n');
    console.log(`${engine}: ${samples.length} metadata samples recorded (max accepted probe ${maxAccepted})`);
  } finally {
    try {
      if (started) {
        cleanup.attempted = true;
        run('docker', [...args, 'down', '--volumes', '--remove-orphans'], env);
        const containers = run('docker', ['ps', '-a', '--filter', `label=com.docker.compose.project=${project}`, '-q']);
        const networks = run('docker', ['network', 'ls', '--filter', `label=com.docker.compose.project=${project}`, '-q']);
        cleanup.remainingContainers = containers ? containers.split('\n').length : 0;
        cleanup.remainingNetworks = networks ? networks.split('\n').length : 0;
        if (containers || networks) throw Error('Owned fixture resources remain');
      }
    } finally {
      rmSync(temp, { recursive: true, force: true });
      cleanup.temporaryRemoved = !existsSync(temp);
      writeFileSync(join(output, `cleanup-${engine}.json`), JSON.stringify(cleanup, null, 2) + '\n');
    }
  }
}
