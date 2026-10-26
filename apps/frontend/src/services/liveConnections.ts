import type { LiveConnection, LiveConnectionStatus, ResultSet, CellValue, SqliteFileGrant, EngineDto, SshAuthRequest, SshTunnelRequest, SshHostKey, SshKeyFileGrant, SecretPurpose, SecretTarget, SecretReference, CancelConnectResult } from '../../../../contracts/generated/v1';
import type { Connection, SqlResult } from '../types';
import { nativeDatabase } from './nativeHost.ts';
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
export async function openLive(connection: Connection, attemptId?: string): Promise<LiveConnection> {
  if (!liveSupported(connection)) throw new Error('Live connections support MySQL, MariaDB and PostgreSQL over Direct or SSH, or a chosen SQLite file.');
  if (connection.engine === 'SQLite') {
    if (!connection.sqliteGrant) throw new Error('Choose the SQLite file again; file access is granted for this app session only.');
    return nativeDatabase<LiveConnection>('connect_sqlite', { grant: connection.sqliteGrant });
  }
  const ssh = sshTunnel(connection);
  return nativeDatabase<LiveConnection>('connect_direct', {
    engine: networkEngines[connection.engine], host: connection.host, port: connection.port,
    username: connection.user, password: connection.password || '', database: connection.database,
    rootCertificatePem: connection.rootCertificatePem || null,
    ...(!connection.password && connection.passwordRef ? { passwordRef: connection.passwordRef } : {}),
    ...(ssh ? { ssh } : {}),
    ...(attemptId ? { attemptId } : {}),
  });
}
export const closeLive = (handle: string) => nativeDatabase<LiveConnectionStatus>('disconnect_database',{handle});
export const liveStatus = (handle: string) => nativeDatabase<LiveConnectionStatus>('database_status',{handle});
export async function queryLive(handle: string, sql: string): Promise<SqlResult> {
  // Row/byte limits are omitted so the native runtime applies and enforces its own maximums.
  const raw = await nativeDatabase<ResultSet>('read_only_query',{handle,sql});
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
