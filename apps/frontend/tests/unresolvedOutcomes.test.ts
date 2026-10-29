import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createPinia, setActivePinia } from 'pinia';
import { loadModule } from './helpers/vueHost.ts';
const preferences = new Map<string, string>();
Object.assign(globalThis, { localStorage: { getItem: (key: string) => preferences.get(key) ?? null, setItem: (key: string, value: string) => preferences.set(key, value) } });
const { useWorkspace } = await loadModule('../../src/stores/workspace.ts');
const { bootstrapNativeHost } = await loadModule('../../src/services/nativeHost.ts');
const { describeUnresolvedOutcome } = await loadModule('../../src/services/liveConnections.ts');
const { connections } = await loadModule('../../src/data/sample.ts');

const lostCommit = { outcomeId: 'lease_1', database: 'shop', secondsAgo: 5, transaction: { outcome: 'unknown', reason: 'requested', commitSent: true, rollbackSent: false }, job: null };
const interrupted = { outcomeId: 'job_1', database: 'shop', secondsAgo: 2, transaction: null, job: { statementCount: 3, unknownStatement: 1, inTransaction: false, cancellation: 'requested', serverCancelSent: true, deadlineExceeded: false } };
/** A desktop host that, like the native runtime, keeps unknown outcomes across sessions until acknowledged. */
function host(initial: any[], options: { failAck?: boolean; jobOutcome?: string; refuse?: boolean } = {}) {
  const outcomes = [...initial], calls: Array<{ command: string; payload: any }> = [];
  let sessions = 0;
  const reply = (command: string, payload: any) => {
    switch (command) {
      case 'host_info': return { runtime: 'desktop', appVersion: '0.1.0', sessionId: `session_${++sessions}`, nonce: payload.nonce, simulated: true };
      case 'contract_probe': return { values: payload.values, simulated: true };
      case 'connect_direct': return { handle: 'h', engineVersion: 'v', database: 'shop', expiresAfterSeconds: 3600, access: 'read_write' };
      case 'list_unresolved_outcomes': return { outcomes: [...outcomes], limit: 16 };
      case 'acknowledge_outcome': {
        if (options.failAck) throw { code: 'DEADLINE_EXCEEDED', message: 'timeout' };
        const index = outcomes.findIndex((o) => o.outcomeId === payload.outcomeId);
        if (index < 0) throw { code: 'FORBIDDEN', message: 'no' };
        outcomes.splice(index, 1);
        return { outcomes: [...outcomes], limit: 16 };
      }
      case 'submit_job':
        if (options.refuse) throw { code: 'UNRESOLVED_OUTCOMES', message: 'Nothing was sent: acknowledge the unknown outcomes listed first. The server may or may not have applied them.' };
        if (options.jobOutcome === 'unknown') outcomes.push(interrupted);
        return { jobId: 'job_1', statementCount: 3 };
      case 'fetch_job': return { jobId: 'job_1', state: 'finished', outcome: options.jobOutcome ?? 'succeeded', cancellation: 'none', serverCancelSent: false, deadlineExceeded: false, statements: [], batch: null, lease: null, done: true };
      default: throw { code: 'INTERNAL', message: command };
    }
  };
  const global = { __TAURI__: { core: { async invoke(command: string, args: { message: string }) {
    const request = JSON.parse(args.message);
    calls.push({ command, payload: request.payload });
    return { version: 1, requestId: request.requestId, payload: reply(command, request.payload) };
  } } } };
  return { global, calls, outcomes };
}
async function storeOn(global: unknown) {
  preferences.clear();
  await bootstrapNativeHost(global);
  setActivePinia(createPinia());
  return useWorkspace();
}

test('unknown outcomes are described plainly and never as success or rollback', () => {
  for (const outcome of [lostCommit, interrupted, { ...lostCommit, transaction: { ...lostCommit.transaction, commitSent: false, rollbackSent: true, reason: 'closed' } }]) {
    const text = describeUnresolvedOutcome(outcome);
    assert.match(text, /Outcome unknown — the server may or may not have applied it\./);
    assert.match(text, /Verify on the server; StoneQL never retries\./);
    assert.doesNotMatch(text, /committed|rolled back|succeeded|success/i);
  }
  assert.match(describeUnresolvedOutcome(lostCommit), /on “shop”.*COMMIT may have been sent and was not confirmed/);
  assert.match(describeUnresolvedOutcome(interrupted), /statement 2 of 3.*may still be running on the server/);
});

test('after a reload the new renderer lists the previous session\'s unknown outcomes until acknowledged', async () => {
  const native = host([lostCommit, interrupted]);
  // First renderer session (before reload): nothing listed yet by this store.
  let store = await storeOn(native.global);
  // Reload: a new page bootstraps a new session and store, then lists what the host kept.
  store = await storeOn(native.global);
  await store.refreshUnresolvedOutcomes();
  assert.deepEqual(store.unresolvedOutcomes.map((o: any) => o.outcomeId), ['lease_1', 'job_1']);
  assert.equal(store.outcomeProfiles.lease_1, undefined, 'outcomes from before the reload have no profile in this session');
  // A failed acknowledgement keeps the entry: nothing is hidden without the host's record.
  const failing = host([lostCommit], { failAck: true });
  const other = await storeOn(failing.global);
  await other.refreshUnresolvedOutcomes();
  await assert.rejects(other.acknowledgeUnresolvedOutcome('lease_1'));
  assert.equal(other.unresolvedOutcomes.length, 1);
  // Back on the first host: explicit acknowledgement sends only the id and removes it.
  await bootstrapNativeHost(native.global);
  await store.acknowledgeUnresolvedOutcome('lease_1');
  const ack = native.calls.filter((c) => c.command === 'acknowledge_outcome');
  assert.deepEqual(Object.keys(ack[0]!.payload).sort(), ['outcomeId', 'sessionId']);
  assert.deepEqual(store.unresolvedOutcomes.map((o: any) => o.outcomeId), ['job_1']);
  await bootstrapNativeHost({});
});

test('an unknown job outcome and a refused submission refresh the list; browser mode never asks', async () => {
  const native = host([], { jobOutcome: 'unknown' });
  const store = await storeOn(native.global);
  const profile = { ...connections[0], mode: 'live', transport: { type: 'direct' } };
  store.connections.push(profile);
  await store.connectLiveProfile(profile, undefined, 'read_write');
  const job = await store.runLiveJob(profile.id, 'UPDATE t SET a = 1');
  assert.equal(job.outcome, 'unknown');
  assert.deepEqual(store.unresolvedOutcomes.map((o: any) => o.outcomeId), ['job_1']);
  assert.equal(store.outcomeProfiles.job_1, profile.id);
  const refused = host([lostCommit], { refuse: true });
  const second = await storeOn(refused.global);
  second.connections.push(profile);
  await second.connectLiveProfile(profile, undefined, 'read_write');
  await assert.rejects(second.runLiveJob(profile.id, 'UPDATE t SET a = 1'), { code: 'UNRESOLVED_OUTCOMES', message: /acknowledge the unknown outcomes listed first/ });
  assert.deepEqual(second.unresolvedOutcomes.map((o: any) => o.outcomeId), ['lease_1']);
  const browser = await storeOn({});
  await browser.refreshUnresolvedOutcomes();
  assert.deepEqual(browser.unresolvedOutcomes, []);
});

test('a job the reload detached is listed once it records its outcome, without user action', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  // The reset detached a running job: the first list is empty but reports it pending; the
  // job then records its unknown outcome and the host reports nothing pending.
  const native = host([]);
  let lists = 0;
  const invoke = native.global.__TAURI__.core.invoke;
  native.global.__TAURI__.core.invoke = async (command: string, args: { message: string }) => {
    if (command !== 'list_unresolved_outcomes') return invoke(command, args);
    const request = JSON.parse(args.message);
    native.calls.push({ command, payload: request.payload });
    const late = ++lists >= 3;
    return { version: 1, requestId: request.requestId, payload: { outcomes: late ? [interrupted] : [], limit: 16, pending: late ? 0 : 1 } };
  };
  const store = await storeOn(native.global);
  await store.refreshUnresolvedOutcomes();
  assert.deepEqual(store.unresolvedOutcomes, []);
  for (let tick = 0; tick < 4; tick++) { t.mock.timers.tick(1000); await new Promise((resolve) => setImmediate(resolve)); }
  assert.deepEqual(store.unresolvedOutcomes.map((o: any) => o.outcomeId), ['job_1']);
  assert.equal(lists, 3, 'listing stops once nothing is pending');
  // Nothing pending (or an older host without the field): a single list, no polling.
  const quiet = host([lostCommit]);
  const other = await storeOn(quiet.global);
  await other.refreshUnresolvedOutcomes();
  t.mock.timers.tick(10_000);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(quiet.calls.filter((c) => c.command === 'list_unresolved_outcomes').length, 1);
  await bootstrapNativeHost({});
});
