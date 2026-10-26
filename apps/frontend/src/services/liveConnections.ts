import type { LiveConnection, LiveConnectionStatus, ResultSet, CellValue, SqliteFileGrant, EngineDto } from '../../../../contracts/generated/v1';
import type { Connection, SqlResult } from '../types';
import { nativeDatabase } from './nativeHost.ts';
const networkEngines: Partial<Record<Connection['engine'], EngineDto>> = { MySQL: 'mysql', MariaDB: 'mariadb', PostgreSQL: 'postgresql' };
export function liveSupported(connection: Connection): boolean {
  return connection.engine === 'SQLite' || (connection.transport.type === 'direct' && !!networkEngines[connection.engine]);
}
/** Opens the native file picker. The renderer receives an opaque grant, never a filesystem path. */
export const pickSqliteFile = () => nativeDatabase<SqliteFileGrant>('pick_sqlite_file', {});
export async function openLive(connection: Connection): Promise<LiveConnection> {
  if (!liveSupported(connection)) throw new Error('Live connections support Direct MySQL, MariaDB and PostgreSQL, or a chosen SQLite file.');
  if (connection.engine === 'SQLite') {
    if (!connection.sqliteGrant) throw new Error('Choose the SQLite file again; file access is granted for this app session only.');
    return nativeDatabase<LiveConnection>('connect_sqlite', { grant: connection.sqliteGrant });
  }
  return nativeDatabase<LiveConnection>('connect_direct', {
    engine: networkEngines[connection.engine], host: connection.host, port: connection.port,
    username: connection.user, password: connection.password || '', database: connection.database,
    rootCertificatePem: connection.rootCertificatePem || null,
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
