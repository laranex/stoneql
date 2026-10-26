import type { Envelope, QueryRequest, LiveQueryRequest, QueryResult, CellValue, InputValue, ErrorDto } from '../generated/v1.js';
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
void [request, result, defaults, error, invalid, future, liveDefaults, liveExplicit, liveNull];
