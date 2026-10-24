import { spawnSync } from 'node:child_process';
import { fixtureEnvironment } from './fixture-environment.mjs';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir, arch, platform, release, cpus, totalmem } from 'node:os';
import { join, resolve } from 'node:path';
const root = resolve(import.meta.dirname, '..');
const baseEnv = fixtureEnvironment(process.env);
const output = resolve(root, 'artifacts/m001');
mkdirSync(output, { recursive: true });
function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', timeout: 240_000, env: baseEnv, ...options });
  if (result.error || result.status !== 0) throw new Error(`${command} failed (status ${result.status}); inspect local fixture availability. No connection credentials are logged.`);
  return result.stdout.trim();
}
run('cargo', ['build', '--locked', '-p', 'dbadmin-sqlx-spike']);
const binary = resolve(root, 'target/debug/dbadmin-sqlx-spike');
const compose = resolve(root, 'experiments/sqlx-spike/fixtures/compose.yml');
const daemon = process.env.DOCKER_HOST || run('docker', ['context', 'inspect', '--format', '{{.Endpoints.docker.Host}}']);
if (!daemon.startsWith('unix://') && !daemon.startsWith('npipe://')) throw new Error('Driver fixtures require a local Docker daemon; remote contexts are refused');
const metadata = { timestamp: new Date().toISOString(), cargoLockSha256: createHash('sha256').update(readFileSync(join(root, 'Cargo.lock'))).digest('hex'), arch: arch(), platform: platform(), kernel: release(), cpu: cpus()[0]?.model, logicalCpus: cpus().length, memoryBytes: totalmem(), rust: run('rustc', ['--version']), docker: run('docker', ['version', '--format', '{{.Server.Version}}']), images: {}, cleanup: {} };
const failures = [];
for (const engine of ['mysql', 'mariadb', 'postgresql', 'sqlite']) {
  const project = `dbadmin-m001-${engine}-${randomBytes(6).toString('hex')}`;
  const marker = randomUUID();
  const temp = mkdtempSync(join(tmpdir(), 'dbadmin-m001-fixture-'));
  const password = randomBytes(24).toString('hex');
  const env = { ...baseEnv, DBADMIN_FIXTURE_PASSWORD: password, DBADMIN_FIXTURE_INIT: temp, DBADMIN_SPIKE_FIXTURE_ID: marker };
  const args = ['compose', '-f', compose, '-p', project];
  writeFileSync(join(temp, '001-marker.sql'), `CREATE TABLE dbadmin_fixture_marker (id VARCHAR(64) PRIMARY KEY);
INSERT INTO dbadmin_fixture_marker VALUES ('${marker}');
`);
  try {
    if (engine !== 'sqlite') {
      run('docker', [...args, 'up', '-d', engine], { env });
      // Poll a query inside our own container; never reuse existing local databases.
      const deadline = Date.now() + 120_000;
      let ready = false;
      while (Date.now() < deadline) {
        const query = engine === 'postgresql'
          ? ['exec', '-T', engine, 'sh', '-c', 'PGPASSWORD="$POSTGRES_PASSWORD" exec psql -h127.0.0.1 -U dbadmin_fixture -d dbadmin_m001_fixture -tAc "SELECT id FROM dbadmin_fixture_marker"']
          : ['exec', '-T', engine, 'sh', '-c', engine === 'mysql'
            ? 'MYSQL_PWD="$MYSQL_ROOT_PASSWORD" exec mysql -h127.0.0.1 -uroot -N -e "SELECT id FROM dbadmin_m001_fixture.dbadmin_fixture_marker"'
            : 'MYSQL_PWD="$MARIADB_ROOT_PASSWORD" exec mariadb -h127.0.0.1 -uroot -N -e "SELECT id FROM dbadmin_m001_fixture.dbadmin_fixture_marker"'];
        const result = spawnSync('docker', [...args, ...query], { env, encoding: 'utf8', timeout: 10_000 });
        if (result.status === 0 && result.stdout.trim() === marker) { ready = true; break; }
        await new Promise(resolve => setTimeout(resolve, 500));
      }
      if (!ready) throw new Error(`${engine}: disposable fixture did not become ready`);
      const port = run('docker', [...args, 'port', engine, engine === 'postgresql' ? '5432' : '3306'], { env });
      if (!/^127[.]0[.]0[.]1:[0-9]+$/.test(port)) throw new Error(`Fixture must bind loopback only; received ${JSON.stringify(port)}`);
      env.DBADMIN_SPIKE_URL = `${engine === 'postgresql' ? 'postgres' : 'mysql'}://${engine === 'postgresql' ? 'dbadmin_fixture' : 'root'}:${password}@${port}/dbadmin_m001_fixture`;
      const id = run('docker', [...args, 'ps', '-q', engine], { env });
      metadata.images[engine] = JSON.parse(run('docker', ['inspect', '--format', '{{json .Image}}', id]));
    }
    const result = spawnSync(binary, [engine], { env, encoding: 'utf8', timeout: 120_000 });
    let report;
    try { report = JSON.parse(result.stdout); } catch { throw new Error(`${engine}: spike did not produce a safe JSON report`); }
    writeFileSync(join(output, `${engine}.json`), `${JSON.stringify(report, null, 2)}
`);
    console.log(`${engine}: ${report.probes.filter(p => p.status === 'passed').length} passed, ${report.probes.filter(p => p.status === 'failed').length} failed; other observations are explicit limits`);
    if (result.status !== 0) failures.push(engine);
  } finally {
    if (engine !== 'sqlite') {
      run('docker', [...args, 'down', '--volumes', '--remove-orphans'], { env });
      const remaining = run('docker', ['ps', '-a', '--filter', `label=com.docker.compose.project=${project}`, '-q']);
      const networks = run('docker', ['network', 'ls', '--filter', `label=com.docker.compose.project=${project}`, '-q']);
      metadata.cleanup[engine] = { remainingContainers: remaining ? remaining.split('\n').length : 0, remainingNetworks: networks ? networks.split('\n').length : 0 };
      if (remaining || networks) throw new Error(`${engine}: disposable resources remain after cleanup`);
    }
    rmSync(temp, { recursive: true, force: true });
    writeFileSync(join(output, 'environment.json'), `${JSON.stringify(metadata, null, 2)}
`);
  }
}

if (failures.length) throw new Error(`Driver observations failed for ${failures.join(', ')}; inspect artifacts/m001 reports.`);
