// Owned PostgreSQL 17.6 TLS fixture for the native IPC adapter. Mirrors the MySQL/MariaDB runner's
// isolation rules: local Docker only, random project/port/credentials/marker, generated v3 CAs,
// no inherited PG*/database settings, and verified removal of every owned resource.
import { spawn, spawnSync } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir, arch, release, cpus, totalmem } from 'node:os';
import { resolve, join } from 'node:path';
import { fixtureEnvironment, assertLocalFixtureDocker } from './fixture-environment.mjs';
import { parseMaximumRss } from './resource-report.mjs';
const root = resolve(import.meta.dirname, '..');
const base = fixtureEnvironment(process.env);
// `--execution` runs the M006 read-write harness (examples/execution.rs) instead of IPC checks.
const executionEvaluation = process.argv[2] === '--execution';
// `--resource-profile` runs the shared 100,000-row resource profile (examples/resources.rs).
const resourceEvaluation = process.argv[2] === '--resource-profile';
if (process.argv.length > (executionEvaluation || resourceEvaluation ? 3 : 2)) throw Error('Unknown fixture option');
if (resourceEvaluation && !['darwin', 'linux'].includes(process.platform)) throw Error('Resource profiling requires macOS or Linux');
let binary;
function run(command, args, env = base) {
  // The execution harness waits for a real lease idle expiry and runs 200+ relayed lease cycles.
  const result = spawnSync(command, args, { cwd: root, env, encoding: 'utf8', timeout: executionEvaluation ? 1_800_000 : 240_000 });
  if (result.status !== 0 || result.error) {
    // The fixture test emits only static assertions and synthetic values; still redact the password.
    const message = command === binary && env.STONEQL_FIXTURE_PASSWORD
      ? `${result.stdout}\n${result.stderr}`.replaceAll(env.STONEQL_FIXTURE_PASSWORD, '[redacted]')
      : `${command} fixture operation failed; credentials and raw errors withheld`;
    throw Error(message);
  }
  return result.stdout.trim();
}
assertLocalFixtureDocker(base, context => run('docker', ['context', 'inspect', ...(context ? [context] : []), '--format', '{{.Endpoints.docker.Host}}']));
if (executionEvaluation || resourceEvaluation) {
  const example = executionEvaluation ? 'execution' : 'resources';
  run('cargo', ['build', '--locked', '-p', 'stoneql-runtime', '--example', example]);
  binary = resolve(root, `target/debug/examples/${example}`);
} else {
  const artifacts = run('cargo', ['test', '--locked', '-p', 'stoneql-desktop', '--features', 'custom-protocol', '--test', 'live_postgres', '--no-run', '--message-format=json']);
  binary = artifacts.split('\n').map(line => JSON.parse(line)).find(message => message.reason === 'compiler-artifact' && message.target.name === 'live_postgres' && message.executable)?.executable;
}
if (!binary) throw Error('PostgreSQL fixture test binary missing');
const output = resolve(root, executionEvaluation ? 'artifacts/m006' : 'artifacts/m004');
const mode = executionEvaluation ? 'execution' : resourceEvaluation ? 'resource' : 'native';
mkdirSync(output, { recursive: true });
const temp = mkdtempSync(join(tmpdir(), 'stoneql-postgres-'));
const tls = join(temp, 'tls');
const init = join(temp, 'init');
const exposed = join(temp, 'server-tls');
mkdirSync(tls); mkdirSync(init); mkdirSync(exposed);
const project = `stoneql-m004-${randomBytes(6).toString('hex')}`;
const password = randomBytes(24).toString('hex');
const marker = randomUUID();
const env = { ...base, STONEQL_FIXTURE_PASSWORD: password, STONEQL_FIXTURE_INIT: init, STONEQL_SPIKE_FIXTURE_ID: marker };
const override = join(temp, 'compose.json');
const args = ['compose', '-f', resolve(root, 'experiments/sqlx-spike/fixtures/compose.yml'), '-f', override, '-p', project];
let started = false;
let relay;
const cleanup = { attempted: false, remainingContainers: null, remainingNetworks: null, temporaryRemoved: false };
try {
  writeFileSync(join(tls, 'root.cnf'), `[req]
distinguished_name = dn
x509_extensions = v3_ca
[dn]
CN = StoneQL disposable CA
[v3_ca]
basicConstraints = critical,CA:true
keyUsage = critical,keyCertSign,cRLSign
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always
`);
  for (const name of ['ca', 'bad-ca']) run('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(tls, `${name}-key.pem`), '-out', join(tls, `${name}.pem`), '-days', '1', '-config', join(tls, 'root.cnf'), '-extensions', 'v3_ca', '-subj', `/CN=StoneQL disposable ${name}`]);
  run('openssl', ['req', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(tls, 'server-key.pem'), '-out', join(tls, 'server.csr'), '-subj', '/CN=stoneql-fixture']);
  writeFileSync(join(tls, 'extensions.cnf'), 'subjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n');
  run('openssl', ['x509', '-req', '-in', join(tls, 'server.csr'), '-CA', join(tls, 'ca.pem'), '-CAkey', join(tls, 'ca-key.pem'), '-CAcreateserial', '-out', join(tls, 'server.pem'), '-days', '1', '-extfile', join(tls, 'extensions.cnf')]);
  // Only server material is mounted. PostgreSQL requires a private key owned by its user, so the
  // container copies it into a private directory before starting the official entrypoint.
  for (const name of ['server.pem', 'server-key.pem']) writeFileSync(join(exposed, name), readFileSync(join(tls, name)), { mode: 0o644 });
  // Local socket trust serves only the image's init/readiness tooling inside the container.
  writeFileSync(join(exposed, 'pg_hba.conf'), 'local all all trust\nhostssl all all all scram-sha-256\nhostnossl all all all reject\n', { mode: 0o644 });
  const prepare = 'install -d -o postgres -m 700 /tmp/stoneql-tls && install -o postgres -m 600 /stoneql-tls/server-key.pem /tmp/stoneql-tls/ && install -o postgres -m 644 /stoneql-tls/server.pem /stoneql-tls/pg_hba.conf /tmp/stoneql-tls/ && exec docker-entrypoint.sh postgres -c ssl=on -c ssl_cert_file=/tmp/stoneql-tls/server.pem -c ssl_key_file=/tmp/stoneql-tls/server-key.pem -c hba_file=/tmp/stoneql-tls/pg_hba.conf -c ssl_min_protocol_version=TLSv1.2' +
    // Execution evidence reads the writer's statements from a csvlog inside the disposable
    // container (only stoneql_writer logs statements; see 002-execution.sql). The data
    // directory is tmpfs inside a 512 MiB cgroup: the default max_wal_size (1 GB, never reached
    // by a checkpoint during a run) let the bulk-cancel section's WAL fill the container's memory
    // until the kernel OOM-killed a backend (crash restart, every session dropped), so WAL is
    // bounded here like InnoDB's fixed-size redo log.
    (executionEvaluation ? ' -c logging_collector=on -c log_destination=csvlog -c log_directory=log -c log_filename=stoneql -c max_wal_size=64MB' : '');
  writeFileSync(override, JSON.stringify({ services: { postgresql: { volumes: [`${exposed}:/stoneql-tls:ro`], entrypoint: ['sh', '-c', prepare] } } }));
  writeFileSync(join(init, '001-fixture.sql'), `CREATE TABLE stoneql_fixture_marker(id text PRIMARY KEY); INSERT INTO stoneql_fixture_marker VALUES ('${marker}');
CREATE TYPE fixture_mood AS ENUM ('active', 'unknown label');
CREATE TABLE "Mixed ""Case"" values"(id int PRIMARY KEY, i8 bigint, n numeric(38,4), nn numeric, f8 double precision, b bytea, t text, d text DEFAULT 'fallback', e fixture_mood, ts timestamp(6), tz timestamptz, dt date, tm time, ttz timetz, j jsonb, u uuid, ip inet, flag boolean, "Quoted ""Column""" text);
INSERT INTO "Mixed ""Case"" values"(id,i8,n,nn,f8,b,t,e,ts,tz,dt,tm,ttz,j,u,ip,flag,"Quoted ""Column""") VALUES
 (1,-9223372036854775808,123456789012345678901234567890.4500,'NaN',0.1,'\\x00ff0180','','unknown label','2024-02-29 12:34:56.123456','2024-02-29 12:34:56.123456+05:30','0044-03-15 BC','24:00:00','12:34:56.5-07','{"b":1,"a":[1,2]}','a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11','192.168.0.1/24',true,'quoted identifier'),
 (2,9223372036854775807,-0.0001,'1e-20','Infinity','\\x',NULL,'active','infinity','-infinity','infinity','00:00:00','00:00:00+00','null',NULL,NULL,false,NULL);
UPDATE "Mixed ""Case"" values" SET d=NULL WHERE id=2;
CREATE TABLE big(n int); INSERT INTO big SELECT generate_series(1,2048);
CREATE TABLE trust_counter(id int PRIMARY KEY, touches int NOT NULL); INSERT INTO trust_counter VALUES (1,0);
CREATE FUNCTION trust_effect() RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path = public AS $$ BEGIN UPDATE trust_counter SET touches = touches + 1 WHERE id = 1; RETURN 1; END $$;
CREATE VIEW trust_view AS SELECT trust_effect() AS effect;
CREATE VIEW trust_nested_view AS SELECT effect FROM trust_view;
CREATE SEQUENCE trust_seq;
CREATE VIEW trust_sequence_view AS SELECT nextval('trust_seq') AS value;
CREATE VIEW trust_lock_view AS SELECT pg_try_advisory_lock(4242) AS acquired;
CREATE VIEW slow_view AS SELECT (SELECT 1 FROM pg_sleep(10)) AS slept;
CREATE ROLE stoneql_reader LOGIN PASSWORD '${password}';
GRANT CONNECT ON DATABASE stoneql_m001_fixture TO stoneql_reader;
GRANT USAGE ON SCHEMA public TO stoneql_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO stoneql_reader;
GRANT USAGE ON SEQUENCE trust_seq TO stoneql_reader;
`);
  // Created before 001's grants so the reader can SELECT it; 1 KiB payloads like the MySQL profile.
  if (resourceEvaluation) writeFileSync(join(init, '000-resource.sql'), `CREATE TABLE resource_rows(id int PRIMARY KEY, payload varchar(1024) NOT NULL);
INSERT INTO resource_rows SELECT n, repeat('r', 1024) FROM generate_series(1, 100000) AS n;
ANALYZE resource_rows;
`);
  if (executionEvaluation) writeFileSync(join(init, '002-execution.sql'), `CREATE ROLE stoneql_writer LOGIN PASSWORD '${password}';
GRANT CONNECT ON DATABASE stoneql_m001_fixture TO stoneql_writer;
GRANT USAGE, CREATE ON SCHEMA public TO stoneql_writer;
CREATE TABLE exec_parent(id int PRIMARY KEY); INSERT INTO exec_parent VALUES (1);
CREATE TABLE exec_item(id int PRIMARY KEY, parent int REFERENCES exec_parent(id));
CREATE TABLE exec_cycle(n int PRIMARY KEY);
CREATE TABLE exec_lock(id int PRIMARY KEY, v bigint NOT NULL); INSERT INTO exec_lock VALUES (1, 0);
CREATE TABLE exec_digit(n int PRIMARY KEY); INSERT INTO exec_digit SELECT generate_series(0, 9);
CREATE TABLE exec_bulk(n int NOT NULL);
CREATE PROCEDURE exec_proc(INOUT result text) LANGUAGE plpgsql AS $$ BEGIN result := 'procedure'; END $$;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO stoneql_writer;
GRANT INSERT, UPDATE, DELETE ON exec_parent, exec_item, exec_cycle, exec_lock, exec_bulk TO stoneql_writer;
ALTER ROLE stoneql_writer SET log_statement = 'all';
GRANT EXECUTE ON PROCEDURE exec_proc(text) TO stoneql_writer;
`);
  started = true;
  run('docker', [...args, 'up', '-d', 'postgresql'], env);
  let ready = false;
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    // The marker can be visible while the entrypoint's socket-only init server runs, so also
    // require the final TCP listener.
    const marked = spawnSync('docker', [...args, 'exec', '-T', 'postgresql', 'psql', '-U', 'stoneql_fixture', '-d', 'stoneql_m001_fixture', '-tAc', 'SELECT id FROM stoneql_fixture_marker'], { env, encoding: 'utf8', timeout: 10_000 });
    if (marked.status === 0 && marked.stdout.trim() === marker) {
      const tcp = spawnSync('docker', [...args, 'exec', '-T', 'postgresql', 'pg_isready', '-h', '127.0.0.1', '-p', '5432'], { env, encoding: 'utf8', timeout: 10_000 });
      if (tcp.status === 0) { ready = true; break; }
    }
    const state = run('docker', [...args, 'ps', '-a', '--format', 'json', 'postgresql'], env);
    if (state.split('\n').filter(Boolean).some(line => JSON.parse(line).State === 'exited')) break;
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  if (!ready) throw Error('postgresql: disposable TLS fixture not ready');
  const port = run('docker', [...args, 'port', 'postgresql', '5432'], env);
  if (!/^127\.0\.0\.1:\d+$/.test(port)) throw Error('Loopback fixture required');
  Object.assign(env, { STONEQL_LIFECYCLE_PORT: port.split(':')[1], STONEQL_LIFECYCLE_CA: join(tls, 'ca.pem'), STONEQL_LIFECYCLE_BAD_CA: join(tls, 'bad-ca.pem') });
  const image = run('docker', ['inspect', '--format', '{{.Image}}', run('docker', [...args, 'ps', '-q', 'postgresql'], env)]);
  if (executionEvaluation) {
    // Test-owned loopback relay for lost-reply and failed-cancel injection (examples/execution.rs).
    relay = spawn(process.execPath, [resolve(root, 'tools/tcp-fault-relay.mjs'), env.STONEQL_LIFECYCLE_PORT], { cwd: root, stdio: ['ignore', 'pipe', 'inherit'] });
    const ports = await new Promise((done, fail) => {
      let text = '';
      relay.stdout.on('data', chunk => { text += chunk; if (text.includes('\n')) done(JSON.parse(text.split('\n')[0])); });
      relay.on('exit', () => fail(Error('Fault relay exited')));
    });
    Object.assign(env, { STONEQL_RELAY_PORT: String(ports.relayPort), STONEQL_RELAY_CONTROL: String(ports.controlPort) });
    const report = JSON.parse(run(binary, [], { ...env, STONEQL_LIFECYCLE_ENGINE: 'postgresql' }));
    report.image = image;
    writeFileSync(join(output, 'execution-postgresql.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(`postgresql: read-write execution checks passed (cancel p95 ${report.cancel.p95Ms.toFixed(1)} ms)`);
  } else if (resourceEvaluation) {
    const measured = spawnSync('/usr/bin/time', [process.platform === 'darwin' ? '-l' : '-v', binary], { cwd: root, env: { ...env, STONEQL_LIFECYCLE_ENGINE: 'postgresql' }, encoding: 'utf8', timeout: 240_000 });
    if (measured.status !== 0 || measured.error) throw Error('Resource fixture execution failed; credentials and raw errors withheld');
    const report = JSON.parse(measured.stdout.trim());
    report.processMaxRssBytes = parseMaximumRss(measured.stderr, process.platform);
    report.machine = { platform: process.platform, arch: arch(), osRelease: release(), cpu: cpus()[0]?.model, logicalCpus: cpus().length, totalMemoryBytes: totalmem() };
    report.image = image;
    writeFileSync(join(output, 'resource-postgresql.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(`postgresql: resource and metadata profile passed (p95 ${(report.p95Us / 1000).toFixed(3)} ms, max RSS ${(report.processMaxRssBytes / 1048576).toFixed(3)} MiB)`);
  } else {
    const result = run(binary, ['--ignored', '--exact', 'postgres_commands_use_owned_tls_fixture', '--nocapture'], env);
    const report = JSON.parse(result.split('\n').find(line => line.startsWith('STONEQL_NATIVE_RESULT '))?.slice('STONEQL_NATIVE_RESULT '.length) || 'null');
    if (!report) throw Error('Fixture report missing');
    report.image = image;
    writeFileSync(join(output, 'native-postgresql.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(`postgresql: real database IPC checks passed (${report.lifecycleSamples} TLS lifecycle samples)`);
  }
} finally {
  relay?.kill();
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
      writeFileSync(join(output, `${mode}-cleanup-postgresql.json`), JSON.stringify(cleanup, null, 2) + '\n');
    }
  }
}
