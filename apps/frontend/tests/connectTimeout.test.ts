import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { bootstrapNativeHost, nativeWaitFor, SECRET_CONNECT_WAIT_MS } from '../src/services/nativeHost.ts';
import { openLive } from '../src/services/liveConnections.ts';
import { connections } from '../src/data/sample.ts';
const profile = { ...connections[0]!, mode: 'live' as const, password: '', passwordRef: undefined as string | undefined, transport: { type: 'direct' as const } };
/** Native stand-in: connect_direct replies only when `release` is called; everything else at once. */
function host() {
  const calls: Array<{ command: string; payload: any }> = [];
  let release: ((handle: string) => void) | undefined;
  const global = { __TAURI__: { core: { invoke: (command: string, args: { message: string }) => {
    const request = JSON.parse(args.message);
    calls.push({ command, payload: request.payload });
    const reply = (payload: unknown) => ({ version: 1, requestId: request.requestId, payload });
    if (command === 'host_info') return Promise.resolve(reply({ runtime: 'desktop', appVersion: '0.1.0', sessionId: 'session_1', nonce: request.payload.nonce, simulated: true }));
    if (command === 'contract_probe') return Promise.resolve(reply({ values: request.payload.values, simulated: true }));
    if (command === 'connect_direct') return new Promise((done) => { release = (handle) => done(reply({ handle, engineVersion: 'v', database: 'd', expiresAfterSeconds: 3600 })); });
    if (command === 'cancel_connect') return Promise.resolve(reply({ cancelled: true }));
    return Promise.resolve(reply({ connected: false }));
  } } } };
  return { calls, global, release: (handle: string) => release!(handle) };
}
const flush = async () => { for (let i = 0; i < 5; i++) await Promise.resolve(); };
test('F-02: a timed-out connect cancels its attempt once and closes a late success', async () => {
  const native = host();
  await bootstrapNativeHost(native.global);
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    const opening = openLive(profile, 'attempt_late').catch((error) => error);
    await flush();
    mock.timers.tick(8000);
    const error = await opening;
    assert.equal(error.code, 'DEADLINE_EXCEEDED');
    assert.deepEqual(native.calls.filter((c) => c.command === 'cancel_connect').map((c) => c.payload.attemptId), ['attempt_late']);
    native.release('late_handle');
    await flush();
    assert.deepEqual(native.calls.filter((c) => c.command === 'disconnect_database').map((c) => c.payload), [{ handle: 'late_handle', sessionId: 'session_1' }]);
  } finally {
    mock.timers.reset();
    await bootstrapNativeHost({});
  }
});
test('F-02: a connect that resolves a saved reference waits for the native secret deadline', async () => {
  const native = host();
  await bootstrapNativeHost(native.global);
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    let settled = false;
    const opening = openLive({ ...profile, passwordRef: '00000000-0000-4000-8000-000000000001' }, 'attempt_ref').catch((error) => error).finally(() => { settled = true; });
    await flush();
    mock.timers.tick(8000);
    await flush();
    assert.equal(settled, false, 'still waiting after 8 s');
    mock.timers.tick(SECRET_CONNECT_WAIT_MS - 8000);
    assert.equal((await opening).code, 'DEADLINE_EXCEEDED');
    native.release('unused');
  } finally {
    mock.timers.reset();
    await bootstrapNativeHost({});
  }
  assert.equal(nativeWaitFor('connect_direct', { ssh: { auth: { method: 'password', passwordRef: 'r' } } }), SECRET_CONNECT_WAIT_MS);
  assert.equal(nativeWaitFor('connect_direct', { ssh: { auth: { method: 'key_file', passphraseRef: 'r' } } }), SECRET_CONNECT_WAIT_MS);
  assert.equal(nativeWaitFor('connect_direct', { password: 'typed' }), 8000);
  assert.equal(nativeWaitFor('delete_secret', {}), 120_000);
});
