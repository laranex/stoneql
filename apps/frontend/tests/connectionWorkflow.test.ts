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
function nativeFixture(failConnect = false) {
  const calls: Array<{ command: string; payload: any }> = [];
  const global = { __TAURI__: { core: { async invoke(command: string, args: { message: string }) {
    const request = JSON.parse(args.message);
    calls.push({ command, payload: request.payload });
    if (command === 'connect_direct' && failConnect) throw { code: 'CONNECTION_UNAVAILABLE', message: 'synthetic-password leaked' };
    const payload = command === 'host_info' ? { runtime: 'desktop', appVersion: '0.1.0', sessionId: 'test_session', nonce: request.payload.nonce, simulated: true }
      : command === 'contract_probe' ? { values: request.payload.values, simulated: true }
      : command === 'connect_direct' ? { handle: 'owned_handle', engineVersion: 'fixture-version', database: request.payload.database, expiresAfterSeconds: 3600 }
      : { connected: false };
    return { version: 1, requestId: request.requestId, payload };
  } } } };
  return { calls, global };
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
    confirm: (request: any) => { confirmation = request; },
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
  assert.equal(flow.connectionError.value, 'Live connections require the desktop app, Direct MySQL/MariaDB and a database name.');
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
