import type { LiveConnection, LiveConnectionStatus, ResultSet, CellValue, SqliteFileGrant, EngineDto, SshAuthRequest, SshTunnelRequest, SshHostKey, SshKeyFileGrant, SecretPurpose, SecretTarget, SecretReference, CancelConnectResult, AccessModeDto, JobAccepted, JobUpdate, CancelJobResult, TransactionLease, TransactionEnd, TransactionActionDto, Column as ColumnDto } from '../../../../contracts/generated/v1';
import type { Connection, SqlResult } from '../types';
import { nativeDatabase, NativeOperationError } from './nativeHost.ts';
const networkEngines: Partial<Record<Connection['engine'], EngineDto>> = { MySQL: 'mysql', MariaDB: 'mariadb', PostgreSQL: 'postgresql' };
/** SQLite opens a natively granted file; network engines use Direct or an SSH tunnel. */
export function liveSupported(connection: Connection): boolean {
  if (connection.engine === 'SQLite') return connection.transport.type === 'direct';
  return ['direct', 'ssh'].includes(connection.transport.type) && !!networkEngines[connection.engine];
}
/** Opens the native file picker. The renderer receives an opaque grant, never a filesystem path. */
export const pickSqliteFile = () => nativeDatabase<SqliteFileGrant>('pick_sqlite_file', {});
/** Native SSH private-key picker: an opaque session grant; key bytes never reach the renderer. */
export const pickSshKeyFile = () => nativeDatabase<SshKeyFileGrant>('pick_ssh_key_file', {});
/** Presented host key for explicit user verification. Scanning never trusts it. */
export const scanHostKey = (host: string, port: number) => nativeDatabase<SshHostKey>('ssh_host_key', { host, port });
export const cancelConnect = (attemptId: string) => nativeDatabase<CancelConnectResult>('cancel_connect', { attemptId });
export async function storeSecret(purpose: SecretPurpose, connection: Connection, secret: string): Promise<string> {
  return (await nativeDatabase<SecretReference>('store_secret', { purpose, target: secretTarget(connection), secret })).reference;
}
export const deleteSecret = (reference: string) => nativeDatabase<SecretReference>('delete_secret', { reference });
/** The target a saved secret is bound to; the host refuses it for any other target or route. */
export function secretTarget(connection: Connection): SecretTarget {
  const transport = connection.transport;
  return {
    engine: networkEngines[connection.engine]!, host: connection.host, port: connection.port, username: connection.user,
    ...(transport.type === 'ssh' ? { ssh: { host: transport.host, port: transport.port, username: transport.user } } : {}),
  };
}
function sshTunnel(connection: Connection): SshTunnelRequest | undefined {
  const transport = connection.transport;
  if (transport.type !== 'ssh') return undefined;
  let auth: SshAuthRequest;
  if (transport.auth === 'agent') auth = { method: 'agent' };
  else if (transport.auth === 'password') {
    auth = transport.password ? { method: 'password', password: transport.password } : transport.passwordRef ? { method: 'password', passwordRef: transport.passwordRef } : { method: 'password' };
  } else {
    if (!transport.keyGrant) throw new Error('Choose the SSH key file again; key access is granted for this app session only.');
    auth = { method: 'key_file', grant: transport.keyGrant, ...(transport.passphrase ? { passphrase: transport.passphrase } : transport.passphraseRef ? { passphraseRef: transport.passphraseRef } : {}) };
  }
  return { host: transport.host, port: transport.port, username: transport.user, ...(transport.hostKey ? { hostKey: transport.hostKey } : {}), auth };
}
/** Read-only unless `access` is the explicit, per-session `read_write` opt-in. */
export async function openLive(connection: Connection, attemptId?: string, access: AccessModeDto = 'read_only'): Promise<LiveConnection> {
  if (!liveSupported(connection)) throw new Error('Live connections support MySQL, MariaDB and PostgreSQL over Direct or SSH, or a chosen SQLite file.');
  const mode = access === 'read_write' ? { access } : {};
  if (connection.engine === 'SQLite') {
    if (!connection.sqliteGrant) throw new Error('Choose the SQLite file again; file access is granted for this app session only.');
    return nativeDatabase<LiveConnection>('connect_sqlite', { grant: connection.sqliteGrant, ...mode });
  }
  const ssh = sshTunnel(connection);
  return nativeDatabase<LiveConnection>('connect_direct', {
    engine: networkEngines[connection.engine], host: connection.host, port: connection.port,
    username: connection.user, password: connection.password || '', database: connection.database,
    rootCertificatePem: connection.rootCertificatePem || null,
    ...(!connection.password && connection.passwordRef ? { passwordRef: connection.passwordRef } : {}),
    ...(ssh ? { ssh } : {}),
    ...(attemptId ? { attemptId } : {}),
    ...mode,
  });
}
export const closeLive = (handle: string) => nativeDatabase<LiveConnectionStatus>('disconnect_database',{handle});
export const liveStatus = (handle: string) => nativeDatabase<LiveConnectionStatus>('database_status',{handle});
export async function queryLive(handle: string, sql: string): Promise<SqlResult> {
  // Row/byte limits are omitted so the native runtime applies and enforces its own maximums.
  return toSqlResult(await nativeDatabase<ResultSet>('read_only_query',{handle,sql}));
}
function toSqlResult(raw: ResultSet): SqlResult {
  const columns=raw.columns.map((_,index)=>`__live_${index}`);
  return {
    columns, columnLabels:Object.fromEntries(columns.map((key,index)=>[key,raw.columns[index]!.name])),
    columnTypes:Object.fromEntries(columns.map((key,index)=>[key,raw.columns[index]!.databaseType])),
    rows:raw.rows.map(row=>Object.fromEntries(columns.map((key,index)=>[key,displayValue(row[index]!)]))), raw,truncated:raw.truncated,
  };
}
function displayValue(value: CellValue): string|null {
  switch(value.kind){
    case 'null': return null;
    case 'boolean': return value.value ? 'true' : 'false';
    case 'binary': return `0x${value.hex}`;
    case 'unknown': return `[${value.databaseType}] 0x${value.hex}`;
    default:return value.text;
  }
}

/** One result set of a read-write job, labelled with the statement that produced it. */
export type LiveResultSet = SqlResult & { statement: number; result: number };
export interface LiveJobResult {
  outcome: NonNullable<JobUpdate['outcome']>;
  cancellation: JobUpdate['cancellation'];
  serverCancelSent: boolean;
  deadlineExceeded: boolean;
  statements: JobUpdate['statements'];
  lease: JobUpdate['lease'];
  resultSets: LiveResultSet[];
}
/**
 * Runs a read-write script as a native job and collects its bounded pages. The job id is
 * reported early so the caller can request cancellation. Nothing is retried.
 */
export async function runLiveJob(handle: string, sql: string, options: { leaseId?: string; onJob?: (jobId: string) => void } = {}): Promise<LiveJobResult> {
  const accepted = await nativeDatabase<JobAccepted>('submit_job', { handle, sql, ...(options.leaseId ? { leaseId: options.leaseId } : {}) });
  options.onJob?.(accepted.jobId);
  const sets = new Map<string, { statement: number; result: number; columns: ColumnDto[]; rows: CellValue[][]; truncated: boolean }>();
  for (;;) {
    let update: JobUpdate;
    try { update = await nativeDatabase<JobUpdate>('fetch_job', { jobId: accepted.jobId, waitMs: 1000 }); }
    catch (error) {
      // The job was accepted, so statements may have been sent: never report a failure or retry.
      const code = error instanceof NativeOperationError ? error.code : 'INTERNAL';
      throw new NativeOperationError(code, `Outcome unknown: the job was accepted but its progress could not be read (${(error as Error).message}) It may still be running natively; StoneQL never retries. Verify on the server.`);
    }
    const batch = update.batch;
    if (batch) {
      const key = `${batch.statement}:${batch.result}`;
      const set = sets.get(key) ?? { statement: batch.statement, result: batch.result, columns: [], rows: [], truncated: false };
      if (batch.columns) set.columns = batch.columns;
      set.rows.push(...batch.rows);
      set.truncated ||= batch.truncated;
      sets.set(key, set);
    }
    if (update.done) {
      // B-11: rows read after an interrupt were discarded natively; never show them as complete.
      const withheld = new Set(update.statements.filter(s => s.resultsWithheld).map(s => s.index));
      for (const set of sets.values()) set.truncated ||= withheld.has(set.statement);
      return {
        outcome: update.outcome ?? 'unknown', cancellation: update.cancellation, serverCancelSent: update.serverCancelSent,
        deadlineExceeded: update.deadlineExceeded, statements: update.statements, lease: update.lease,
        resultSets: [...sets.values()].map(set => ({
          ...toSqlResult({ index: set.result, columns: set.columns, rows: set.rows, affectedRows: '0', truncated: set.truncated }),
          statement: set.statement, result: set.result,
        })),
      };
    }
  }
}
export const cancelLiveJob = (jobId: string) => nativeDatabase<CancelJobResult>('cancel_job', { jobId });
export const beginLiveTransaction = (handle: string) => nativeDatabase<TransactionLease>('begin_transaction', { handle });
/** Text for a COMMIT/ROLLBACK whose reply did not arrive in time: never success, never rollback. */
export function endTimeoutText(action: TransactionActionDto): string {
  return action === 'commit'
    ? 'Commit outcome unknown: COMMIT may have been sent and was not confirmed. Verify on the server; StoneQL never retries.'
    : 'Rollback outcome unknown: ROLLBACK may have been sent and was not confirmed, so it is never claimed rolled back. Verify on the server; StoneQL never retries.';
}
export async function endLiveTransaction(leaseId: string, action: TransactionActionDto): Promise<TransactionEnd> {
  try { return await nativeDatabase<TransactionEnd>('end_transaction', { leaseId, action }); }
  catch (error) {
    if (error instanceof NativeOperationError && error.code === 'DEADLINE_EXCEEDED') throw new NativeOperationError(error.code, endTimeoutText(action));
    throw error;
  }
}
export type OutcomeNotice = { kind: 'success' | 'warning' | 'error'; text: string };
/** Transaction end as the server confirmed it; anything unconfirmed is unknown. */
export function describeTransactionEnd(end: TransactionEnd): OutcomeNotice {
  const reason = end.reason === 'expired' ? ' The transaction had expired after being idle.' : end.reason === 'connection_lost' ? ' Its connection was lost.' : end.reason === 'closed' ? ' Its connection was closed.' : '';
  if (end.outcome === 'committed') return { kind: 'success', text: `Transaction committed (confirmed by the server).${reason}` };
  if (end.outcome === 'rolled_back') return { kind: 'success', text: `Transaction rolled back (confirmed by the server).${reason}` };
  // commitSent/rollbackSent mean "attempted": the bytes may not have reached the server.
  const sent = end.commitSent && end.rollbackSent ? 'COMMIT and then ROLLBACK may have been sent; neither was confirmed.'
    : end.commitSent ? 'COMMIT may have been sent and was not confirmed.'
    : end.rollbackSent ? 'ROLLBACK may have been sent and was not confirmed.'
    : 'Neither COMMIT nor ROLLBACK was sent or confirmed.';
  return { kind: 'error', text: `Transaction outcome unknown.${reason} ${sent} Verify on the server; StoneQL never retries.` };
}
/**
 * Job outcome text derived only from the job/statement outcome taxonomy. "Executed
 * successfully" appears only when the job outcome is `succeeded`, which the native runtime
 * reports only after checking the session's transaction state (autocommit on and no open
 * transaction outside a lease; the transaction still open inside one).
 */
export function describeJob(job: LiveJobResult, inTransaction: boolean): OutcomeNotice {
  const notice = describeJobOutcome(job, inTransaction);
  const withheld = job.statements.filter(s => s.resultsWithheld).map(s => s.index + 1);
  if (!withheld.length) return notice;
  return { ...notice, text: `${notice.text} Results of statement${withheld.length === 1 ? '' : 's'} ${withheld.join(', ')} after the interrupt were not delivered; the rows shown are incomplete.` };
}
function describeJobOutcome(job: LiveJobResult, inTransaction: boolean): OutcomeNotice {
  const count = job.statements.length;
  const where = (index: number) => `statement ${index + 1} of ${count}`;
  const unknown = job.statements.find((s) => s.status === 'unknown');
  const failed = job.statements.find((s) => s.status === 'failed');
  const ran = job.statements.filter((s) => s.status !== 'not_run').length;
  if (job.outcome === 'succeeded')
    return { kind: 'success', text: `${count} statement${count === 1 ? '' : 's'} executed successfully (${inTransaction ? 'in the open transaction' : 'autocommit'}).` };
  if (job.outcome === 'unknown' && !unknown)
    return { kind: 'error', text: `Outcome unknown: the session's transaction state after the job did not match ${inTransaction ? 'the open transaction (it may have been committed or rolled back)' : 'autocommit (a transaction may have been left open and discarded)'}. Verify on the server; StoneQL never retries.${inTransaction ? ' The transaction can only be ended without COMMIT or ROLLBACK.' : ''}` };
  if (job.outcome === 'unknown' || unknown) {
    const at = unknown ? ` at ${where(unknown.index)}` : '';
    const cancel = job.cancellation === 'confirmed' ? ' (cancellation confirmed; effects not)' : job.cancellation === 'requested' ? ' (cancellation requested, not confirmed; it may still be running on the server)' : '';
    return { kind: 'error', text: `Outcome unknown${at}${cancel}${job.deadlineExceeded ? ' after the deadline' : ''}. Verify on the server; StoneQL never retries.${unknown?.inTransaction ? ' Roll back the transaction to discard its work.' : ''}` };
  }
  if (job.outcome === 'failed' && !failed && ran === 0)
    return { kind: 'error', text: "The job's connection could not be opened; nothing was sent." };
  if (job.outcome === 'failed' && failed)
    return { kind: 'error', text: `The database rejected ${where(failed.index)}${failed.failure?.constraint ? ` (${failed.failure.constraint.replace('_', ' ')} constraint)` : ''}${failed.failure?.sqlState ? ` · SQLSTATE ${failed.failure.sqlState}` : ''}. ${failed.index ? `Earlier statements ${failed.inTransaction ? 'remain in the open transaction' : 'were committed'}; ` : ''}later statements were not sent.` };
  if (job.outcome === 'cancelled')
    return { kind: 'warning', text: ran ? `Stopped before ${where(ran)}; nothing after it was sent.` : 'Cancelled before any statement was sent.' };
  return { kind: 'error', text: 'The job did not succeed; its outcome is unknown. Verify on the server; StoneQL never retries.' };
}
/** UX safeguard only (not authorization): statements that change or remove data or objects. */
export function writeKeywords(sql: string): string[] {
  const code = sql.replace(/'(?:[^'\\]|\\.|'')*'|"(?:[^"\\]|\\.)*"|`[^`]*`|--[^\n]*|#[^\n]*|\/\*(?!!)[\s\S]*?\*\//g, ' ');
  return [...new Set((code.match(/\b(DELETE|UPDATE|DROP|TRUNCATE|ALTER|REPLACE|RENAME|GRANT|REVOKE)\b/gi) || []).map(word => word.toUpperCase()))];
}
