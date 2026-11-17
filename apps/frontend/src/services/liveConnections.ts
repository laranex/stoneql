import type { LiveConnection, LiveConnectionStatus, ResultSet, CellValue, SqliteFileGrant, EngineDto, SshAuthRequest, SshTunnelRequest, SshHostKey, SshHostKeyStatus, SshKeyFileGrant, ExportFileGrant, ImportFileGrant, SqlImportAccepted, SqlImportStatus, TransferAccepted, TransferStatus, TransferFormatDto, SecretPurpose, SecretTarget, SecretReference, CancelConnectResult, AccessModeDto, JobAccepted, JobUpdate, CancelJobResult, TransactionLease, TransactionEnd, TransactionActionDto, UnresolvedOutcome, UnresolvedOutcomes, JournalProblemDto, Column as ColumnDto } from '../../../../contracts/generated/v1';
import type { Connection, SqlResult } from '../types';
import { hostState, nativeDatabase, NativeOperationError } from './nativeHost.ts';
const networkEngines: Partial<Record<Connection['engine'], EngineDto>> = { MySQL: 'mysql', MariaDB: 'mariadb', PostgreSQL: 'postgresql' };
/** SQLite opens a natively granted file; network engines use Direct or an SSH tunnel. */
export function liveSupported(connection: Connection): boolean {
  // SQLite is excluded from release builds (A5); the host reports whether this build has it.
  if (connection.engine === 'SQLite') { const host = hostState(); return connection.transport.type === 'direct' && host.kind === 'desktop' && host.info.sqlite === true; }
  return ['direct', 'ssh'].includes(connection.transport.type) && !!networkEngines[connection.engine];
}
/** Opens the native file picker. The renderer receives an opaque grant, never a filesystem path. */
export const pickSqliteFile = () => nativeDatabase<SqliteFileGrant>('pick_sqlite_file', {});
/** Native SSH private-key picker: an opaque session grant; key bytes never reach the renderer. */
export const pickSshKeyFile = () => nativeDatabase<SshKeyFileGrant>('pick_ssh_key_file', {});
/** Presented host key for explicit user verification. Scanning never trusts it. */
export const scanHostKey = (host: string, port: number) => nativeDatabase<SshHostKey>('ssh_host_key', { host, port });
/** Native trust state of an SSH endpoint (no network). */
export const hostKeyStatus = (host: string, port: number) => nativeDatabase<SshHostKeyStatus>('ssh_host_key_status', { host, port });
/** Enrolls the key a native scan presented; the host asks for confirmation in its own dialog. */
export const trustScannedHostKey = (host: string, port: number, fingerprint: string) => nativeDatabase<SshHostKeyStatus>('trust_ssh_host_key', { host, port, fingerprint });
export const forgetTrustedHostKey = (host: string, port: number) => nativeDatabase<SshHostKeyStatus>('forget_ssh_host_key', { host, port });
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
  return { host: transport.host, port: transport.port, username: transport.user, auth };
}
/** Hosts that mean this computer; only these may turn TLS off (the host enforces the same rule). */
export function isLoopbackHost(host: string): boolean {
  return host.toLowerCase() === 'localhost' || host === '127.0.0.1' || host === '::1';
}
/** "Use TLS" is offered only for a live direct MySQL/MariaDB connection to this computer. */
export function tlsOptional(connection: Connection): boolean {
  return connection.mode === 'live' && (connection.engine === 'MySQL' || connection.engine === 'MariaDB')
    && connection.transport.type === 'direct' && isLoopbackHost(connection.host);
}
/** Verified TLS unless TLS was explicitly turned off for this computer. */
export function usesTls(connection: Connection): boolean {
  return !tlsOptional(connection) || connection.tls !== false;
}
/** Read-only unless `access` is `read_write` (a Direct profile not marked read-only, A15). */
export async function openLive(connection: Connection, attemptId?: string, access: AccessModeDto = 'read_only'): Promise<LiveConnection> {
  if (!liveSupported(connection)) throw new Error('Live connections support MySQL, MariaDB and PostgreSQL over Direct or SSH, or a chosen SQLite file.');
  const mode = access === 'read_write' ? { access } : {};
  if (connection.engine === 'SQLite') {
    if (!connection.sqliteGrant) throw new Error('Choose the SQLite file again; file access is granted for this app session only.');
    return nativeDatabase<LiveConnection>('connect_sqlite', { grant: connection.sqliteGrant, ...mode });
  }
  const ssh = sshTunnel(connection), tls = usesTls(connection);
  try {
    return await nativeDatabase<LiveConnection>('connect_direct', {
    engine: networkEngines[connection.engine], host: connection.host, port: connection.port,
    username: connection.user, password: connection.password || '', database: connection.database,
    // TLS off sends no CA (a CA has no meaning without TLS) and never falls back the other way.
    rootCertificatePem: tls ? connection.rootCertificatePem || null : null,
    ...(tls ? {} : { tls: false }),
    ...(!connection.password && connection.passwordRef ? { passwordRef: connection.passwordRef } : {}),
    ...(ssh ? { ssh } : {}),
    ...(attemptId ? { attemptId } : {}),
    ...mode,
    });
  } catch (error) {
    // F-02: the renderer gave up; ask the host to stop the attempt too (best effort, once).
    if (attemptId && error instanceof NativeOperationError && error.code === 'DEADLINE_EXCEEDED') await cancelConnect(attemptId).catch(() => undefined);
    throw error;
  }
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
/**
 * Unknown outcomes the native host recorded and the user has not acknowledged: those from before
 * a reload and, from its durable journal, those from before a restart (quit or crash).
 */
export const listUnresolvedOutcomes = () => nativeDatabase<UnresolvedOutcomes>('list_unresolved_outcomes', {});
/** Hides one listed outcome. It never changes what was recorded; nothing is sent to a database. */
export const acknowledgeOutcome = (outcomeId: string) => nativeDatabase<UnresolvedOutcomes>('acknowledge_outcome', { outcomeId });
const engineNames: Record<EngineDto, string> = { mysql: 'MySQL', mariadb: 'MariaDB', postgresql: 'PostgreSQL', sqlite: 'SQLite' };
/** Plain text for a listed unknown outcome; never claims success or rollback. */
export function describeUnresolvedOutcome(outcome: UnresolvedOutcome): string {
  if (outcome.journalDamage) {
    const damage = outcome.journalDamage;
    const lost = damage.unreadableRecords === null ? 'How many recorded outcomes could not be read is unknown.'
      : damage.unreadableRecords === 0 ? 'Every recorded outcome in it was recovered and is listed.'
      : `${damage.unreadableRecords} recorded outcome${damage.unreadableRecords === 1 ? '' : 's'} could not be read and ${damage.unreadableRecords === 1 ? 'is' : 'are'} not listed.`;
    const kept = damage.preservedAs ? `The damaged file was kept unchanged as “${damage.preservedAs}” in the outcome-journal folder.` : 'The damaged file could not be set aside and was left unchanged.';
    return `The unknown-outcome journal could not be read intact when StoneQL started. ${lost} ${kept} Verify recent live writes on your servers. New live writes stay refused until you acknowledge this.`;
  }
  const engine = outcome.engine ? `${engineNames[outcome.engine]} ` : '';
  const where = outcome.database ? ` on ${engine}“${outcome.database}”` : engine ? ` on ${engine.trim()}` : '';
  const base = 'Outcome unknown — the server may or may not have applied it.';
  if (outcome.transaction && outcome.appExited) {
    const end = outcome.transaction;
    const sent = end.commitSent ? 'COMMIT may have been sent and was not confirmed.'
      : end.rollbackSent ? 'ROLLBACK may have been sent and was not confirmed; StoneQL never sent COMMIT.'
      : "StoneQL never sent COMMIT; the server normally discards an open transaction's work when its session ends, but this was not confirmed.";
    return `Transaction${where} was still open when StoneQL quit or crashed. ${base} ${sent} Verify on the server; StoneQL never retries.`;
  }
  if (outcome.transaction) {
    const end = outcome.transaction;
    const why = end.reason === 'expired' ? 'expired after being idle' : end.reason === 'connection_lost' ? 'lost its connection' : end.reason === 'closed' ? 'was closed (disconnect, reload or quit)' : 'was ended';
    const sent = end.commitSent && end.rollbackSent ? 'COMMIT and then ROLLBACK may have been sent; neither was confirmed.'
      : end.commitSent ? 'COMMIT may have been sent and was not confirmed.'
      : end.rollbackSent ? 'ROLLBACK may have been sent and was not confirmed.'
      : 'Neither COMMIT nor ROLLBACK was confirmed.';
    return `Transaction${where} ${why}. ${base} ${sent} Verify on the server; StoneQL never retries.`;
  }
  const job = outcome.job!;
  if (outcome.appExited) {
    const at = job.unknownStatement === null ? '' : ` Statement ${job.unknownStatement + 1} of ${job.statementCount} may have been sent${job.unknownStatement > 0 ? ' (the statements before it were sent)' : ''}; later ones were not.`;
    return `SQL job${where}${job.inTransaction ? ' inside a transaction' : ''} was running when StoneQL quit or crashed. ${base}${at} Verify on the server; StoneQL never retries.`;
  }
  const at = job.unknownStatement === null ? ' (the session state after it did not match)' : ` at statement ${job.unknownStatement + 1} of ${job.statementCount}`;
  const how = job.cancellation === 'requested' ? ' Cancellation was requested, not confirmed; it may still be running on the server.' : job.cancellation === 'confirmed' ? ' It was interrupted.' : '';
  return `SQL job${where}${at}${job.inTransaction ? ' inside a transaction' : ''}${job.deadlineExceeded ? ' after the deadline' : ''}. ${base}${how} Verify on the server; StoneQL never retries.`;
}
/** Why unknown outcomes cannot be saved durably; new live writes are refused meanwhile. */
export function describeJournalProblem(problem: JournalProblemDto): string {
  const why: Record<JournalProblemDto, string> = {
    not_opened: 'StoneQL has not opened its unknown-outcome journal.',
    unavailable: 'The unknown-outcome journal folder could not be created, read or locked.',
    insecure: 'The unknown-outcome journal is a symbolic link, is not owned by you, or can be read by other users, so StoneQL does not use it.',
    in_use: 'Another running StoneQL is using the unknown-outcome journal.',
    unsupported_version: 'The unknown-outcome journal was written by a newer StoneQL and was left untouched.',
    damaged_not_preserved: 'The unknown-outcome journal is damaged and could not be set aside; it was left untouched.',
    write_failed: 'The unknown-outcome journal could not be saved (for example, the disk is full).',
  };
  return `${why[problem]} New live writes and transactions are refused until unknown outcomes can be saved again; read-only queries still work. Nothing was deleted.`;
}
export type OutcomeNotice = { kind: 'success' | 'warning' | 'error'; text: string };
/**
 * Transaction end as the server confirmed it; anything unconfirmed is unknown. `requested`: the
 * user's action, so a COMMIT the host refused to send (its unknown outcome could not be saved
 * durably first) and replaced by a confirmed rollback is not mistaken for a plain rollback.
 */
export function describeTransactionEnd(end: TransactionEnd, requested?: TransactionActionDto): OutcomeNotice {
  const reason = end.reason === 'expired' ? ' The transaction had expired after being idle.' : end.reason === 'connection_lost' ? ' Its connection was lost.' : end.reason === 'closed' ? ' Its connection was closed.' : '';
  if (requested === 'commit' && end.outcome === 'rolled_back' && !end.commitSent) return { kind: 'error', text: `COMMIT was not sent: StoneQL could not save its possible unknown outcome durably first. The transaction was rolled back instead (confirmed by the server); nothing was committed.${reason}` };
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
  // deadlineExceeded on a failed job: the server's own statement timeout (PostgreSQL, set below
  // the client deadline) ended the statement with an error; the client sent no interrupt.
  if (job.outcome === 'failed' && failed && job.deadlineExceeded)
    return { kind: 'error', text: `The server stopped ${where(failed.index)} at the statement time limit${failed.failure?.sqlState ? ` · SQLSTATE ${failed.failure.sqlState}` : ''}. ${failed.index ? `Earlier statements ${failed.inTransaction ? 'remain in the open transaction' : 'were committed'}; later` : 'Later'} statements were not sent.` };
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

/** M007: native save dialog; the renderer receives a one-shot grant, never a path. */
export const pickExportFile = (suggestedName: string, format: TransferFormatDto) => nativeDatabase<ExportFileGrant>('pick_export_file', { suggestedName, format });
export const startExport = (handle: string, grant: string, table: string, format: TransferFormatDto, structure: boolean) =>
  nativeDatabase<TransferAccepted>('start_export', { handle, grant, table, format, structure, data: true });
export const exportStatus = (transferId: string) => nativeDatabase<TransferStatus>('export_status', { transferId });
export const cancelExport = (transferId: string) => nativeDatabase<TransferStatus>('cancel_export', { transferId });
/** M007 d5 SQL import (MySQL/MariaDB read-write sessions): native open grant, then the host reads,
 *  validates the whole file and runs it from that snapshot; cancel with `cancelLiveJob`. */
export const pickImportFile = () => nativeDatabase<ImportFileGrant>('pick_import_file', {});
export const startSqlImport = (handle: string, grant: string) => nativeDatabase<SqlImportAccepted>('start_sql_import', { handle, grant });
export const sqlImportStatus = (importId: string) => nativeDatabase<SqlImportStatus>('sql_import_status', { importId });
