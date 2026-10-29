import type { Envelope, QueryRequest, LiveQueryRequest, QueryResult, CellValue, InputValue, ErrorDto, SqliteConnectionRequest, SqliteFileGrant, DirectConnectionRequest, SshTunnelRequest, StoreSecretRequest, SubmitJobRequest, JobUpdate, TransactionEnd, EndTransactionRequest, UnresolvedOutcomes, AcknowledgeOutcomeRequest } from '../generated/v1.js';
const value: CellValue = { kind: 'integer', text: '18446744073709551615' };
const request: Envelope<QueryRequest> = { version: 1, requestId: 'r', payload: { connectionId: 'c', database: null, sql: "SELECT 'unchanged'", maxRows: 200, maxBytes: 262144 } };
const result: Envelope<QueryResult> = { version: 1, requestId: 'r', payload: { resultSets: [{ index: 0, columns: [{name: 'n', databaseType: 'BIGINT', nullable: null}], rows: [[value]], affectedRows: '0', truncated: false }] } };
const defaults: InputValue[] = [{kind: 'default'}, {kind: 'null'}, {kind: 'value', value: {kind: 'text', text: ''}}];
const error: ErrorDto = {code: 'CONNECTION_LOST', message: 'Connection lost.', retryable: false, outcome: 'unknown'};
// @ts-expect-error exact integers must not cross the wire as JS numbers
const invalid: CellValue = {kind: 'integer', text: 9007199254740993};
// @ts-expect-error protocol versions are literal and unsupported versions cannot type-check
const future: Envelope<QueryRequest> = {...request, version: 2};
const liveDefaults: LiveQueryRequest = {sessionId: 's', handle: 'h', sql: 'SELECT 1'};
const liveExplicit: LiveQueryRequest = {...liveDefaults, maxRows: 10, maxBytes: 1024};
// @ts-expect-error omitted limits are absent, not null; the native decoder rejects null
const liveNull: LiveQueryRequest = {...liveDefaults, maxRows: null};
const sqliteOpen: SqliteConnectionRequest = {sessionId: 's', grant: 'g'};
// @ts-expect-error SQLite connections name an opaque native grant, never a renderer path
const sqlitePath: SqliteConnectionRequest = {sessionId: 's', path: '/etc/passwd'};
const pickerCancelled: SqliteFileGrant = {grant: null, fileName: null};
const tunnel: SshTunnelRequest = {host: 'bastion', port: 22, username: 'deploy', hostKey: 'ssh-ed25519 AAAA', auth: {method: 'key_file', grant: 'g', passphraseRef: 'r'}};
const overSsh: DirectConnectionRequest = {sessionId: 's', engine: 'mysql', host: 'db.internal', port: 3306, username: 'u', password: '', database: 'd', rootCertificatePem: null, passwordRef: 'r', ssh: tunnel, attemptId: 'a'};
// @ts-expect-error private key material never crosses IPC; the renderer names a native grant
const pastedKey: SshTunnelRequest = {...tunnel, auth: {method: 'key_file', privateKey: '-----BEGIN OPENSSH PRIVATE KEY-----'}};
// @ts-expect-error an unenrolled host key is omitted, not null
const nullHostKey: SshTunnelRequest = {...tunnel, hostKey: null};
const remember: StoreSecretRequest = {sessionId: 's', purpose: 'database_password', target: {engine: 'mysql', host: 'db', port: 3306, username: 'u'}, secret: 'x'};
const hostKeyChanged: ErrorDto = {...error, code: 'SSH_HOST_KEY_CHANGED'};
const job: SubmitJobRequest = {sessionId: 's', handle: 'h', sql: 'UPDATE t SET a = 1', leaseId: 'l'};
// @ts-expect-error omitted job options are absent, not null
const nullLease: SubmitJobRequest = {...job, leaseId: null};
const page: JobUpdate = {jobId: 'j', state: 'finished', outcome: 'unknown', cancellation: 'confirmed', serverCancelSent: true, deadlineExceeded: false, statements: [], batch: null, lease: null, done: true};
const interrupted: JobUpdate = {...page, statements: [{index: 0, status: 'unknown', inTransaction: false, affectedRows: '0', resultSets: 1, failure: null, resultsWithheld: true}]};
// @ts-expect-error resultsWithheld is required
const unflagged: JobUpdate = {...page, statements: [{index: 0, status: 'unknown', inTransaction: false, affectedRows: '0', resultSets: 1, failure: null}]};
// @ts-expect-error jobs are never queued
const queued: JobUpdate = {...page, state: 'queued'};
// @ts-expect-error a cancelled write is unknown; cancellation is a separate fact
const writeCancelled: JobUpdate = {...page, cancellation: 'cancelled'};
const ended: TransactionEnd = {outcome: 'unknown', reason: 'connection_lost', commitSent: false, rollbackSent: false};
// @ts-expect-error rollbackSent is required
const endedWithoutRollbackFact: TransactionEnd = {outcome: 'unknown', reason: 'closed', commitSent: false};
// @ts-expect-error savepoints are not offered
const savepoint: EndTransactionRequest = {sessionId: 's', leaseId: 'l', action: 'savepoint'};
const busy: ErrorDto = {...error, code: 'BUSY'};
const unresolvedRefusal: ErrorDto = {...error, code: 'UNRESOLVED_OUTCOMES', outcome: 'rejected'};
const unresolved: UnresolvedOutcomes = {limit: 16, pending: 1, outcomes: [
  {outcomeId: 'o1', database: 'app', secondsAgo: 4, transaction: ended, job: null},
  {outcomeId: 'o2', database: 'app', secondsAgo: 9, transaction: null, job: {statementCount: 2, unknownStatement: 1, inTransaction: false, cancellation: 'requested', serverCancelSent: true, deadlineExceeded: false}},
]};
const acknowledge: AcknowledgeOutcomeRequest = {sessionId: 's', outcomeId: 'o1'};
// @ts-expect-error acknowledgement never sets or changes an outcome
const resolveByAck: AcknowledgeOutcomeRequest = {sessionId: 's', outcomeId: 'o1', outcome: 'rolled_back'};
// @ts-expect-error a listed outcome carries no SQL text
const withSql: UnresolvedOutcomes = {limit: 16, pending: 0, outcomes: [{...unresolved.outcomes[0]!, sql: 'UPDATE t SET a = 1'}]};
// @ts-expect-error the host always reports detached jobs still pending
const withoutPending: UnresolvedOutcomes = {limit: 16, outcomes: []};
void [job, nullLease, page, interrupted, unflagged, queued, writeCancelled, ended, endedWithoutRollbackFact, savepoint, busy, unresolvedRefusal, unresolved, acknowledge, resolveByAck, withSql, withoutPending];
void [request, result, defaults, error, invalid, future, liveDefaults, liveExplicit, liveNull, sqliteOpen, sqlitePath, pickerCancelled, overSsh, pastedKey, nullHostKey, remember, hostKeyChanged];
