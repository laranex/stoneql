import { spawnSync } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { fixtureEnvironment } from './fixture-environment.mjs';
const root = resolve(import.meta.dirname, '..');
const base = fixtureEnvironment(process.env);
function run(command, args, env = base) {
  const result = spawnSync(command, args, { cwd: root, env, encoding: 'utf8', timeout: 240_000 });
  if (result.status !== 0 || result.error) {
    // The fixture binary emits only static errors/codes and synthetic assertion values.
    const message = command.endsWith('/examples/lifecycle')
      ? result.stderr.replaceAll(env.DBADMIN_FIXTURE_PASSWORD, '[redacted]')
      : `${command} fixture operation failed; credentials and raw errors withheld`;
    throw Error(message);
  }
  return result.stdout.trim();
}
const daemon = process.env.DOCKER_HOST || run('docker', ['context', 'inspect', '--format', '{{.Endpoints.docker.Host}}']);
if (!daemon.startsWith('unix://') && !daemon.startsWith('npipe://')) throw Error('Local disposable Docker daemon required');
run('cargo', ['build', '--locked', '-p', 'dbadmin-runtime', '--example', 'lifecycle']);
const output = resolve(root, 'artifacts/m003');
mkdirSync(output, { recursive: true });
for (const engine of ['mysql', 'mariadb']) {
  const temp = mkdtempSync(join(tmpdir(), 'dbadmin-lifecycle-'));
  const tls = join(temp, 'tls');
  const init = join(temp, 'init');
  mkdirSync(tls);
  mkdirSync(init);
  const project = `dbadmin-m003-${randomBytes(6).toString('hex')}`;
  const password = randomBytes(24).toString('hex');
  const marker = randomUUID();
  const env = { ...base, DBADMIN_FIXTURE_PASSWORD: password, DBADMIN_FIXTURE_INIT: init, DBADMIN_SPIKE_FIXTURE_ID: marker, DBADMIN_LIFECYCLE_ENGINE: engine };
  const override = join(temp, 'compose.json');
  const args = ['compose', '-f', resolve(root, 'experiments/sqlx-spike/fixtures/compose.yml'), '-f', override, '-p', project];
  try {
    for (const name of ['ca', 'bad-ca']) run('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(tls, `${name}-key.pem`), '-out', join(tls, `${name}.pem`), '-days', '1', '-subj', `/CN=dbAdmin disposable ${name}`]);
    run('openssl', ['req', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(tls, 'server-key.pem'), '-out', join(tls, 'server.csr'), '-subj', '/CN=localhost']);
    writeFileSync(join(tls, 'extensions.cnf'), 'subjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n');
    run('openssl', ['x509', '-req', '-in', join(tls, 'server.csr'), '-CA', join(tls, 'ca.pem'), '-CAkey', join(tls, 'ca-key.pem'), '-CAcreateserial', '-out', join(tls, 'server.pem'), '-days', '1', '-extfile', join(tls, 'extensions.cnf')]);
    // Only disposable server material is mounted. Private parent directory remains mkdtemp-protected.
    const exposed = join(temp, 'server-tls');
  mkdirSync(exposed);
    for (const name of ['ca.pem', 'server.pem', 'server-key.pem']) writeFileSync(join(exposed, name), readFileSync(join(tls, name)), { mode: 0o644 });
    writeFileSync(override, JSON.stringify({ services: { [engine]: { volumes: [`${exposed}:/dbadmin-tls:ro`], command: ['--ssl-ca=/dbadmin-tls/ca.pem', '--ssl-cert=/dbadmin-tls/server.pem', '--ssl-key=/dbadmin-tls/server-key.pem', '--require-secure-transport=ON'] } } }));
    writeFileSync(join(init, '001-fixture.sql'), `CREATE TABLE dbadmin_fixture_marker(id VARCHAR(64) PRIMARY KEY); INSERT INTO dbadmin_fixture_marker VALUES ('${marker}');
SET SESSION sql_mode='';
CREATE TABLE raw_values(id INT PRIMARY KEY, s BIGINT, u BIGINT UNSIGNED, d DECIMAL(38,4), b VARBINARY(4), dt DATETIME(6), zd DATE, zt DATETIME, tm TIME(6), e ENUM('active','suspended'), txt VARCHAR(20), n VARCHAR(20));
INSERT INTO raw_values VALUES (1,-9223372036854775808,18446744073709551615,123456789012345678901234567890.4500,0x00ff0180,'2024-02-29 12:34:56.123456','0000-00-00','0000-00-00 00:00:00','-26:03:04.123456','active','',NULL),(2,0,0,0,x'','2024-01-01','2024-01-01','2024-01-01','00:00:00','suspended','active',NULL);
CREATE USER 'dbadmin_writer'@'%' IDENTIFIED BY '${password}' REQUIRE SSL; GRANT SELECT, INSERT, UPDATE, DELETE ON dbadmin_m001_fixture.* TO 'dbadmin_writer'@'%';
CREATE USER 'dbadmin_reader'@'%' IDENTIFIED BY '${password}' REQUIRE SSL; GRANT SELECT ON dbadmin_m001_fixture.* TO 'dbadmin_reader'@'%';`);
    run('docker', [...args, 'up', '-d', engine], env);
    let ready = false;
    const deadline = Date.now() + 120_000;
    const cli = engine === 'mysql'
      ? 'MYSQL_PWD="$MYSQL_ROOT_PASSWORD" exec mysql -uroot -N -e "SELECT id FROM dbadmin_m001_fixture.dbadmin_fixture_marker"'
      : 'MYSQL_PWD="$MARIADB_ROOT_PASSWORD" exec mariadb -uroot -N -e "SELECT id FROM dbadmin_m001_fixture.dbadmin_fixture_marker"';
    while (Date.now() < deadline) {
      const result = spawnSync('docker', [...args, 'exec', '-T', engine, 'sh', '-c', cli], { env, encoding: 'utf8', timeout: 10_000 });
      if (result.status === 0 && result.stdout.trim() === marker) {
        ready = true;
        break;
      }
      const state = run('docker', [...args, 'ps', '-a', '--format', 'json', engine], env);
      if (state.split('\n').filter(Boolean).some(line => JSON.parse(line).State === 'exited')) break;
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    if (!ready) throw Error(`${engine}: disposable TLS fixture not ready`);
    const port = run('docker', [...args, 'port', engine, '3306'], env); if (!/^127\.0\.0\.1:\d+$/.test(port)) throw Error('Loopback fixture required');
    Object.assign(env, { DBADMIN_LIFECYCLE_PORT: port.split(':')[1], DBADMIN_LIFECYCLE_CA: join(tls, 'ca.pem'), DBADMIN_LIFECYCLE_BAD_CA: join(tls, 'bad-ca.pem') });
    const result = run(resolve(root, 'target/debug/examples/lifecycle'), [], env);
    const report = JSON.parse(result);
    writeFileSync(join(output, `${engine}.json`), JSON.stringify(report, null, 2) + '\n');
    console.log(`${engine}: ${report.lifecycleSamples} TLS lifecycle samples passed`);
  } finally {
    run('docker', [...args, 'down', '--volumes', '--remove-orphans'], env);
    if (run('docker', ['ps', '-a', '--filter', `label=com.docker.compose.project=${project}`, '-q']) || run('docker', ['network', 'ls', '--filter', `label=com.docker.compose.project=${project}`, '-q'])) throw Error('Owned fixture resources remain');
    rmSync(temp, { recursive: true, force: true });
  }
}
