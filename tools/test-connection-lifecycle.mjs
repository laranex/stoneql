import { spawn, spawnSync } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { parseMaximumRss } from './resource-report.mjs';
import { cpus, totalmem, release, arch } from 'node:os';
import { fixtureEnvironment, assertLocalFixtureDocker } from './fixture-environment.mjs';
import { runRemotely, endRemote } from './remote-fixture.mjs';
const root = resolve(import.meta.dirname, '..');
const base = fixtureEnvironment(process.env);
const packetEvaluation = process.argv.slice(2).includes('--packet-limit');
const resourceEvaluation = process.argv.slice(2).includes('--resource-profile');
if(resourceEvaluation && !['darwin','linux'].includes(process.platform))throw Error('Resource profiling requires macOS or Linux');
const nativeEvaluation = process.argv.slice(2).includes('--native-ipc');
// M006 read-write execution harness (examples/execution.rs): jobs, cancellation, leases, faults.
const executionEvaluation = process.argv.slice(2).includes('--execution');
// M007 SQL import benchmark (examples/import_bench.rs, release build): one case per process
// under /usr/bin/time for peak RSS; results feed the A20 limit sign-off.
const importBench = process.argv.slice(2).includes('--import-bench');
// `--native-ipc --remote`: the native test binary runs on another machine (tools/remote-fixture.mjs).
const remote = process.argv.slice(2).includes('--remote');
if(remote&&!nativeEvaluation)throw Error('--remote requires --native-ipc');
if([packetEvaluation,nativeEvaluation,resourceEvaluation,executionEvaluation,importBench].filter(Boolean).length>1)throw Error('Choose a single fixture mode');
let nativeBinary;
if (process.argv.slice(2).some(argument => !['--packet-limit','--native-ipc','--resource-profile','--execution','--import-bench','--remote'].includes(argument))) throw Error('Unknown fixture option');
function run(command, args, env = base) {
  // The execution harness waits for a real lease idle expiry and 100 cancel/fault cycles.
  const result = spawnSync(command, args, { cwd: root, env, encoding: 'utf8', timeout: executionEvaluation ? 1_800_000 : 240_000 });
  if (result.status !== 0 || result.error) {
    // The fixture binary emits only static errors/codes and synthetic assertion values.
    const message = (nativeEvaluation && command===nativeBinary || command.endsWith('/examples/lifecycle') || command.endsWith('/examples/execution') || command.endsWith('/examples/live_packet_limits'))
      ? result.stderr.replaceAll(env.STONEQL_FIXTURE_PASSWORD, '[redacted]')
      : `${command} fixture operation failed; credentials and raw errors withheld`;
    throw Error(message);
  }
  return result.stdout.trim();
}
assertLocalFixtureDocker(base, context => run('docker', ['context', 'inspect', ...(context ? [context] : []), '--format', '{{.Endpoints.docker.Host}}']));
if(importBench){
  run('cargo',['build','--release','--locked','-p','stoneql-runtime','--example','import_bench']);
} else if(resourceEvaluation){
  run('cargo',['build','--locked','-p','stoneql-runtime','--example','resources']);
} else if(executionEvaluation){
  run('cargo',['build','--locked','-p','stoneql-runtime','--example','execution']);
} else if(remote){
  // The native test binary is built and run by the remote client.
} else if(nativeEvaluation){
  const artifacts=run('cargo',['test','--locked','-p','stoneql-desktop','--features','custom-protocol','--test','live_database','--no-run','--message-format=json']);
  nativeBinary=artifacts.split('\n').map(line=>JSON.parse(line)).find(message=>message.reason==='compiler-artifact'&&message.target.name==='live_database'&&message.executable)?.executable;
  if(!nativeBinary)throw Error('Native fixture test binary missing');
} else if (packetEvaluation) {
  run('cargo', ['build', '--locked', '--manifest-path', 'experiments/sqlx-receive-limit/Cargo.toml', '--target-dir', 'target/receive-limit', '--example', 'live_packet_limits']);
} else {
  run('cargo', ['build', '--locked', '-p', 'stoneql-runtime', '--example', 'lifecycle']);
}
const output = resolve(root, executionEvaluation ? 'artifacts/m006' : importBench ? 'artifacts/m007' : 'artifacts/m003');
mkdirSync(output, { recursive: true });
for (const engine of (importBench && process.env.STONEQL_BENCH_ENGINES?.split(',')) || ['mysql', 'mariadb']) {
  const temp = mkdtempSync(join(tmpdir(), 'stoneql-lifecycle-'));
  const tls = join(temp, 'tls');
  const init = join(temp, 'init');
  mkdirSync(tls);
  mkdirSync(init);
  const project = `stoneql-m003-${randomBytes(6).toString('hex')}`;
  const password = randomBytes(24).toString('hex');
  const marker = randomUUID();
  const env = { ...base, STONEQL_FIXTURE_PASSWORD: password, STONEQL_FIXTURE_INIT: init, STONEQL_SPIKE_FIXTURE_ID: marker, STONEQL_LIFECYCLE_ENGINE: engine };
  const override = join(temp, 'compose.json');
  const args = ['compose', '-f', resolve(root, 'experiments/sqlx-spike/fixtures/compose.yml'), '-f', override, '-p', project];
  async function waitForFixture({ expired = false } = {}) {
    let ready = false;
    let lastError = '';
    const deadline = Date.now() + 120_000;
    const cli = engine === 'mysql'
      ? 'MYSQL_PWD="$MYSQL_ROOT_PASSWORD" exec mysql -uroot -N -e "SELECT id FROM stoneql_m001_fixture.stoneql_fixture_marker"'
      // MariaDB's CLI enables TLS even on its Unix socket. The expired phase's local
      // readiness observer is socket-only; StoneQL still uses verified TLS over TCP.
      : expired
        ? 'MYSQL_PWD="$MARIADB_ROOT_PASSWORD" exec mariadb --protocol=SOCKET --skip-ssl -uroot -N -e "SELECT id FROM stoneql_m001_fixture.stoneql_fixture_marker"'
        : 'MYSQL_PWD="$MARIADB_ROOT_PASSWORD" exec mariadb -uroot -N -e "SELECT id FROM stoneql_m001_fixture.stoneql_fixture_marker"';
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
    Object.assign(env, { STONEQL_LIFECYCLE_PORT: port.split(':')[1], STONEQL_LIFECYCLE_CA: join(tls, 'ca.pem'), STONEQL_LIFECYCLE_BAD_CA: join(tls, 'bad-ca.pem') });
  }
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
    run('openssl', ['req', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(tls, 'server-key.pem'), '-out', join(tls, 'server.csr'), '-subj', '/CN=localhost']);
    writeFileSync(join(tls, 'extensions.cnf'), 'subjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n');
    run('openssl', ['x509', '-req', '-in', join(tls, 'server.csr'), '-CA', join(tls, 'ca.pem'), '-CAkey', join(tls, 'ca-key.pem'), '-CAcreateserial', '-out', join(tls, 'server.pem'), '-days', '1', '-extfile', join(tls, 'extensions.cnf')]);
    if (!packetEvaluation && !nativeEvaluation && !resourceEvaluation && !executionEvaluation) {
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
    // The 100,000-row profile's data, binlog and redo live on tmpfs, which counts against the
    // memory limit. Docker Desktop lets tmpfs pages swap (1g + swap); a host without swap OOM-kills
    // the 1g server during init, so this profile gets the same 2g total as memory, without swap.
    const memory = resourceEvaluation ? { mem_limit: '2g', memswap_limit: '2g' } : {};
    writeFileSync(override, JSON.stringify({ services: { [engine]: { ...memory, volumes: [`${exposed}:/stoneql-tls:ro`], command: ['--ssl-ca=/stoneql-tls/ca.pem', '--ssl-cert=/stoneql-tls/server.pem', '--ssl-key=/stoneql-tls/server-key.pem', '--require-secure-transport=ON'] } } }));
    writeFileSync(join(init, '001-fixture.sql'), `CREATE TABLE stoneql_fixture_marker(id VARCHAR(64) PRIMARY KEY); INSERT INTO stoneql_fixture_marker VALUES ('${marker}');
SET SESSION sql_mode='';
CREATE TABLE raw_large(payload LONGBLOB);
INSERT INTO raw_large VALUES(REPEAT('x',3145728));
CREATE TABLE raw_values(id INT PRIMARY KEY, s BIGINT, u BIGINT UNSIGNED, d DECIMAL(38,4), b VARBINARY(4), dt DATETIME(6), zd DATE, zt DATETIME, tm TIME(6), e ENUM('active','suspended'), txt VARCHAR(20), n VARCHAR(20));
INSERT INTO raw_values VALUES (1,-9223372036854775808,18446744073709551615,123456789012345678901234567890.4500,0x00ff0180,'2024-02-29 12:34:56.123456','0000-00-00','0000-00-00 00:00:00','-26:03:04.123456','active','',NULL),(2,0,0,0,x'','2024-01-01','2024-01-01','2024-01-01','00:00:00','suspended','active',NULL);
CREATE TABLE export_values(id BIGINT UNSIGNED PRIMARY KEY, t VARCHAR(64) NULL, d DECIMAL(38,4) NULL, dt DATETIME(6) NULL, zd DATE NULL, e ENUM('a','b c') NULL, s SET('x','y') NULL, j JSON NULL, f DOUBLE NULL) CHARACTER SET utf8mb4;
INSERT INTO export_values VALUES (18446744073709551615, CONCAT('quote " comma , newline', CHAR(10), 'end'), 123456789012345678901234567890.4500, '2024-02-29 12:34:56.123456', '0000-00-00', 'b c', 'x,y', '{"a": [1, "é"]}', 0.1), (1, '', NULL, NULL, NULL, NULL, '', NULL, -0.0), (2, 'NULL', -0.0001, '1000-01-01 00:00:00', '2024-01-01', 'a', 'x', 'null', 1e300), (3, NULL, 0, '2024-01-01', '2024-01-01', 'a', NULL, '[]', NULL), (4, CONCAT('tab', CHAR(9), 'back', CHAR(92), 'slash é ✓ 😀'), 1, '2024-01-01', '2024-01-01', 'a', 'y', '"s"', -1.5);
CREATE TABLE fk_child(id INT PRIMARY KEY, parent INT, KEY fk_parent_idx (parent), CONSTRAINT fk_child_parent FOREIGN KEY (parent) REFERENCES raw_values(id) ON DELETE CASCADE) ENGINE=InnoDB;
CREATE TABLE trust_innodb(id INT PRIMARY KEY, touches INT NOT NULL) ENGINE=InnoDB;
CREATE TABLE trust_myisam(id INT PRIMARY KEY, touches INT NOT NULL) ENGINE=MyISAM;
INSERT INTO trust_innodb VALUES(1,0); INSERT INTO trust_myisam VALUES(1,0);
DELIMITER //
CREATE FUNCTION trust_effect_innodb() RETURNS INT DETERMINISTIC MODIFIES SQL DATA SQL SECURITY DEFINER
BEGIN UPDATE trust_innodb SET touches=touches+1 WHERE id=1; RETURN 1; END//
CREATE FUNCTION trust_effect_myisam() RETURNS INT DETERMINISTIC MODIFIES SQL DATA SQL SECURITY DEFINER
BEGIN UPDATE trust_myisam SET touches=touches+1 WHERE id=1; RETURN 1; END//
DELIMITER ;
CREATE SQL SECURITY DEFINER VIEW trust_view_innodb AS SELECT trust_effect_innodb() AS effect;
CREATE SQL SECURITY DEFINER VIEW trust_view_myisam AS SELECT trust_effect_myisam() AS effect;
CREATE SQL SECURITY DEFINER VIEW trust_nested_view AS SELECT effect FROM trust_view_innodb;
CREATE SQL SECURITY INVOKER VIEW trust_safe_view AS SELECT id,txt FROM raw_values;
CREATE SQL SECURITY DEFINER VIEW trust_lock_view AS SELECT GET_LOCK('stoneql_${marker}',0) AS acquired;
CREATE USER 'stoneql_writer'@'%' IDENTIFIED BY '${password}' REQUIRE SSL; GRANT SELECT, INSERT, UPDATE, DELETE ON stoneql_m001_fixture.* TO 'stoneql_writer'@'%';
CREATE USER 'stoneql_reader'@'%' IDENTIFIED BY '${password}' REQUIRE SSL; GRANT SELECT ON stoneql_m001_fixture.* TO 'stoneql_reader'@'%';
CREATE USER 'stoneql_plain'@'%' IDENTIFIED BY '${password}'; GRANT SELECT ON stoneql_m001_fixture.* TO 'stoneql_plain'@'%';
CREATE TABLE switch_marker(id INT PRIMARY KEY, label VARCHAR(20)); INSERT INTO switch_marker VALUES (1, 'default');
CREATE DATABASE stoneql_switch_b; CREATE TABLE stoneql_switch_b.switch_marker(id INT PRIMARY KEY, label VARCHAR(20)); INSERT INTO stoneql_switch_b.switch_marker VALUES (1, 'other');
GRANT SELECT, INSERT, DELETE ON stoneql_switch_b.* TO 'stoneql_writer'@'%';
CREATE USER 'stoneql_admin'@'%' IDENTIFIED BY '${password}' REQUIRE SSL; GRANT ALL PRIVILEGES ON *.* TO 'stoneql_admin'@'%';`);
    if(executionEvaluation||importBench){
      writeFileSync(join(init,'003-execution.sql'),`GRANT CREATE, DROP, ALTER, INDEX, REFERENCES, CREATE TEMPORARY TABLES, EXECUTE, CREATE ROUTINE, ALTER ROUTINE ON stoneql_m001_fixture.* TO 'stoneql_writer'@'%';
CREATE DATABASE stoneql_export_whole; GRANT CREATE, DROP, ALTER, INDEX, REFERENCES, SELECT, INSERT, UPDATE, DELETE ON stoneql_export_whole.* TO 'stoneql_writer'@'%';
CREATE TABLE exec_parent(id INT PRIMARY KEY) ENGINE=InnoDB; INSERT INTO exec_parent VALUES (1);
CREATE TABLE exec_item(id INT PRIMARY KEY, parent INT, FOREIGN KEY (parent) REFERENCES exec_parent(id)) ENGINE=InnoDB;
CREATE TABLE exec_cycle(n INT PRIMARY KEY) ENGINE=InnoDB;
CREATE TABLE exec_lock(id INT PRIMARY KEY, v INT NOT NULL) ENGINE=InnoDB; INSERT INTO exec_lock VALUES (1, 0);
CREATE TABLE exec_digit(n INT PRIMARY KEY) ENGINE=InnoDB; INSERT INTO exec_digit VALUES (0),(1),(2),(3),(4),(5),(6),(7),(8),(9);
CREATE TABLE exec_bulk(n INT NOT NULL) ENGINE=InnoDB;
CREATE TABLE exec_text(id INT PRIMARY KEY, label VARCHAR(40), amount DECIMAL(10,2)) ENGINE=InnoDB CHARACTER SET utf8mb4;
DELIMITER //
CREATE PROCEDURE exec_multi() SQL SECURITY INVOKER BEGIN SELECT 1 AS a; SELECT 'two' AS b, 2 AS c; END//
CREATE PROCEDURE exec_autocommit_off() SQL SECURITY INVOKER BEGIN SET autocommit = 0; END//
CREATE PROCEDURE exec_start_txn() SQL SECURITY INVOKER BEGIN START TRANSACTION; END//
DELIMITER ;`);
    }
    if(resourceEvaluation){
      writeFileSync(join(init,'002-resource.sql'),`CREATE TABLE resource_digit(n INT PRIMARY KEY); INSERT INTO resource_digit VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9);
CREATE TABLE resource_rows(id INT PRIMARY KEY,payload VARCHAR(1024) NOT NULL) ENGINE=InnoDB;
INSERT INTO resource_rows SELECT 1+a.n+10*b.n+100*c.n+1000*d.n+10000*e.n,REPEAT('r',1024) FROM resource_digit a CROSS JOIN resource_digit b CROSS JOIN resource_digit c CROSS JOIN resource_digit d CROSS JOIN resource_digit e;`);
    }
    started = true;
    run('docker', [...args, 'up', '-d', engine], env);
    await waitForFixture();
    if (executionEvaluation) {
      // Test-owned loopback relay for lost-reply and failed-KILL injection (examples/execution.rs).
      relay = spawn(process.execPath, [resolve(root, 'tools/tcp-fault-relay.mjs'), env.STONEQL_LIFECYCLE_PORT], { cwd: root, stdio: ['ignore', 'pipe', 'inherit'] });
      const ports = await new Promise((done, fail) => {
        let text = '';
        relay.stdout.on('data', chunk => { text += chunk; if (text.includes('\n')) done(JSON.parse(text.split('\n')[0])); });
        relay.on('exit', () => fail(Error('Fault relay exited')));
      });
      Object.assign(env, { STONEQL_RELAY_PORT: String(ports.relayPort), STONEQL_RELAY_CONTROL: String(ports.controlPort) });
    }
    if (importBench) {
      const cases = process.env.STONEQL_BENCH_CASES?.split(',') ?? ['exec-1k-4m', 'exec-1k-4m-nojournal', 'exec-64k-64m', 'exec-1m-256m', 'stmt-4m', 'stmt-8m', 'stmt-15m', 'stmt-16m', 'validate-1k-1g', 'exec-4m-1g'];
      const bench = { engine, machine: { platform: process.platform, arch: arch(), osRelease: release(), cpu: cpus()[0]?.model, logicalCpus: cpus().length, totalMemoryBytes: totalmem() }, cases: [] };
      for (const name of cases) {
        const measured = spawnSync('/usr/bin/time', [process.platform === 'darwin' ? '-l' : '-v', resolve(root, 'target/release/examples/import_bench'), name], { cwd: root, env: { ...env, STONEQL_BENCH_DIR: temp }, encoding: 'utf8', timeout: 3_600_000 });
        // The benchmark prints only static errors and synthetic values; the password is redacted.
        // A failing case is recorded and the matrix continues; the run fails at the end.
        if (measured.status !== 0 || measured.error) {
          // Server-side evidence for the failure: container state (OOM kill, exit, restarts) and its last log lines.
          const id = run('docker', [...args, 'ps', '-aq', engine], env);
          const state = id ? run('docker', ['inspect', '--format', 'oomKilled={{.State.OOMKilled}} running={{.State.Running}} exitCode={{.State.ExitCode}} restarts={{.RestartCount}}', id], env) : 'container missing';
          const logs = id ? spawnSync('docker', ['logs', '--tail', '15', id], { encoding: 'utf8' }) : null;
          bench.cases.push({ case: name, error: (measured.stderr || '').replaceAll(password, '[redacted]').split('\n').filter((line) => line && !/^\s*\d+\s+[a-z]/.test(line)).slice(-4).join(' '), serverState: state, serverLog: `${logs?.stdout ?? ''}${logs?.stderr ?? ''}`.replaceAll(password, '[redacted]').split('\n').filter(Boolean).slice(-15) });
          console.log(`${engine} ${name}: FAILED ${bench.cases.at(-1).error} :: ${state}`);
          continue;
        }
        const result = JSON.parse(measured.stdout.trim().split('\n').at(-1));
        result.processMaxRssBytes = parseMaximumRss(measured.stderr, process.platform);
        bench.cases.push(result);
        console.log(`${engine} ${name}: ${JSON.stringify({ outcome: result.outcome, refused: result.refused, fileMiB: +(result.fileBytes / 1048576).toFixed(1), statements: result.validatedStatements, readMs: Math.round(result.readMs), validateMs: Math.round(result.validateMs), executeMs: result.executeMs && Math.round(result.executeMs), mibPerSecond: result.mibPerSecond && +result.mibPerSecond.toFixed(1), peakRssMiB: Math.round(result.processMaxRssBytes / 1048576), failedSqlState: result.failedSqlState, unknownStatement: result.unknownStatement })}`);
      }
      writeFileSync(join(output, `import-bench-${engine}.json`), JSON.stringify(bench, null, 2) + '\n');
      if (bench.cases.some((result) => result.error)) throw Error(`${engine}: benchmark cases failed`);
    } else {
    // M007: the native test exports here; the files are restored below with the server's client.
    const exports = join(temp, 'exports');
    if (nativeEvaluation) { mkdirSync(exports); env.STONEQL_EXPORT_DIR = exports; }
    const binary = executionEvaluation ? 'target/debug/examples/execution' : resourceEvaluation ? 'target/debug/examples/resources' : nativeEvaluation ? nativeBinary : packetEvaluation ? 'target/receive-limit/debug/examples/live_packet_limits' : 'target/debug/examples/lifecycle';
    const measured = resourceEvaluation ? spawnSync('/usr/bin/time',[process.platform==='darwin'?'-l':'-v',resolve(root,binary)],{cwd:root,env,encoding:'utf8',timeout:240000}) : null;
    if(measured && (measured.status!==0||measured.error))throw Error('Resource fixture execution failed; credentials and raw errors withheld');
    const result = measured ? measured.stdout.trim() : remote ? await runRemotely(env, engine) : run(resolve(root, binary), nativeEvaluation ? ['--ignored','--exact','actual_database_commands_use_owned_tls_fixtures','--nocapture'] : [], env);
    const report = JSON.parse(nativeEvaluation ? result.split('\n').find(line=>line.startsWith('STONEQL_NATIVE_RESULT '))?.slice('STONEQL_NATIVE_RESULT '.length) || 'null' : result);
    if(!report)throw Error('Fixture report missing');
    if(measured){report.processMaxRssBytes=parseMaximumRss(measured.stderr,process.platform);report.machine={platform:process.platform,arch:arch(),osRelease:release(),cpu:cpus()[0]?.model,logicalCpus:cpus().length,totalMemoryBytes:totalmem()};}
    if (nativeEvaluation) {
      // Independent read-back (G6): load both exports with the server's own client into scratch
      // databases and compare every column NULL-safely and byte-exactly with the source rows.
      const client = engine === 'mysql' ? 'mysql' : 'mariadb', password = engine === 'mysql' ? 'MYSQL_ROOT_PASSWORD' : 'MARIADB_ROOT_PASSWORD';
      const dir = engine === 'mysql' ? '/var/lib/mysql-files' : '/tmp';
      // Exports are written 0600; the copies inside the disposable container are made readable
      // for the server process (LOAD DATA reads the file server-side).
      for (const name of ['export_values.sql', 'export_values.csv']) {
        run('docker', [...args, 'cp', join(exports, name), `${engine}:${dir}/${name}`], env);
        run('docker', [...args, 'exec', '-T', '-u', 'root', engine, 'chmod', '0644', `${dir}/${name}`], env);
      }
      const columns = ['t', 'd', 'dt', 'zd', 'e', 's', 'j', 'f'];
      const same = (db) => `SELECT (SELECT COUNT(*) FROM ${db}.export_values), (SELECT COUNT(*) FROM stoneql_m001_fixture.export_values a JOIN ${db}.export_values b USING (id) WHERE ${columns.map((c) => `BINARY a.${c} <=> BINARY b.${c}`).join(' AND ')});`;
      writeFileSync(join(temp, 'restore.sql'), [
        "SET SESSION sql_mode = '';",
        `CREATE DATABASE restore_sql; USE restore_sql; SOURCE ${dir}/export_values.sql;`,
        'CREATE DATABASE restore_csv; CREATE TABLE restore_csv.export_values LIKE stoneql_m001_fixture.export_values;',
        `LOAD DATA INFILE '${dir}/export_values.csv' INTO TABLE restore_csv.export_values CHARACTER SET utf8mb4 FIELDS TERMINATED BY ',' ENCLOSED BY '"' ESCAPED BY '' LINES TERMINATED BY '\\n' IGNORE 1 LINES;`,
        same('restore_sql'),
        same('restore_csv'),
      ].join('\n') + '\n');
      run('docker', [...args, 'cp', join(temp, 'restore.sql'), `${engine}:/tmp/restore.sql`], env);
      const command = 'MYSQL_PWD="$' + password + '" exec ' + client + ' -uroot -N < /tmp/restore.sql';
      const loaded = spawnSync('docker', [...args, 'exec', '-T', engine, 'sh', '-c', command], { env, encoding: 'utf8', timeout: 120_000 });
      // Only the client's own ERROR lines are shown (SQL errors; never credentials).
      if (loaded.status !== 0) throw Error(`${engine}: restore failed: ${loaded.stderr.split('\n').filter((line) => line.startsWith('ERROR')).join(' ')}`);
      const restored = loaded.stdout.trim().split('\n').map((line) => line.trim().split(/\s+/));
      report.exportRestore = { sql: restored[0], csv: restored[1] };
      if (JSON.stringify(restored) !== JSON.stringify([['5', '5'], ['5', '5']])) throw Error(`${engine}: exported files did not restore exactly: ${JSON.stringify(restored)}`);
    }
    writeFileSync(join(output, `${executionEvaluation ? 'execution-' : resourceEvaluation ? 'resource-' : nativeEvaluation ? 'native-' : packetEvaluation ? 'packet-' : ''}${engine}.json`), JSON.stringify(report, null, 2) + '\n');
    console.log(executionEvaluation ? `${engine}: read-write execution checks passed (cancel p95 ${report.cancel.p95Ms.toFixed(1)} ms)` : resourceEvaluation ? `${engine}: resource and metadata profile passed` : nativeEvaluation ? `${engine}: real database IPC checks passed` : packetEvaluation ? `${engine}: TLS packet-bound and certificate checks passed` : `${engine}: ${report.lifecycleSamples} TLS lifecycle samples passed`);
    if (!packetEvaluation && !nativeEvaluation && !resourceEvaluation && !executionEvaluation && !importBench) {
      run('docker', [...args, 'stop', engine], env);
      writeFileSync(join(exposed, 'server.pem'), readFileSync(join(tls, 'expired-server.pem')));
      run('docker', [...args, 'up', '-d', '--force-recreate', engine], env);
      await waitForFixture({ expired: true });
      const expired = JSON.parse(run(resolve(root, binary), ['--expired-certificate'], env));
      writeFileSync(join(output, `expired-${engine}.json`), JSON.stringify(expired, null, 2) + '\n');
      console.log(`${engine}: expired-certificate rejection passed`);
    }
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
        writeFileSync(join(output, `${executionEvaluation ? 'execution-' : importBench ? 'import-bench-' : resourceEvaluation ? 'resource-' : nativeEvaluation ? 'native-' : packetEvaluation ? 'packet-' : ''}cleanup-${engine}.json`), JSON.stringify(cleanup, null, 2) + '\n');
      }
    }
  }
}
if (remote) endRemote();
