import { spawnSync } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { fixtureEnvironment, assertLocalFixtureDocker } from './fixture-environment.mjs';
const root = resolve(import.meta.dirname, '..');
const base = fixtureEnvironment(process.env);
const packetEvaluation = process.argv.slice(2).includes('--packet-limit');
const nativeEvaluation = process.argv.slice(2).includes('--native-ipc');
if(packetEvaluation && nativeEvaluation)throw Error('Choose a single fixture mode');
let nativeBinary;
if (process.argv.slice(2).some(argument => !['--packet-limit','--native-ipc'].includes(argument))) throw Error('Unknown fixture option');
function run(command, args, env = base) {
  const result = spawnSync(command, args, { cwd: root, env, encoding: 'utf8', timeout: 240_000 });
  if (result.status !== 0 || result.error) {
    // The fixture binary emits only static errors/codes and synthetic assertion values.
    const message = (nativeEvaluation && command===nativeBinary || command.endsWith('/examples/lifecycle') || command.endsWith('/examples/live_packet_limits'))
      ? result.stderr.replaceAll(env.DBADMIN_FIXTURE_PASSWORD, '[redacted]')
      : `${command} fixture operation failed; credentials and raw errors withheld`;
    throw Error(message);
  }
  return result.stdout.trim();
}
assertLocalFixtureDocker(base, context => run('docker', ['context', 'inspect', ...(context ? [context] : []), '--format', '{{.Endpoints.docker.Host}}']));
if(nativeEvaluation){
  const artifacts=run('cargo',['test','--locked','-p','dbadmin-desktop','--features','custom-protocol','--test','live_database','--no-run','--message-format=json']);
  nativeBinary=artifacts.split('\n').map(line=>JSON.parse(line)).find(message=>message.reason==='compiler-artifact'&&message.target.name==='live_database'&&message.executable)?.executable;
  if(!nativeBinary)throw Error('Native fixture test binary missing');
} else if (packetEvaluation) {
  run('cargo', ['build', '--locked', '--manifest-path', 'experiments/sqlx-receive-limit/Cargo.toml', '--target-dir', 'target/receive-limit', '--example', 'live_packet_limits']);
} else {
  run('cargo', ['build', '--locked', '-p', 'dbadmin-runtime', '--example', 'lifecycle']);
}
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
  async function waitForFixture({ expired = false } = {}) {
    let ready = false;
    let lastError = '';
    const deadline = Date.now() + 120_000;
    const cli = engine === 'mysql'
      ? 'MYSQL_PWD="$MYSQL_ROOT_PASSWORD" exec mysql -uroot -N -e "SELECT id FROM dbadmin_m001_fixture.dbadmin_fixture_marker"'
      // MariaDB's CLI enables TLS even on its Unix socket. The expired phase's local
      // readiness observer is socket-only; dbAdmin still uses verified TLS over TCP.
      : expired
        ? 'MYSQL_PWD="$MARIADB_ROOT_PASSWORD" exec mariadb --protocol=SOCKET --skip-ssl -uroot -N -e "SELECT id FROM dbadmin_m001_fixture.dbadmin_fixture_marker"'
        : 'MYSQL_PWD="$MARIADB_ROOT_PASSWORD" exec mariadb -uroot -N -e "SELECT id FROM dbadmin_m001_fixture.dbadmin_fixture_marker"';
    while (Date.now() < deadline) {
      const result = spawnSync('docker', [...args, 'exec', '-T', engine, 'sh', '-c', cli], { env, encoding: 'utf8', timeout: 10_000 });
      if (result.status === 0 && result.stdout.trim() === marker) {
        // Image entrypoints initialize through a socket-only server. The marker can
        // exist before the final TCP server starts, so require its listener too.
        const tcp = spawnSync('docker', [...args, 'exec', '-T', engine, 'bash', '-c', 'exec 3<>/dev/tcp/127.0.0.1/3306'], { env, encoding: 'utf8', timeout: 10_000 });
        if (tcp.status === 0) {
          ready = true;
          break;
        }
      }
      lastError = result.stderr.match(/^ERROR [0-9]+(?: \([A-Z0-9]+\))?/m)?.[0] || 'not_ready';
      if (result.stderr.toLowerCase().includes('expired')) lastError += ': certificate_expired';
      const state = run('docker', [...args, 'ps', '-a', '--format', 'json', engine], env);
      if (state.split('\n').filter(Boolean).some(line => JSON.parse(line).State === 'exited')) break;
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    if (!ready) throw Error(`${engine}: disposable TLS fixture not ready (${lastError})`);
    const port = run('docker', [...args, 'port', engine, '3306'], env); if (!/^127\.0\.0\.1:\d+$/.test(port)) throw Error('Loopback fixture required');
    Object.assign(env, { DBADMIN_LIFECYCLE_PORT: port.split(':')[1], DBADMIN_LIFECYCLE_CA: join(tls, 'ca.pem'), DBADMIN_LIFECYCLE_BAD_CA: join(tls, 'bad-ca.pem') });
  }
  let started = false;
  const cleanup = { attempted: false, remainingContainers: null, remainingNetworks: null, temporaryRemoved: false };
  try {
    writeFileSync(join(tls, 'root.cnf'), `[req]
distinguished_name = dn
x509_extensions = v3_ca
[dn]
CN = dbAdmin disposable CA
[v3_ca]
basicConstraints = critical,CA:true
keyUsage = critical,keyCertSign,cRLSign
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always
`);
    for (const name of ['ca', 'bad-ca']) run('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(tls, `${name}-key.pem`), '-out', join(tls, `${name}.pem`), '-days', '1', '-config', join(tls, 'root.cnf'), '-extensions', 'v3_ca', '-subj', `/CN=dbAdmin disposable ${name}`]);
    run('openssl', ['req', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(tls, 'server-key.pem'), '-out', join(tls, 'server.csr'), '-subj', '/CN=localhost']);
    writeFileSync(join(tls, 'extensions.cnf'), 'subjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n');
    run('openssl', ['x509', '-req', '-in', join(tls, 'server.csr'), '-CA', join(tls, 'ca.pem'), '-CAkey', join(tls, 'ca-key.pem'), '-CAcreateserial', '-out', join(tls, 'server.pem'), '-days', '1', '-extfile', join(tls, 'extensions.cnf')]);
    if (!packetEvaluation && !nativeEvaluation) {
      mkdirSync(join(tls, 'issued'));
      writeFileSync(join(tls, 'index.txt'), '');
      writeFileSync(join(tls, 'serial'), '1000\n');
      writeFileSync(join(tls, 'ca.cnf'), `[ca]
default_ca = fixture_ca
[fixture_ca]
dir = "${tls}"
database = $dir/index.txt
serial = $dir/serial
new_certs_dir = $dir/issued
default_md = sha256
policy = fixture_policy
[fixture_policy]
commonName = supplied
[server]
subjectAltName = IP:127.0.0.1
extendedKeyUsage = serverAuth
`);
      run('openssl', ['ca', '-config', join(tls, 'ca.cnf'), '-cert', join(tls, 'ca.pem'), '-keyfile', join(tls, 'ca-key.pem'), '-in', join(tls, 'server.csr'), '-out', join(tls, 'expired-server.pem'), '-startdate', '20000101000000Z', '-enddate', '20000102000000Z', '-batch', '-notext', '-extensions', 'server']);
    }
    // Only disposable server material is mounted. Private parent directory remains mkdtemp-protected.
    const exposed = join(temp, 'server-tls');
    mkdirSync(exposed);
    for (const name of ['ca.pem', 'server.pem', 'server-key.pem']) writeFileSync(join(exposed, name), readFileSync(join(tls, name)), { mode: 0o644 });
    writeFileSync(override, JSON.stringify({ services: { [engine]: { volumes: [`${exposed}:/dbadmin-tls:ro`], command: ['--ssl-ca=/dbadmin-tls/ca.pem', '--ssl-cert=/dbadmin-tls/server.pem', '--ssl-key=/dbadmin-tls/server-key.pem', '--require-secure-transport=ON'] } } }));
    writeFileSync(join(init, '001-fixture.sql'), `CREATE TABLE dbadmin_fixture_marker(id VARCHAR(64) PRIMARY KEY); INSERT INTO dbadmin_fixture_marker VALUES ('${marker}');
SET SESSION sql_mode='';
CREATE TABLE raw_large(payload LONGBLOB);
INSERT INTO raw_large VALUES(REPEAT('x',3145728));
CREATE TABLE raw_values(id INT PRIMARY KEY, s BIGINT, u BIGINT UNSIGNED, d DECIMAL(38,4), b VARBINARY(4), dt DATETIME(6), zd DATE, zt DATETIME, tm TIME(6), e ENUM('active','suspended'), txt VARCHAR(20), n VARCHAR(20));
INSERT INTO raw_values VALUES (1,-9223372036854775808,18446744073709551615,123456789012345678901234567890.4500,0x00ff0180,'2024-02-29 12:34:56.123456','0000-00-00','0000-00-00 00:00:00','-26:03:04.123456','active','',NULL),(2,0,0,0,x'','2024-01-01','2024-01-01','2024-01-01','00:00:00','suspended','active',NULL);
CREATE USER 'dbadmin_writer'@'%' IDENTIFIED BY '${password}' REQUIRE SSL; GRANT SELECT, INSERT, UPDATE, DELETE ON dbadmin_m001_fixture.* TO 'dbadmin_writer'@'%';
CREATE USER 'dbadmin_reader'@'%' IDENTIFIED BY '${password}' REQUIRE SSL; GRANT SELECT ON dbadmin_m001_fixture.* TO 'dbadmin_reader'@'%';`);
    started = true;
    run('docker', [...args, 'up', '-d', engine], env);
    await waitForFixture();
    const binary = nativeEvaluation ? nativeBinary : packetEvaluation ? 'target/receive-limit/debug/examples/live_packet_limits' : 'target/debug/examples/lifecycle';
    const result = run(resolve(root, binary), nativeEvaluation ? ['--ignored','--exact','actual_database_commands_use_owned_tls_fixtures','--nocapture'] : [], env);
    const report = JSON.parse(nativeEvaluation ? result.split('\n').find(line=>line.startsWith('DBADMIN_NATIVE_RESULT '))?.slice('DBADMIN_NATIVE_RESULT '.length) || 'null' : result);
    if(!report)throw Error('Native fixture report missing');
    writeFileSync(join(output, `${nativeEvaluation ? 'native-' : packetEvaluation ? 'packet-' : ''}${engine}.json`), JSON.stringify(report, null, 2) + '\n');
    console.log(nativeEvaluation ? `${engine}: real database IPC checks passed` : packetEvaluation ? `${engine}: TLS packet-bound and certificate checks passed` : `${engine}: ${report.lifecycleSamples} TLS lifecycle samples passed`);
    if (!packetEvaluation && !nativeEvaluation) {
      run('docker', [...args, 'stop', engine], env);
      writeFileSync(join(exposed, 'server.pem'), readFileSync(join(tls, 'expired-server.pem')));
      run('docker', [...args, 'up', '-d', '--force-recreate', engine], env);
      await waitForFixture({ expired: true });
      const expired = JSON.parse(run(resolve(root, binary), ['--expired-certificate'], env));
      writeFileSync(join(output, `expired-${engine}.json`), JSON.stringify(expired, null, 2) + '\n');
      console.log(`${engine}: expired-certificate rejection passed`);
    }

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
      try {
        rmSync(temp, { recursive: true, force: true });
      } finally {
        cleanup.temporaryRemoved = !existsSync(temp);
        writeFileSync(join(output, `${nativeEvaluation ? 'native-' : packetEvaluation ? 'packet-' : ''}cleanup-${engine}.json`), JSON.stringify(cleanup, null, 2) + '\n');
      }
    }
  }
}
