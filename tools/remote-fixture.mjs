// `--remote` fixture mode (fixture host side). The harness still runs next to its local Docker
// daemon and owns every container; only the native test binary runs elsewhere (the Windows CI
// runner, reaching the loopback port through a restricted SSH tunnel; see
// tools/test-remote-fixture.mjs and tools/ci-fixture-host/). Over the harness's own stdio it
// sends one READY line per database and waits for the test's output and export files, then
// continues with the same checks (report, export read-back) and teardown as a local run.
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createInterface } from 'node:readline';

export const READY = 'STONEQL_REMOTE_READY ';
const EXPORTS = ['export_values.sql', 'export_values.csv'];
let replies;

// `test` names a further test of the same binary (with `STONEQL_RSA_PHASE` and the server's
// public key); without it the client runs its session's main test, which also returns exports.
export async function runRemotely(env, engine, test) {
  replies ??= createInterface({ input: process.stdin })[Symbol.asyncIterator]();
  const pem = (name) => readFileSync(env[name], 'utf8');
  console.log(READY + JSON.stringify({
    engine,
    port: Number(env.STONEQL_LIFECYCLE_PORT),
    password: env.STONEQL_FIXTURE_PASSWORD,
    marker: env.STONEQL_SPIKE_FIXTURE_ID,
    ca: pem('STONEQL_LIFECYCLE_CA'),
    badCa: pem('STONEQL_LIFECYCLE_BAD_CA'),
    ...(test ? { test, rsaPhase: env.STONEQL_RSA_PHASE, serverPublicKey: env.STONEQL_SERVER_PUBLIC_KEY ? pem('STONEQL_SERVER_PUBLIC_KEY') : undefined } : {}),
  }));
  const { value, done } = await replies.next();
  if (done) throw Error(`${engine}: remote test client disconnected`);
  const reply = JSON.parse(value);
  if (reply.passed !== true) throw Error(`${engine}: remote native test failed`);
  if (test) return String(reply.stdout);
  for (const name of EXPORTS) {
    if (typeof reply.exports?.[name] !== 'string') throw Error(`${engine}: remote export ${name} missing`);
    writeFileSync(join(env.STONEQL_EXPORT_DIR, name), Buffer.from(reply.exports[name], 'base64'), { mode: 0o600 });
  }
  return String(reply.stdout);
}

// The reply reader keeps stdin open; a finished harness releases it so the process can exit.
export function endRemote() {
  replies?.return?.();
  process.stdin.destroy();
}
