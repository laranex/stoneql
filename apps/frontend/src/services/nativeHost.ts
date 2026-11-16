import type { CellValue, CloseDecision, CloseResult, CloseAction, ContractProbe, Envelope, HostInfo, HostInfoRequest, ProbeResult } from '../../../../contracts/generated/v1';
export interface NativeInvoker { invoke<T>(command: string, args: { message: string }): Promise<T> }
export type HostState = { kind: 'browser' } | { kind: 'desktop'; info: HostInfo } | { kind: 'unavailable' };
let current: HostState = { kind: 'browser' };
let activeInvoker: NativeInvoker | undefined;
let sequence = 0;
function invokerFrom(global: unknown): NativeInvoker | undefined {
  const native = (global as { __TAURI__?: { core?: NativeInvoker } }).__TAURI__?.core;
  return typeof native?.invoke === 'function' ? native : undefined;
}
/** `onLate` receives a reply that arrives after the timeout already failed the call. */
async function call<T, P>(invoker: NativeInvoker, command: string, payload: P, timeoutMs = 2000, onLate?: (payload: T) => void): Promise<T> {
  const request: Envelope<P> = { version: 1, requestId: `desktop_${++sequence}`, payload };
  let timer: ReturnType<typeof setTimeout> | undefined, timedOut = false;
  const invoked = invoker.invoke<Envelope<T>>(command, { message: JSON.stringify(request) });
  if (onLate) invoked.then((late) => { if (timedOut && late?.version === 1 && late.requestId === request.requestId) onLate(late.payload); }, () => undefined);
  try {
    const reply = await Promise.race([
      invoked,
      new Promise<never>((_, reject) => { timer = setTimeout(() => { timedOut = true; reject(new Error('Native host timeout')); }, timeoutMs); }),
    ]);
    if (reply.version !== 1 || reply.requestId !== request.requestId) throw new Error('Native protocol mismatch');
    return reply.payload;
  } finally { if (timer) clearTimeout(timer); }
}
export function hostState(): HostState { return current; }
export async function bootstrapNativeHost(global: unknown = globalThis): Promise<HostState> {
  const invoker = invokerFrom(global);
  if (!invoker) { activeInvoker = undefined; return (current = {kind: 'browser'}); }
  try {
    const payload: HostInfoRequest = {nonce: `bootstrap_${crypto.randomUUID()}`};
    const info = await call<HostInfo, HostInfoRequest>(invoker, 'host_info', payload, 8000);
    if (info.runtime !== 'desktop' || info.nonce !== payload.nonce || !info.sessionId || info.simulated !== true) throw new Error('Native host metadata mismatch');
    const values: CellValue[] = [{kind: 'integer', text: '18446744073709551615'}, {kind: 'decimal', text: '12345678901234567890.4500'}, {kind: 'null'}, {kind: 'binary', hex: '00ff0180'}];
    const probe = await call<ProbeResult, ContractProbe>(invoker, 'contract_probe', {sessionId: info.sessionId, action: 'echo', values});
    if (!probe.simulated || probe.values.length !== values.length || probe.values.some((value, index) => JSON.stringify(Object.entries(value).sort()) !== JSON.stringify(Object.entries(values[index]!).sort()))) throw new Error('Native value round-trip mismatch');
    activeInvoker = invoker;
    return (current = {kind: 'desktop', info});
  } catch { activeInvoker = undefined; return (current = {kind: 'unavailable'}); }
}
export async function confirmNativeRendered(): Promise<void> {
  if (current.kind !== 'desktop' || !activeInvoker) return;
  await call<ProbeResult, ContractProbe>(activeInvoker, 'contract_probe', {sessionId: current.info.sessionId, action: 'ready', values: []});
}

export async function resolveNativeClose(action: CloseAction): Promise<void> {
  if (current.kind !== 'desktop' || !activeInvoker) throw new Error('Native host unavailable');
  await call<CloseResult, CloseDecision>(activeInvoker, 'resolve_close', {sessionId: current.info.sessionId, action});
}

/**
 * Confirmed reload: rotates the native session before the page goes away. The host then resets
 * the registry (leases rolled back with recorded outcomes, jobs interrupted, pools closed) and its
 * navigation guard lets the reload through. The current session id becomes stale on purpose.
 */
export async function invalidateNativeSession(): Promise<void> {
  if (current.kind !== 'desktop' || !activeInvoker) throw new Error('Native host unavailable');
  const payload: HostInfoRequest = {nonce: `reload_${crypto.randomUUID()}`};
  const info = await call<HostInfo, HostInfoRequest>(activeInvoker, 'host_info', payload, 20_000);
  if (info.nonce !== payload.nonce || info.sessionId === current.info.sessionId) throw new Error('Native session was not rotated');
  activeInvoker = undefined;
}

export class NativeOperationError extends Error {
  readonly code: string;
  constructor(code: string, message: string) { super(message); this.code = code; }
}
const nativeMessages: Record<string,string> = {
  FORBIDDEN: 'Connection is unavailable to this session. Reconnect before running queries.',
  INVALID_REQUEST: 'Invalid native connection or query request.',
  UNSUPPORTED_CAPABILITY: 'This live connection capability is not supported.',
  DEADLINE_EXCEEDED: 'Operation timed out; server-side cancellation is unconfirmed.',
  PACKET_TOO_LARGE: 'Response exceeded the incoming limit; the physical connection was discarded.',
  QUERY_REJECTED: 'The database rejected or could not complete the read-only query.',
  CONNECTION_UNAVAILABLE: 'Connection failed. Check authentication, TLS and availability.',
  CAPACITY: 'Connection limit reached. Disconnect another live workspace.',
  UNSUPPORTED_SQL: 'This read-only SQL form is not supported.',
  CANCELLED: 'Connection setup cancelled; no connection was opened.',
  SSH_HOST_KEY_UNKNOWN: 'The SSH host key is not enrolled. Fetch it, verify the fingerprint and trust it explicitly.',
  SSH_HOST_KEY_CHANGED: 'SSH host key changed — connection refused. Verify the new key with the server administrator; forget the stored key only after verification.',
  SSH_AUTHENTICATION_FAILED: 'SSH authentication failed. Check the SSH username, password, key file or passphrase.',
  SSH_AGENT_UNAVAILABLE: 'The SSH agent is unavailable. Start an agent with your key loaded, or choose another method.',
  SECRET_UNAVAILABLE: 'The saved credential is unavailable or belongs to a different target. Enter it again.',
  BUSY: 'This connection is busy with another statement or an open transaction. Nothing was queued or sent.',
  CONFLICT: 'A statement in this transaction failed; only rollback is possible.',
  UNRESOLVED_OUTCOMES: 'Nothing was sent: acknowledge the unknown outcomes listed first. The server may or may not have applied them.',
  OUTCOME_JOURNAL_UNAVAILABLE: 'Nothing was sent: StoneQL cannot save unknown outcomes durably right now. See the notice about the unknown-outcome journal.',
};
// The host's own fixed text is more specific for these codes (it never contains SQL or values).
const hostMessages = new Set(['UNSUPPORTED_SQL', 'CONFLICT', 'UNRESOLVED_OUTCOMES', 'OUTCOME_JOURNAL_UNAVAILABLE']);
/**
 * Renderer waits never end before the native worst case: end_transaction is COMMIT (10 s) +
 * ROLLBACK (2 s) + close (2 s); disconnect releases jobs/leases (5 s) then closes the pool and tunnel.
 */
export const nativeWaitMs: Partial<Record<NativeDatabaseCommand, number>> = { end_transaction: 20_000, disconnect_database: 20_000 };
export type NativeDatabaseCommand = 'connect_direct'|'connect_sqlite'|'pick_sqlite_file'|'read_only_query'|'disconnect_database'|'database_status'|'ssh_host_key'|'ssh_host_key_status'|'trust_ssh_host_key'|'forget_ssh_host_key'|'pick_ssh_key_file'|'cancel_connect'|'store_secret'|'delete_secret'|'pick_export_file'|'start_export'|'export_status'|'cancel_export'|'submit_job'|'fetch_job'|'cancel_job'|'begin_transaction'|'end_transaction'|'list_unresolved_outcomes'|'acknowledge_outcome';
/** Proposed (M005 T2): above the native 60 s secret deadline, so the host answers first. */
export const SECRET_CONNECT_WAIT_MS = 75_000;
/** True when connecting resolves a saved reference natively (a Keychain prompt may appear). */
function usesSavedSecret(payload: Record<string, unknown>): boolean {
  const auth = (payload.ssh as { auth?: Record<string, unknown> } | undefined)?.auth;
  return !!(payload.passwordRef || auth?.passwordRef || auth?.passphraseRef);
}
export function nativeWaitFor(command: NativeDatabaseCommand, payload: Record<string, unknown>): number {
  // Native pickers and the OS credential store may wait for the user; database work is bounded.
  if (['pick_sqlite_file','pick_ssh_key_file','store_secret','delete_secret','trust_ssh_host_key','pick_export_file'].includes(command)) return 120_000;
  if (command === 'connect_direct' && usesSavedSecret(payload)) return SECRET_CONNECT_WAIT_MS;
  return nativeWaitMs[command] ?? 8000;
}
export async function nativeDatabase<T>(command: NativeDatabaseCommand, payload: Record<string,unknown>): Promise<T> {
  if (current.kind !== 'desktop' || !activeInvoker) throw new NativeOperationError('UNSUPPORTED_CAPABILITY', 'Live connections require the native desktop app.');
  const invoker = activeInvoker, sessionId = current.info.sessionId;
  // F-02: a connect that succeeds after the renderer stopped waiting is closed at once.
  const closeLate = command === 'connect_direct' || command === 'connect_sqlite'
    ? (late: T) => {
      const handle = (late as { handle?: unknown })?.handle;
      if (typeof handle === 'string') void call(invoker, 'disconnect_database', { handle, sessionId }, 20_000).catch(() => undefined);
    }
    : undefined;
  try {
    return await call<T,Record<string,unknown>>(invoker, command, {...payload,sessionId}, nativeWaitFor(command, payload), closeLate);
  } catch (error) {
    const code = error && typeof error === 'object' && 'code' in error && typeof error.code === 'string' ? error.code : error instanceof Error && error.message === 'Native host timeout' ? 'DEADLINE_EXCEEDED' : 'INTERNAL';
    const hostMessage = hostMessages.has(code) && error && typeof error === 'object' && 'message' in error && typeof error.message === 'string' ? error.message : undefined;
    throw new NativeOperationError(code, hostMessage || nativeMessages[code] || 'Native operation failed. No mock fallback was used.');
  }
}
