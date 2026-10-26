// Owned M005 SSH transport fixtures. Local Docker only; per run: random project, generated
// database CA/server certificate (original hostname in SAN), two OpenSSH servers with distinct
// generated ed25519 host keys, generated client keys, random passwords/passphrase and a private
// ssh-agent. The user's ~/.ssh, agent, known_hosts and keychain items are never read.
// Every owned container/network/agent/temporary directory is removed and verified.
import { spawn, spawnSync } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync, chmodSync, readdirSync } from 'node:fs';
import { connect } from 'node:net';
import { resolve, join } from 'node:path';
import { fixtureEnvironment, assertLocalFixtureDocker } from './fixture-environment.mjs';

const root = resolve(import.meta.dirname, '..');
const base = fixtureEnvironment(process.env);
const requested = process.argv.slice(2);
const engines = requested.length ? requested : ['mysql', 'mariadb', 'postgresql'];
if (engines.some(engine => !['mysql', 'mariadb', 'postgresql'].includes(engine))) throw Error('Unknown fixture engine');
const IMAGE = 'stoneql-ssh-fixture:m005';
const secrets = new Set();
function redact(text) {
  let output = String(text);
  for (const secret of secrets) output = output.replaceAll(secret, '[redacted]');
  return output;
}
function run(command, args, env = base, { input } = {}) {
  const result = spawnSync(command, args, { cwd: root, env, encoding: 'utf8', timeout: 300_000, input });
  if (result.status !== 0 || result.error) {
    // Test binaries print only synthetic values/codes; still redact every generated secret.
    throw Error(`${command} ${args[0] || ''} failed\n${redact(`${result.stdout}\n${result.stderr}`).slice(-6000)}`);
  }
  return result.stdout.trim();
}
assertLocalFixtureDocker(base, context => run('docker', ['context', 'inspect', ...(context ? [context] : []), '--format', '{{.Endpoints.docker.Host}}']));
function testBinary(pkg, test, features = []) {
  const output = run('cargo', ['test', '--locked', '-p', pkg, ...features, '--test', test, '--no-run', '--message-format=json']);
  const binary = output.split('\n').map(line => JSON.parse(line)).find(message => message.reason === 'compiler-artifact' && message.target.name === test && message.executable)?.executable;
  if (!binary) throw Error(`${test} fixture binary missing`);
  return binary;
}
const binaries = [
  { name: 'runtime', path: testBinary('stoneql-runtime', 'ssh_tunnel'), test: 'ssh_tunnel_uses_owned_fixture', marker: 'STONEQL_SSH_RESULT ' },
  { name: 'native', path: testBinary('stoneql-desktop', 'live_ssh', ['--features', 'custom-protocol']), test: 'ssh_commands_use_owned_fixture', marker: 'STONEQL_NATIVE_RESULT ' },
];
run('docker', ['build', '-q', '-t', IMAGE, resolve(root, 'tools/ssh-fixture')]);
const output = resolve(root, 'artifacts/m005');
mkdirSync(output, { recursive: true });

const tlsConfig = `[req]
distinguished_name = dn
x509_extensions = v3_ca
[dn]
CN = StoneQL disposable CA
[v3_ca]
basicConstraints = critical,CA:true
keyUsage = critical,keyCertSign,cRLSign
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always
`;
function sshdConfig(port) {
  // Least privilege: local forwards only to the fixture database names, no sessions/agent/X11.
  return `Port 22
HostKey /etc/ssh/stoneql_host_key
AuthorizedKeysFile /etc/ssh/stoneql-keys/%u
AllowUsers stoneql
PermitRootLogin no
PubkeyAuthentication yes
PasswordAuthentication yes
KbdInteractiveAuthentication no
AllowTcpForwarding local
AllowAgentForwarding no
AllowStreamLocalForwarding no
X11Forwarding no
PermitTunnel no
PermitTTY no
GatewayPorts no
ForceCommand /bin/false
PermitOpen db.stoneql.test:${port} wrong.stoneql.test:${port}
MaxStartups 64:30:128
# OpenSSH 10 penalizes sources that disconnect before authenticating; the fixture does that
# deliberately (cancellation/failure cycles), so penalties would mask StoneQL behaviour.
PerSourcePenalties no
LogLevel DEBUG1
`;
}
async function waitFor(check, label, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await check()) return;
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  throw Error(`${label} not ready`);
}
function banner(port) {
  return new Promise(resolve => {
    const socket = connect({ host: '127.0.0.1', port });
    const done = value => { socket.destroy(); resolve(value); };
    socket.setTimeout(2000, () => done(false));
    socket.once('data', data => done(data.toString().startsWith('SSH-2.0-')));
    socket.once('error', () => done(false));
  });
}

// Only the dedicated per-run service is ever queried or deleted; exit status 44 = not found.
function keychainItemsRemaining(service, purge = false) {
  if (process.platform !== 'darwin' || !service) return 0;
  let remaining = 0;
  for (let attempt = 0; attempt < 32; attempt++) {
    const lookup = spawnSync('security', [purge ? 'delete-generic-password' : 'find-generic-password', '-s', service], { env: base, encoding: 'utf8', timeout: 10_000 });
    if (lookup.status === 44) return remaining;
    if (lookup.status !== 0) throw Error('Keychain lookup failed');
    remaining++;
    if (!purge) return remaining;
  }
  return remaining;
}
for (const engine of engines) {
  // Short base path: Unix socket endpoints are limited to ~104 bytes on macOS.
  const temp = mkdtempSync('/tmp/stoneql-m005-');
  const tls = join(temp, 'tls'), init = join(temp, 'init'), exposed = join(temp, 'server-tls');
  const sshA = join(temp, 'ssh-a'), sshB = join(temp, 'ssh-b'), client = join(temp, 'client');
  const endpoints = join(temp, 't');
  for (const directory of [tls, init, exposed, sshA, sshB, client, endpoints]) mkdirSync(directory, { mode: 0o700 });
  const project = `stoneql-m005-${randomBytes(6).toString('hex')}`;
  const dbPassword = randomBytes(24).toString('hex');
  const sshPassword = randomBytes(24).toString('hex');
  const passphrase = randomBytes(18).toString('hex');
  [dbPassword, sshPassword, passphrase].forEach(secret => secrets.add(secret));
  const marker = randomUUID();
  const databasePort = engine === 'postgresql' ? 5432 : 3306;
  const env = { ...base, STONEQL_FIXTURE_PASSWORD: dbPassword, STONEQL_SSH_PASSWORD: sshPassword, STONEQL_FIXTURE_INIT: init, STONEQL_SPIKE_FIXTURE_ID: marker };
  const override = join(temp, 'compose.json');
  const args = ['compose', '-f', resolve(root, 'experiments/sqlx-spike/fixtures/compose.yml'), '-f', override, '-p', project];
  let started = false;
  let agent;
  const cleanup = { attempted: false, remainingContainers: null, remainingNetworks: null, agentStopped: null, endpointDirectoriesLeft: null, keychainItemsPurged: null, temporaryRemoved: false };
  const report = { engine, image: IMAGE };
  try {
    writeFileSync(join(tls, 'root.cnf'), tlsConfig);
    for (const name of ['ca', 'bad-ca']) run('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(tls, `${name}-key.pem`), '-out', join(tls, `${name}.pem`), '-days', '1', '-config', join(tls, 'root.cnf'), '-extensions', 'v3_ca', '-subj', `/CN=StoneQL disposable ${name}`]);
    run('openssl', ['req', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(tls, 'server-key.pem'), '-out', join(tls, 'server.csr'), '-subj', '/CN=db.stoneql.test']);
    // The original database hostname (and loopback for the Direct baseline) only; the
    // wrong.stoneql.test alias reaches the same server but is outside the certificate.
    writeFileSync(join(tls, 'extensions.cnf'), 'subjectAltName=DNS:db.stoneql.test,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n');
    run('openssl', ['x509', '-req', '-in', join(tls, 'server.csr'), '-CA', join(tls, 'ca.pem'), '-CAkey', join(tls, 'ca-key.pem'), '-CAcreateserial', '-out', join(tls, 'server.pem'), '-days', '1', '-extfile', join(tls, 'extensions.cnf')]);
    for (const name of ['ca.pem', 'server.pem', 'server-key.pem']) writeFileSync(join(exposed, name), readFileSync(join(tls, name)), { mode: 0o644 });

    // Host keys A (enrolled) and B (rotated server), client keys. The passphrase is passed to
    // ssh-keygen's argument list: a disposable random value for a loopback-only fixture key.
    for (const [directory, name] of [[sshA, 'host_key'], [sshB, 'host_key']]) run('ssh-keygen', ['-q', '-t', 'ed25519', '-N', '', '-C', '', '-f', join(directory, name)]);
    run('ssh-keygen', ['-q', '-t', 'ed25519', '-N', '', '-C', 'stoneql-fixture-plain', '-f', join(client, 'plain')]);
    run('ssh-keygen', ['-q', '-t', 'ed25519', '-N', passphrase, '-C', 'stoneql-fixture-locked', '-f', join(client, 'locked')]);
    run('ssh-keygen', ['-q', '-t', 'ed25519', '-N', '', '-C', 'stoneql-fixture-other', '-f', join(client, 'other')]);
    run('ssh-keygen', ['-q', '-t', 'ecdsa', '-b', '256', '-N', '', '-C', 'stoneql-fixture-ecdsa', '-f', join(client, 'ecdsa')]);
    const authorized = ['plain', 'locked', 'ecdsa'].map(name => readFileSync(join(client, `${name}.pub`), 'utf8').trim()).join('\n') + '\n';
    for (const directory of [sshA, sshB]) {
      writeFileSync(join(directory, 'authorized_keys'), authorized, { mode: 0o644 });
      writeFileSync(join(directory, 'sshd_config'), sshdConfig(databasePort), { mode: 0o644 });
      chmodSync(join(directory, 'host_key'), 0o644); // copied to 0600 inside the container
      chmodSync(directory, 0o755);
    }
    for (const name of ['plain', 'locked', 'other', 'ecdsa']) {
      const text = readFileSync(join(client, name), 'utf8');
      secrets.add(text.split('\n')[1]);
    }
    const hostKey = path => readFileSync(path, 'utf8').trim().split(/\s+/).slice(0, 2).join(' ');

    const sshService = directory => ({ image: IMAGE, environment: ['STONEQL_SSH_PASSWORD'], volumes: [`${directory}:/stoneql-ssh:ro`], ports: ['127.0.0.1::22'], mem_limit: '128m' });
    const aliases = { networks: { default: { aliases: ['db.stoneql.test', 'wrong.stoneql.test'] } } };
    let database;
    if (engine === 'postgresql') {
      writeFileSync(join(exposed, 'pg_hba.conf'), 'local all all trust\nhostssl all all all scram-sha-256\nhostnossl all all all reject\n', { mode: 0o644 });
      const prepare = 'install -d -o postgres -m 700 /tmp/stoneql-tls && install -o postgres -m 600 /stoneql-tls/server-key.pem /tmp/stoneql-tls/ && install -o postgres -m 644 /stoneql-tls/server.pem /stoneql-tls/pg_hba.conf /tmp/stoneql-tls/ && exec docker-entrypoint.sh postgres -c ssl=on -c ssl_cert_file=/tmp/stoneql-tls/server.pem -c ssl_key_file=/tmp/stoneql-tls/server-key.pem -c hba_file=/tmp/stoneql-tls/pg_hba.conf -c ssl_min_protocol_version=TLSv1.2';
      database = { volumes: [`${exposed}:/stoneql-tls:ro`], entrypoint: ['sh', '-c', prepare], ...aliases };
      writeFileSync(join(init, '001-fixture.sql'), `CREATE TABLE stoneql_fixture_marker(id text PRIMARY KEY); INSERT INTO stoneql_fixture_marker VALUES ('${marker}');
CREATE ROLE stoneql_reader LOGIN PASSWORD '${dbPassword}';
GRANT CONNECT ON DATABASE stoneql_m001_fixture TO stoneql_reader;
GRANT USAGE ON SCHEMA public TO stoneql_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO stoneql_reader;
`);
    } else {
      database = { volumes: [`${exposed}:/stoneql-tls:ro`], command: ['--ssl-ca=/stoneql-tls/ca.pem', '--ssl-cert=/stoneql-tls/server.pem', '--ssl-key=/stoneql-tls/server-key.pem', '--require-secure-transport=ON'], ...aliases };
      writeFileSync(join(init, '001-fixture.sql'), `CREATE TABLE stoneql_fixture_marker(id VARCHAR(64) PRIMARY KEY); INSERT INTO stoneql_fixture_marker VALUES ('${marker}');
CREATE USER 'stoneql_reader'@'%' IDENTIFIED BY '${dbPassword}' REQUIRE SSL; GRANT SELECT ON stoneql_m001_fixture.* TO 'stoneql_reader'@'%';`);
    }
    writeFileSync(override, JSON.stringify({ services: { [engine]: database, ssh: sshService(sshA), 'ssh-rotated': sshService(sshB) } }));
    started = true;
    run('docker', [...args, 'up', '-d', engine, 'ssh', 'ssh-rotated'], env);

    const probe = engine === 'postgresql'
      ? ['psql', '-U', 'stoneql_fixture', '-d', 'stoneql_m001_fixture', '-tAc', 'SELECT id FROM stoneql_fixture_marker']
      : ['sh', '-c', `MYSQL_PWD="$${engine === 'mysql' ? 'MYSQL' : 'MARIADB'}_ROOT_PASSWORD" exec ${engine === 'mysql' ? 'mysql' : 'mariadb'} -uroot -N -e "SELECT id FROM stoneql_m001_fixture.stoneql_fixture_marker"`];
    const listener = engine === 'postgresql' ? ['pg_isready', '-h', '127.0.0.1', '-p', '5432'] : ['bash', '-c', 'exec 3<>/dev/tcp/127.0.0.1/3306'];
    await waitFor(() => {
      const marked = spawnSync('docker', [...args, 'exec', '-T', engine, ...probe], { env, encoding: 'utf8', timeout: 10_000 });
      if (marked.status !== 0 || marked.stdout.trim() !== marker) return false;
      return spawnSync('docker', [...args, 'exec', '-T', engine, ...listener], { env, encoding: 'utf8', timeout: 10_000 }).status === 0;
    }, `${engine} TLS fixture`);
    const published = service => {
      const value = run('docker', [...args, 'port', service, service.startsWith('ssh') ? '22' : String(databasePort)], env);
      if (!/^127\.0\.0\.1:\d+$/.test(value)) throw Error('Loopback fixture required');
      return value.split(':')[1];
    };
    const ports = { direct: published(engine), ssh: published('ssh'), rotated: published('ssh-rotated') };
    await waitFor(() => banner(Number(ports.ssh)), 'ssh');
    await waitFor(() => banner(Number(ports.rotated)), 'rotated ssh');

    // Private agent holding only the generated plain key.
    const agentSocket = join(temp, 'agent.sock');
    agent = spawn('ssh-agent', ['-D', '-a', agentSocket], { env: base, stdio: 'ignore' });
    await waitFor(() => existsSync(agentSocket), 'ssh-agent', 10_000);
    run('ssh-add', ['-q', join(client, 'plain')], { ...base, SSH_AUTH_SOCK: agentSocket });

    Object.assign(env, {
      TMPDIR: endpoints,
      STONEQL_SSH_ENGINE: engine, STONEQL_SSH_SCOPE: engine === 'mysql' ? 'full' : 'parity',
      STONEQL_SSH_DB_PORT: String(databasePort), STONEQL_LIFECYCLE_PORT: ports.direct,
      STONEQL_SSH_PORT: ports.ssh, STONEQL_SSH_ROTATED_PORT: ports.rotated, STONEQL_SSH_USER: 'stoneql',
      STONEQL_SSH_HOST_KEY: hostKey(join(sshA, 'host_key.pub')), STONEQL_SSH_ROTATED_HOST_KEY: hostKey(join(sshB, 'host_key.pub')),
      STONEQL_SSH_KEY_PLAIN: join(client, 'plain'), STONEQL_SSH_KEY_LOCKED: join(client, 'locked'), STONEQL_SSH_KEY_PASSPHRASE: passphrase,
      STONEQL_SSH_KEY_OTHER: join(client, 'other'), STONEQL_SSH_KEY_ECDSA: join(client, 'ecdsa'), STONEQL_SSH_AGENT: agentSocket,
      STONEQL_LIFECYCLE_CA: join(tls, 'ca.pem'), STONEQL_LIFECYCLE_BAD_CA: join(tls, 'bad-ca.pem'),
      STONEQL_KEYCHAIN_SERVICE: `app.dbadmin.desktop.test-${randomBytes(6).toString('hex')}`,
    });
    for (const binary of binaries) {
      const result = spawnSync(binary.path, ['--ignored', '--exact', binary.test, '--nocapture', '--test-threads=1'], { cwd: root, env, encoding: 'utf8', timeout: 600_000 });
      const combined = `${result.stdout}\n${result.stderr}`;
      const leaked = [...secrets].filter(secret => combined.includes(secret)).length;
      if (result.status !== 0 || result.error) throw Error(`${engine} ${binary.name} fixture failed\n${redact(combined).slice(-8000)}`);
      if (leaked) throw Error(`${engine} ${binary.name}: ${leaked} generated secret(s) appeared in test output`);
      const line = result.stdout.split('\n').find(text => text.includes(binary.marker));
      if (!line) throw Error(`${binary.name} report missing`);
      report[binary.name] = JSON.parse(line.slice(line.indexOf(binary.marker) + binary.marker.length));
      report[binary.name].secretsInOutput = 0;
    }
    report.keychainItemsRemaining = keychainItemsRemaining(env.STONEQL_KEYCHAIN_SERVICE);
    if (report.keychainItemsRemaining) throw Error('Fixture keychain items remain');
    // Server-side evidence: no SSH connection left open, no session/agent-forwarding requests.
    let established = -1;
    await waitFor(() => {
      established = Number(run('docker', [...args, 'exec', '-T', 'ssh', 'sh', '-c', 'netstat -tn 2>/dev/null | grep -c ":22 .*ESTABLISHED" || true'], env));
      return established === 0;
    }, 'server-side SSH connection drain', 15_000);
    const logs = run('docker', [...args, 'logs', 'ssh'], env) + run('docker', [...args, 'logs', 'ssh-rotated'], env);
    const count = pattern => (logs.match(pattern) || []).length;
    report.server = {
      establishedAfter: established,
      directTcpipChannels: count(/ctype direct-tcpip/g),
      sessionChannels: count(/ctype session/g),
      agentForwardingRequests: count(/auth-agent-req/g),
      acceptedPassword: count(/Accepted password/g),
      acceptedPublickey: count(/Accepted publickey/g),
      failedPassword: count(/Failed password/g),
      secretsInLogs: [...secrets].filter(secret => logs.includes(secret)).length,
    };
    if (report.server.sessionChannels || report.server.agentForwardingRequests || report.server.secretsInLogs) throw Error('Unexpected SSH session, agent forwarding or secret in server logs');
    report.reportSecrets = [...secrets].filter(secret => JSON.stringify(report).includes(secret)).length;
    if (report.reportSecrets) throw Error('Generated secret in report');
    writeFileSync(join(output, `ssh-${engine}.json`), JSON.stringify(report, null, 2) + '\n');
    console.log(`${engine}: SSH transport fixture passed (${report.server.directTcpipChannels} forwarded channels, ${report.runtime.full ? 'full matrix' : 'parity'})`);
  } finally {
    try {
      if (agent) { agent.kill('SIGTERM'); await new Promise(resolve => agent.exitCode !== null || agent.signalCode ? resolve() : agent.once('exit', resolve)); cleanup.agentStopped = agent.exitCode !== null || agent.signalCode !== null; }
      cleanup.endpointDirectoriesLeft = existsSync(endpoints) ? readdirSync(endpoints).length : 0;
      // A failed test may stop before its own cleanup; purge the dedicated service and record it.
      cleanup.keychainItemsPurged = keychainItemsRemaining(env.STONEQL_KEYCHAIN_SERVICE, true);
      if (started) {
        cleanup.attempted = true;
        run('docker', [...args, 'down', '--volumes', '--remove-orphans'], env);
        const containers = run('docker', ['ps', '-a', '--filter', `label=com.docker.compose.project=${project}`, '-q']);
        const networks = run('docker', ['network', 'ls', '--filter', `label=com.docker.compose.project=${project}`, '-q']);
        cleanup.remainingContainers = containers ? containers.split('\n').length : 0;
        cleanup.remainingNetworks = networks ? networks.split('\n').length : 0;
        if (containers || networks) throw Error('Owned fixture resources remain');
      }
      if (cleanup.endpointDirectoriesLeft) throw Error('SSH endpoint directories remain');
    } finally {
      try { rmSync(temp, { recursive: true, force: true }); }
      finally {
        cleanup.temporaryRemoved = !existsSync(temp);
        writeFileSync(join(output, `ssh-cleanup-${engine}.json`), JSON.stringify(cleanup, null, 2) + '\n');
      }
    }
  }
}
