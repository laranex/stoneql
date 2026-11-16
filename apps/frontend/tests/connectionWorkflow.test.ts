import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createPinia, setActivePinia } from 'pinia';
import { ref } from 'vue';
import { loadModule } from './helpers/vueHost.ts';
const preferences = new Map<string, string>();
Object.assign(globalThis, {
  localStorage: {
    getItem: (key: string) => preferences.get(key) ?? null,
    setItem: (key: string, value: string) => preferences.set(key, value),
  },
});
const { useConnectionWorkflow } = await loadModule('../../src/composables/useConnectionWorkflow.ts');
const { useWorkspace } = await loadModule('../../src/stores/workspace.ts');
const { bootstrapNativeHost } = await loadModule('../../src/services/nativeHost.ts');
const HOST_KEY = { algorithm: 'ssh-ed25519', publicKey: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl', fingerprint: 'SHA256:9Xl0yQZzJ8m2kKpz1bq5N2f3S5gkT0r9u1g6vXy3K0M' };
function nativeFixture(failConnect: boolean | string = false, pickerCancelled = false, hostKeys: { enrolled?: boolean; declineTrust?: boolean } = {}) {
  const calls: Array<{ command: string; payload: any }> = [];
  let references = 0, cancel: (() => void) | undefined, enrolled = !!hostKeys.enrolled;
  const trusted = () => ({ trusted: enrolled ? { algorithm: HOST_KEY.algorithm, fingerprint: HOST_KEY.fingerprint } : null });
  const global = { __TAURI__: { core: { async invoke(command: string, args: { message: string }) {
    const request = JSON.parse(args.message);
    calls.push({ command, payload: request.payload });
    if (command === 'connect_direct' && failConnect === 'hang') await new Promise<void>((resolve) => { cancel = resolve; });
    if (command === 'connect_direct' && failConnect) throw { code: typeof failConnect === 'string' ? (failConnect === 'hang' ? 'CANCELLED' : failConnect) : 'CONNECTION_UNAVAILABLE', message: 'synthetic-password leaked' };
    if (command === 'cancel_connect') { cancel?.(); return { version: 1, requestId: request.requestId, payload: { cancelled: !!cancel } }; }
    if (command === 'store_secret') return { version: 1, requestId: request.requestId, payload: { reference: `00000000-0000-4000-8000-${String(++references).padStart(12, '0')}` } };
    if (command === 'delete_secret') return { version: 1, requestId: request.requestId, payload: { reference: request.payload.reference } };
    if (command === 'ssh_host_key') return { version: 1, requestId: request.requestId, payload: HOST_KEY };
    if (command === 'ssh_host_key_status') return { version: 1, requestId: request.requestId, payload: trusted() };
    if (command === 'trust_ssh_host_key') {
      if (hostKeys.declineTrust) throw { code: 'CANCELLED', message: 'declined' };
      if (request.payload.fingerprint !== HOST_KEY.fingerprint) throw { code: 'INVALID_REQUEST', message: 'no matching scan' };
      enrolled = true;
      return { version: 1, requestId: request.requestId, payload: trusted() };
    }
    if (command === 'forget_ssh_host_key') { enrolled = false; return { version: 1, requestId: request.requestId, payload: trusted() }; }
    if (command === 'pick_ssh_key_file') return { version: 1, requestId: request.requestId, payload: { grant: 'opaque_key_grant', fileName: 'id_ed25519' } };
    const payload = command === 'host_info' ? { runtime: 'desktop', appVersion: '0.1.0', sessionId: 'test_session', nonce: request.payload.nonce, simulated: true, sqlite: true }
      : command === 'contract_probe' ? { values: request.payload.values, simulated: true }
      : command === 'connect_direct' ? { handle: 'owned_handle', engineVersion: 'fixture-version', database: request.payload.database, expiresAfterSeconds: 3600 }
      : command === 'pick_sqlite_file' ? (pickerCancelled ? { grant: null, fileName: null } : { grant: 'opaque_grant', fileName: 'chosen.sqlite' })
      : command === 'connect_sqlite' ? { handle: 'sqlite_handle', engineVersion: 'SQLite 3.51.3', database: 'chosen.sqlite', expiresAfterSeconds: 3600 }
      : { connected: false };
    return { version: 1, requestId: request.requestId, payload };
  } } } };
  return { calls, global };
}
let lastConfirmation: any;
/** Starts removal and returns its confirmed action. */
async function setupRemoval(flow: any, id: string) {
  flow.removeConnection(id);
  return lastConfirmation.action;
}
async function setup(host: unknown = {}, options: { discard?: boolean } = {}) {
  preferences.clear();
  await bootstrapNativeHost(host);
  setActivePinia(createPinia());
  const store = useWorkspace();
  const manager = ref(true), notices: string[] = [], asked: string[] = [];
  let confirmation: any;
  const flow = useConnectionWorkflow({
    manager,
    notify: (message: string, type = 'success') => notices.push(`${type}:${message}`),
    discard: async () => options.discard ?? true,
    askDiscard: async (message: string) => { asked.push(message); return options.discard ?? true; },
    confirm: (request: any) => { confirmation = request; lastConfirmation = request; },
  });
  return { store, flow, manager, notices, asked, confirmation: () => confirmation };
}
function liveDraft(flow: any) {
  flow.newConnection();
  Object.assign(flow.connectionDraft.value, { name: 'Live fixture', mode: 'live', password: 'synthetic-password', rootCertificatePem: 'synthetic-public-ca' });
}
test('mock save validates required fields, then tests, stores and opens the connection', async () => {
  const { store, flow, manager, notices } = await setup();
  flow.newConnection();
  assert.equal(flow.connectionDialog.value, true);
  await flow.saveConnection();
  assert.equal(flow.connectionError.value, 'Name, host, and username are required.');
  flow.connectionDraft.value.name = 'Fixture';
  await flow.saveConnection();
  const id = flow.connectionDraft.value.id;
  assert.ok(store.connections.some((c: any) => c.id === id));
  assert.equal(store.activeId, id);
  assert.ok(store.opened.includes(id));
  assert.equal(flow.connectionDialog.value, false);
  assert.equal(manager.value, false);
  assert.equal(flow.testing.value, false);
  assert.deepEqual(notices, ['success:Configuration saved; mock connection opened.']);
});
test('mock test reports transport validation without saving', async () => {
  const { store, flow } = await setup();
  const before = store.connections.length;
  flow.newConnection();
  flow.connectionDraft.value.name = 'Fixture';
  flow.changeTransport('ssh');
  await flow.testConnection();
  assert.equal(flow.testOk.value, false);
  assert.equal(flow.testMessage.value.length > 0, true);
  assert.equal(store.connections.length, before);
});
test('live profiles are refused outside the desktop host before any state changes', async () => {
  const { store, flow } = await setup({});
  const before = JSON.stringify(store.connections);
  liveDraft(flow);
  await flow.saveConnection();
  assert.equal(flow.connectionError.value, 'Live connections require the desktop app, MySQL, MariaDB or PostgreSQL over Direct or SSH and a database name.');
  assert.equal(JSON.stringify(store.connections), before);
  await flow.testConnection();
  assert.equal(flow.testOk.value, false);
  assert.match(flow.testMessage.value, /native desktop app/);
});
test('live save connects through the native bridge without persisting secrets, and close disconnects', async () => {
  const native = nativeFixture();
  const { store, flow, notices } = await setup(native.global);
  liveDraft(flow);
  await flow.testConnection();
  assert.equal(flow.testOk.value, true);
  assert.deepEqual(native.calls.filter(c => ['connect_direct', 'disconnect_database'].includes(c.command)).map(c => c.command), ['connect_direct', 'disconnect_database']);
  await flow.saveConnection();
  const id = flow.connectionDraft.value.id;
  const saved = store.connections.find((c: any) => c.id === id);
  assert.equal(saved.password, undefined);
  assert.equal(flow.connectionDraft.value.password, '');
  assert.equal(store.liveSessions[id].status, 'connected');
  assert.equal(store.workspaces[id].view, 'SQL');
  assert.ok(!preferences.get('stoneql-connections')?.includes('synthetic'));
  assert.equal(notices.at(-1), 'success:Live read-only connection opened over verified TLS.');
  native.calls.length = 0;
  await flow.closeConnection(id);
  assert.deepEqual(native.calls.map(c => c.command), ['disconnect_database']);
  assert.equal(store.liveSessions[id].status, 'disconnected');
  assert.ok(!store.opened.includes(id));
});
test('live connection failures surface a fixed message and leave the dialog open', async () => {
  const native = nativeFixture(true);
  const { store, flow } = await setup(native.global);
  liveDraft(flow);
  await flow.saveConnection();
  const id = flow.connectionDraft.value.id;
  assert.equal(flow.connectionError.value, 'Connection failed. Check authentication, TLS and availability.');
  assert.equal(flow.connectionDialog.value, true);
  assert.equal(flow.testing.value, false);
  assert.equal(store.liveSessions[id].status, 'error');
  assert.ok(!flow.connectionError.value.includes('synthetic-password'));
});
test('cancelling a live draft clears the password and CA from memory', async () => {
  const native = nativeFixture();
  const { flow, asked } = await setup(native.global);
  liveDraft(flow);
  await flow.closeConnectionDialog();
  assert.deepEqual(asked, ['Discard connection configuration changes?']);
  assert.equal(flow.connectionDialog.value, false);
  assert.equal(flow.connectionDraft.value, undefined);
});
test('declining to discard keeps the draft dialog open', async () => {
  const { flow } = await setup({}, { discard: false });
  flow.newConnection();
  flow.connectionDraft.value.name = 'Unsaved';
  await flow.closeConnectionDialog();
  assert.equal(flow.connectionDialog.value, true);
  assert.equal(flow.connectionDraft.value.name, 'Unsaved');
});
test('removing a live connection asks for confirmation, disconnects and removes workspace state', async () => {
  const native = nativeFixture();
  const { store, flow, notices, confirmation } = await setup(native.global);
  liveDraft(flow);
  await flow.saveConnection();
  const id = flow.connectionDraft.value.id;
  flow.removeConnection(id);
  assert.equal(store.connections.some((c: any) => c.id === id), true);
  assert.equal(confirmation().acceptLabel, 'Remove connection');
  native.calls.length = 0;
  await confirmation().action();
  assert.deepEqual(native.calls.map(c => c.command), ['disconnect_database']);
  assert.equal(store.connections.some((c: any) => c.id === id), false);
  assert.equal(store.workspaces[id], undefined);
  assert.equal(notices.at(-1), 'success:Connection removed.');
});
test('CA certificate reading rejects oversized or private-key files', async () => {
  const native = nativeFixture();
  const { flow } = await setup(native.global);
  liveDraft(flow);
  flow.connectionDraft.value.rootCertificatePem = undefined;
  const input = (text: string, size = text.length) => ({ target: { value: 'x', files: [{ size, text: async () => text }] } });
  await flow.readCertificate(input('x', 16385));
  assert.equal(flow.connectionError.value, 'CA certificate must be at most 16 KiB.');
  await flow.readCertificate(input('-----BEGIN CERTIFICATE-----\n-----BEGIN PRIVATE KEY-----'));
  assert.equal(flow.connectionError.value, 'Choose a PEM CA certificate, not a private key.');
  assert.equal(flow.connectionDraft.value.rootCertificatePem, undefined);
  await flow.readCertificate(input('-----BEGIN CERTIFICATE-----\nsynthetic'));
  assert.equal(flow.connectionError.value, '');
  assert.equal(flow.connectionDraft.value.rootCertificatePem, '-----BEGIN CERTIFICATE-----\nsynthetic');
});
test('live SQLite uses a native file grant, sends no path and never persists the grant', async () => {
  const cancelled = nativeFixture(false, true);
  const first = await setup(cancelled.global);
  liveDraft(first.flow);
  Object.assign(first.flow.connectionDraft.value, { engine: 'SQLite', database: '/renderer/typed/path.sqlite' });
  await first.flow.chooseSqliteFile();
  assert.equal(first.flow.connectionDraft.value.sqliteGrant, undefined, 'a cancelled picker grants nothing');
  await first.flow.saveConnection();
  assert.equal(first.flow.connectionError.value, 'Live SQLite requires the desktop app, Direct transport and a file chosen with Choose file.');
  assert.ok(!cancelled.calls.some(c => c.command === 'connect_sqlite'));
  const native = nativeFixture();
  const { store, flow, notices } = await setup(native.global);
  liveDraft(flow);
  Object.assign(flow.connectionDraft.value, { engine: 'SQLite', database: '/renderer/typed/path.sqlite' });
  await flow.chooseSqliteFile();
  assert.equal(flow.connectionDraft.value.sqliteGrant, 'opaque_grant');
  assert.equal(flow.connectionDraft.value.database, 'chosen.sqlite');
  await flow.saveConnection();
  const id = flow.connectionDraft.value.id;
  assert.equal(store.liveSessions[id].status, 'connected');
  assert.equal(notices.at(-1), 'success:Live read-only SQLite file opened.');
  const connect = native.calls.find(c => c.command === 'connect_sqlite')!;
  assert.deepEqual(connect.payload, { grant: 'opaque_grant', sessionId: 'test_session' });
  assert.ok(!JSON.stringify(native.calls).includes('/renderer/typed/path.sqlite'));
  const persisted = preferences.get('stoneql-connections') || '';
  assert.ok(persisted.includes('chosen.sqlite') && !persisted.includes('opaque_grant'));
});

async function sshDraft(flow: any, auth: 'agent' | 'password' | 'private-key' = 'password') {
  liveDraft(flow);
  flow.changeTransport('ssh');
  Object.assign(flow.connectionDraft.value.transport, { host: 'bastion.internal', user: 'deploy', auth });
  await flow.refreshHostKeyTrust();
}
test('F-01: live SSH requires native host-key enrollment before any connection attempt', async () => {
  const native = nativeFixture();
  const { store, flow } = await setup(native.global);
  await sshDraft(flow);
  assert.equal(flow.hostKeyTrust.value, null, 'not enrolled natively');
  await flow.testConnection();
  assert.ok(!native.calls.some((c) => c.command === 'connect_direct'), 'no connect without an enrolled key');
  assert.deepEqual(flow.pendingHostKey.value, HOST_KEY);
  assert.match(flow.testMessage.value, /Verify and trust the SSH host key/);
  flow.rejectHostKey();
  assert.equal(flow.pendingHostKey.value, undefined);
  const before = store.connections.length;
  await flow.saveConnection();
  assert.equal(flow.connectionError.value, 'Fetch, verify and trust the SSH host key before saving a live SSH profile.');
  assert.equal(store.connections.length, before);
  await flow.fetchHostKey();
  await flow.trustHostKey();
  assert.deepEqual(native.calls.find((c) => c.command === 'trust_ssh_host_key')!.payload, { host: 'bastion.internal', port: 22, fingerprint: HOST_KEY.fingerprint, sessionId: 'test_session' });
  assert.deepEqual(flow.hostKeyTrust.value, { algorithm: HOST_KEY.algorithm, fingerprint: HOST_KEY.fingerprint });
  assert.equal(flow.pendingHostKey.value, undefined);
  assert.equal('hostKey' in flow.connectionDraft.value.transport, false, 'the renderer never holds the key');
  await flow.testConnection();
  assert.equal(flow.testOk.value, true);
  assert.match(flow.testMessage.value, /Verified SSH host key and TLS/);
  const connect = native.calls.find((c) => c.command === 'connect_direct')!.payload;
  assert.deepEqual(connect.ssh, { host: 'bastion.internal', port: 22, username: 'deploy', auth: { method: 'password' } });
  assert.match(connect.attemptId, /^attempt_[0-9a-f-]+$/);
  flow.forgetHostKey();
  assert.equal(lastConfirmation.acceptLabel, 'Forget host key');
  assert.match(lastConfirmation.message, /bastion\.internal:22\? All profiles using this SSH server will be refused/);
  await lastConfirmation.action();
  assert.equal(flow.hostKeyTrust.value, null);
  assert.ok(native.calls.some((c) => c.command === 'forget_ssh_host_key'));
});
test('F-01: declining the native trust dialog changes nothing', async () => {
  const native = nativeFixture(false, false, { declineTrust: true });
  const { flow } = await setup(native.global);
  await sshDraft(flow);
  await flow.fetchHostKey();
  await flow.trustHostKey();
  assert.equal(flow.hostKeyTrust.value, null);
  assert.deepEqual(flow.pendingHostKey.value, HOST_KEY, 'the scanned key stays for review');
  assert.equal(flow.connectionError.value, '');
});
test('F-04: editing the SSH host or port clears the pending key and the shown trust, then re-queries', async () => {
  const native = nativeFixture(false, false, { enrolled: true });
  const { flow } = await setup(native.global);
  await sshDraft(flow);
  assert.ok(flow.hostKeyTrust.value);
  await flow.fetchHostKey();
  assert.deepEqual(flow.pendingHostKey.value, HOST_KEY);
  for (const change of [{ host: 'other.internal' }, { port: 2222 }]) {
    Object.assign(flow.connectionDraft.value.transport, change);
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(flow.pendingHostKey.value, undefined, JSON.stringify(change));
    assert.equal(flow.hostKeyTrust.value, undefined, 'unknown until re-queried');
    native.calls.length = 0;
    await new Promise((resolve) => setTimeout(resolve, 350));
    // The native host is global in tests, so an earlier flow's timer may also report here.
    const { host, port } = flow.connectionDraft.value.transport;
    assert.ok(native.calls.some((c) => c.command === 'ssh_host_key_status' && c.payload.host === host && c.payload.port === port), `${host}:${port} re-queried`);
    assert.ok(flow.hostKeyTrust.value, 'trust state of the new endpoint shown');
    await flow.fetchHostKey();
  }
});
test('a changed SSH host key is a hard failure without a trust prompt', async () => {
  const native = nativeFixture('SSH_HOST_KEY_CHANGED', false, { enrolled: true });
  const { flow } = await setup(native.global);
  await sshDraft(flow, 'agent');
  await flow.testConnection();
  assert.equal(flow.testOk.value, false);
  assert.match(flow.testMessage.value, /host key changed — connection refused/);
  assert.equal(flow.pendingHostKey.value, undefined);
  assert.ok(flow.hostKeyTrust.value, 'the enrolled key is never replaced automatically');
  assert.ok(!native.calls.some((c) => c.command === 'trust_ssh_host_key'));
  assert.deepEqual(native.calls.find((c) => c.command === 'connect_direct')!.payload.ssh.auth, { method: 'agent' });
});
test('connection setup can be cancelled while the native host is connecting', async () => {
  const native = nativeFixture('hang');
  const { flow } = await setup(native.global);
  liveDraft(flow);
  const pending = flow.testConnection();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.match(flow.attemptId.value, /^attempt_/);
  assert.equal(flow.testing.value, true);
  await flow.cancelAttempt();
  await pending;
  assert.equal(native.calls.find((c) => c.command === 'cancel_connect')!.payload.attemptId, native.calls.find((c) => c.command === 'connect_direct')!.payload.attemptId);
  assert.equal(flow.testMessage.value, 'Connection setup cancelled; no connection was opened.');
  assert.equal(flow.attemptId.value, '');
  assert.equal(flow.testing.value, false);
});
test('remembered secrets become bound OS references; profiles persist references, never secrets or grants', async () => {
  const native = nativeFixture(false, false, { enrolled: true });
  const { store, flow } = await setup(native.global);
  const { connectionSnapshot } = await loadModule('../../src/services/connectionPreferences.ts');
  await sshDraft(flow, 'private-key');
  Object.assign(flow.connectionDraft.value, { rememberPassword: true });
  await flow.chooseSshKeyFile();
  Object.assign(flow.connectionDraft.value.transport, { passphrase: 'synthetic-passphrase', rememberPassphrase: true });
  assert.equal(flow.connectionDraft.value.transport.keyGrant, 'opaque_key_grant');
  await flow.saveConnection();
  assert.equal(flow.connectionError.value, '');
  const stored = native.calls.filter((c) => c.command === 'store_secret').map((c) => c.payload);
  assert.deepEqual(stored.map((p) => p.purpose), ['database_password', 'ssh_passphrase']);
  assert.deepEqual(stored[0].target, { engine: 'mysql', host: '127.0.0.1', port: 3306, username: 'root', ssh: { host: 'bastion.internal', port: 22, username: 'deploy' } });
  const connect = native.calls.find((c) => c.command === 'connect_direct')!.payload;
  assert.equal(connect.password, 'synthetic-password', 'the typed secret is used for this connection');
  assert.deepEqual(connect.ssh.auth, { method: 'key_file', grant: 'opaque_key_grant', passphrase: 'synthetic-passphrase' });
  const id = flow.connectionDraft.value.id;
  const saved = store.connections.find((c: any) => c.id === id);
  const serialized = JSON.stringify(connectionSnapshot(store.connections, store.groups));
  for (const secret of ['synthetic-password', 'synthetic-passphrase', 'opaque_key_grant', 'remember']) assert.ok(!serialized.includes(secret), secret);
  for (const kept of [saved.passwordRef, saved.transport.passphraseRef, 'id_ed25519']) assert.ok(serialized.includes(kept), kept);
  for (const native of [HOST_KEY.publicKey, HOST_KEY.fingerprint]) assert.ok(!serialized.includes(native), 'host-key trust is native only');
  // A later connect uses references only.
  flow.newConnection(saved);
  await flow.refreshHostKeyTrust();
  assert.equal(flow.connectionDraft.value.rememberPassword, true);
  assert.equal(flow.connectionDraft.value.transport.rememberPassphrase, true);
  await flow.testConnection();
  const second = native.calls.filter((c) => c.command === 'connect_direct').at(-1)!.payload;
  assert.equal(second.password, '');
  assert.equal(second.passwordRef, saved.passwordRef);
  assert.deepEqual(second.ssh.auth, { method: 'key_file', grant: 'opaque_key_grant', passphraseRef: saved.transport.passphraseRef });
  // Turning remembering off forgets the old reference after a successful save.
  flow.connectionDraft.value.rememberPassword = false;
  await flow.saveConnection(false);
  assert.deepEqual(native.calls.filter((c) => c.command === 'delete_secret').map((c) => c.payload.reference), [saved.passwordRef]);
  assert.equal(store.connections.find((c: any) => c.id === id).passwordRef, undefined);
  // Removing the profile forgets its remaining references.
  const remove = await setupRemoval(flow, id);
  await remove();
  assert.deepEqual(native.calls.filter((c) => c.command === 'delete_secret').map((c) => c.payload.reference), [saved.passwordRef, saved.transport.passphraseRef]);
});
test('R-7: a failed close with an open transaction still queries and shows its outcome', async () => {
  const native = nativeFixture();
  const invoke = native.global.__TAURI__.core.invoke;
  native.global.__TAURI__.core.invoke = async (command: string, args: { message: string }) => {
    const request = JSON.parse(args.message);
    if (command === 'disconnect_database') { native.calls.push({ command, payload: request.payload }); throw { code: 'INTERNAL', message: 'close failed' }; }
    if (command === 'end_transaction') { native.calls.push({ command, payload: request.payload }); return { version: 1, requestId: request.requestId, payload: { outcome: 'rolled_back', reason: 'closed', commitSent: false, rollbackSent: true } }; }
    return invoke(command, args);
  };
  const { store, flow, notices, asked } = await setup(native.global);
  liveDraft(flow);
  await flow.saveConnection();
  const id = flow.connectionDraft.value.id;
  store.liveSessions[id].lease = { id: 'lease_1', state: 'active' };
  native.calls.length = 0;
  await flow.closeConnection(id);
  assert.match(asked.at(-1)!, /open transaction/);
  assert.deepEqual(native.calls.map(c => c.command), ['disconnect_database', 'end_transaction']);
  assert.equal(native.calls[1]!.payload.leaseId, 'lease_1');
  assert.match(notices.at(-1)!, /^error:.*Transaction rolled back \(confirmed by the server\)\. Its connection was closed\./);
  assert.match(store.liveSessions[id].error, /Transaction rolled back \(confirmed by the server\)/);
  assert.ok(store.opened.includes(id), 'the workspace stays open after a failed close');
});
test('a job on an idle-expired lease reports the recorded expiry, not a lost or unavailable connection', async () => {
  const native = nativeFixture();
  const invoke = native.global.__TAURI__.core.invoke;
  // The runtime refuses a job on an ended lease with FORBIDDEN before sending anything, and
  // end_transaction returns the lease's recorded end.
  native.global.__TAURI__.core.invoke = async (command: string, args: { message: string }) => {
    const request = JSON.parse(args.message);
    if (command === 'submit_job') { native.calls.push({ command, payload: request.payload }); throw { code: 'FORBIDDEN', message: 'This action is not permitted.' }; }
    if (command === 'end_transaction') { native.calls.push({ command, payload: request.payload }); return { version: 1, requestId: request.requestId, payload: { outcome: 'rolled_back', reason: 'expired', commitSent: false, rollbackSent: true } }; }
    return invoke(command, args);
  };
  const { store, flow } = await setup(native.global);
  liveDraft(flow);
  await flow.saveConnection();
  const id = flow.connectionDraft.value.id;
  store.liveSessions[id].lease = { id: 'lease_1', state: 'active' };
  native.calls.length = 0;
  await assert.rejects(store.runLiveJob(id, 'UPDATE t SET a = 1'), (error: any) => {
    assert.equal(error.code, 'FORBIDDEN');
    assert.equal(error.message, 'Nothing was sent: the transaction is no longer open. Transaction rolled back (confirmed by the server). The transaction had expired after being idle.');
    assert.doesNotMatch(error.message, /connection was lost|Reconnect|unavailable/i);
    return true;
  });
  assert.deepEqual(native.calls.map(c => c.command), ['submit_job', 'end_transaction']);
  assert.equal(native.calls[1]!.payload.leaseId, 'lease_1');
  assert.equal(store.liveSessions[id].lease, undefined, 'the ended lease is no longer shown as open');
  assert.equal(store.liveSessions[id].status, 'connected', 'the connection itself is still usable');
  // Without a lease, FORBIDDEN keeps its connection meaning and no lease end is queried.
  native.calls.length = 0;
  await assert.rejects(store.runLiveJob(id, 'UPDATE t SET a = 1'), (error: any) => error.code === 'FORBIDDEN' && /Reconnect/.test(error.message));
  assert.deepEqual(native.calls.map(c => c.command), ['submit_job']);
});
test('a lease that ends during a job reports its recorded reason', async () => {
  const native = nativeFixture();
  const invoke = native.global.__TAURI__.core.invoke;
  native.global.__TAURI__.core.invoke = async (command: string, args: { message: string }) => {
    const request = JSON.parse(args.message);
    const reply = (payload: unknown) => { native.calls.push({ command, payload: request.payload }); return { version: 1, requestId: request.requestId, payload }; };
    if (command === 'submit_job') return reply({ jobId: 'job_1', statementCount: 1 });
    if (command === 'fetch_job') return reply({ jobId: 'job_1', state: 'finished', outcome: 'unknown', cancellation: 'none', serverCancelSent: false, deadlineExceeded: false, statements: [{ index: 0, status: 'unknown', inTransaction: true, affectedRows: '0', resultSets: 0, failure: null, resultsWithheld: false }], batch: null, lease: 'ended', done: true });
    if (command === 'end_transaction') return reply({ outcome: 'unknown', reason: 'connection_lost', commitSent: false, rollbackSent: false });
    return invoke(command, args);
  };
  const { store, flow } = await setup(native.global);
  liveDraft(flow);
  await flow.saveConnection();
  const id = flow.connectionDraft.value.id;
  store.liveSessions[id].lease = { id: 'lease_1', state: 'active' };
  const result = await store.runLiveJob(id, 'UPDATE t SET a = 1');
  assert.equal(result.lease, 'ended');
  assert.equal(store.liveSessions[id].lease, undefined);
  assert.equal(store.liveSessions[id].error, 'The transaction ended during this job. Transaction outcome unknown. Its connection was lost. Neither COMMIT nor ROLLBACK was sent or confirmed. Verify on the server; StoneQL never retries.');
});
/** A native fixture whose delete_secret fails (locked or denied Keychain), and optionally store_secret too. */
function failingDeletes(failStoreAfter = Infinity, enrolled = false) {
  const native = nativeFixture(false, false, { enrolled });
  const invoke = native.global.__TAURI__.core.invoke;
  let stores = 0;
  native.global.__TAURI__.core.invoke = async (command: string, args: { message: string }) => {
    const request = JSON.parse(args.message);
    if (command === 'delete_secret') { native.calls.push({ command, payload: request.payload }); throw { code: 'SECRET_UNAVAILABLE', message: 'keychain locked' }; }
    if (command === 'store_secret' && ++stores > failStoreAfter) { native.calls.push({ command, payload: request.payload }); throw { code: 'SECRET_UNAVAILABLE', message: 'keychain locked' }; }
    return invoke(command, args);
  };
  return native;
}
test('F-10: a failed credential deletion on removal keeps the profile and offers Remove anyway', async () => {
  const native = failingDeletes();
  const { store, flow, notices } = await setup(native.global);
  liveDraft(flow);
  flow.connectionDraft.value.rememberPassword = true;
  await flow.saveConnection(false);
  const id = flow.connectionDraft.value.id;
  const remove = await setupRemoval(flow, id);
  await remove();
  assert.equal(store.connections.some((c: any) => c.id === id), true, 'profile kept');
  assert.equal(lastConfirmation.acceptLabel, 'Remove anyway');
  assert.match(lastConfirmation.message, /1 saved credential could not be removed from the Keychain \(service app\.dbadmin\.desktop\.credentials\)/);
  await lastConfirmation.action();
  assert.equal(store.connections.some((c: any) => c.id === id), false);
  assert.equal(notices.at(-1), 'success:Connection removed.');
});
test('F-10: a save whose old credential cannot be deleted stands but warns with the count', async () => {
  const native = failingDeletes();
  const { store, flow, notices } = await setup(native.global);
  liveDraft(flow);
  flow.connectionDraft.value.rememberPassword = true;
  await flow.saveConnection(false);
  const id = flow.connectionDraft.value.id;
  flow.newConnection(store.connections.find((c: any) => c.id === id));
  flow.connectionDraft.value.rememberPassword = false;
  await flow.saveConnection(false);
  assert.equal(flow.connectionError.value, '');
  assert.equal(store.connections.find((c: any) => c.id === id).passwordRef, undefined);
  assert.ok(notices.includes('error:Saved, but 1 old credential could not be removed from the Keychain (service app.dbadmin.desktop.credentials).'), notices.join('\n'));
});
test('F-10: when saving fails, a rollback deletion failure is reported, not hidden', async () => {
  const native = failingDeletes(1, true);
  const { store, flow } = await setup(native.global);
  await sshDraft(flow, 'password');
  Object.assign(flow.connectionDraft.value, { rememberPassword: true });
  Object.assign(flow.connectionDraft.value.transport, { password: 'synthetic-ssh', rememberPassword: true });
  await flow.saveConnection(false);
  assert.match(flow.connectionError.value, /1 newly saved credential could not be removed from the Keychain/);
  assert.equal(store.connections.some((c: any) => c.name === flow.connectionDraft.value.name), false);
});
test('F-10: no secret deletion failure is swallowed in the workflow source', async () => {
  const { readFileSync } = await import('node:fs');
  const source = readFileSync(new URL('../src/composables/useConnectionWorkflow.ts', import.meta.url), 'utf8');
  assert.equal(/deleteSecret\([^)]*\)\s*\.catch\(/.test(source), false);
});
