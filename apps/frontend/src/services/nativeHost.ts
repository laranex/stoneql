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
async function call<T, P>(invoker: NativeInvoker, command: string, payload: P): Promise<T> {
  const request: Envelope<P> = { version: 1, requestId: `desktop_${++sequence}`, payload };
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const reply = await Promise.race([
      invoker.invoke<Envelope<T>>(command, { message: JSON.stringify(request) }),
      new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error('Native host timeout')), 2000); }),
    ]);
    if (reply.version !== 1 || reply.requestId !== request.requestId) throw new Error('Native protocol mismatch');
    return reply.payload;
  } finally { if (timer) clearTimeout(timer); }
}
export function hostState(): HostState { return current; }
export async function bootstrapNativeHost(global: unknown = globalThis): Promise<HostState> {
  const invoker = invokerFrom(global);
  if (!invoker) return (current = {kind: 'browser'});
  try {
    const payload: HostInfoRequest = {nonce: `bootstrap_${Date.now()}`};
    const info = await call<HostInfo, HostInfoRequest>(invoker, 'host_info', payload);
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
