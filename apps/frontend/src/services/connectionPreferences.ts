import type { Connection, Engine, EnvironmentTag, ConnectionGroup } from '../types/index.ts';
export const connectionColors = [
  { name: 'Slate', value: '#687b91' },
  { name: 'Blue', value: '#557fa6' },
  { name: 'Teal', value: '#378777' },
  { name: 'Olive', value: '#87916b' },
  { name: 'Amber', value: '#bc8b49' },
  { name: 'Rose', value: '#af7278' },
  { name: 'Violet', value: '#8e7ba1' },
];
export const environments: EnvironmentTag[] = [
  'Local',
  'Development',
  'Staging',
  'Production',
  'Custom',
];
export const CONNECTION_STORAGE_KEY = 'stoneql-connections';
export interface ConnectionPreferences {
  version: 1;
  connections: Connection[];
  groups: ConnectionGroup[];
}
const text = (value: unknown, fallback = '') => (typeof value === 'string' ? value : fallback);
const reference = (value: unknown) =>
  typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value) ? value : undefined;
/** Drops undefined members so serialized profiles never contain placeholder keys. */
const defined = <T extends object>(value: T): T =>
  Object.fromEntries(Object.entries(value).filter(([, member]) => member !== undefined)) as T;
export function sanitizeConnection(value: Connection, groups: ConnectionGroup[]): Connection {
  const source = value as Connection;
  const environment = environments.includes(source.environment as EnvironmentTag)
    ? source.environment!
    : /prod/i.test(source.name)
      ? 'Production'
      : /stag/i.test(source.name)
        ? 'Staging'
        : /local/i.test(source.name)
          ? 'Local'
          : 'Development';
  return {
    ...(source.mode === 'live' ? { mode: 'live' as const } : {}),
    id: text(source.id) || crypto.randomUUID(),
    name: text(source.name, 'Untitled connection'),
    engine: ['MySQL', 'MariaDB', 'PostgreSQL', 'SQLite'].includes(source.engine)
      ? source.engine
      : ('MySQL' as Engine),
    host: text(source.host, '127.0.0.1'),
    port: Number.isInteger(source.port) ? source.port : 3306,
    user: text(source.user, 'root'),
    database: text(source.database, 'acme_store'),
    color: /^#[a-f\d]{6}$/i.test(source.color ?? '') ? source.color : connectionColors[0]!.value,
    environment,
    customEnvironment: environment === 'Custom' ? text(source.customEnvironment).slice(0, 40) : '',
    groupId: groups.some((g) => g.id === source.groupId) ? source.groupId : '',
    // Opaque OS secret reference only; the secret itself is never serialized.
    ...defined({ passwordRef: source.mode === 'live' ? reference(source.passwordRef) : undefined }),
    // Only an explicit "TLS off" is stored; anything else means verified TLS.
    ...defined({ tls: source.mode === 'live' && source.tls === false ? (false as const) : undefined }),
    // Deliberately reconstruct transport. Never serialize credential fields, grants or unknown
    // fields; only the enrolled public host key, its fingerprint, references and a file name.
    transport:
      source.transport?.type === 'ssh'
        ? {
            type: 'ssh',
            host: text(source.transport.host),
            port: Number.isInteger(source.transport.port) ? source.transport.port : 22,
            user: text(source.transport.user),
            auth: ['agent', 'password', 'private-key'].includes(source.transport.auth)
              ? source.transport.auth
              : 'agent',
            ...defined({
              // Host keys are trusted natively (F-01); legacy hostKey/hostKeyFingerprint are dropped.
              passwordRef: source.mode === 'live' ? reference(source.transport.passwordRef) : undefined,
              passphraseRef: source.mode === 'live' ? reference(source.transport.passphraseRef) : undefined,
              keyFileName: text(source.transport.keyFileName).replace(/[\\/]/g, '').slice(0, 255) || undefined,
            }),
          }
        : { type: 'direct' },
  };
}
export function connectionSnapshot(
  connections: Connection[],
  groups: ConnectionGroup[],
): ConnectionPreferences {
  return {
    version: 1,
    groups: groups.map((g) => ({ id: g.id, name: g.name, collapsed: !!g.collapsed })),
    connections: connections.map((c) => sanitizeConnection(c, groups)),
  };
}
export function loadConnectionPreferences(
  storage: Pick<Storage, 'getItem'>,
  defaults: Connection[],
): ConnectionPreferences & { error?: string } {
  try {
    const raw = storage.getItem(CONNECTION_STORAGE_KEY);
    if (!raw) return connectionSnapshot(defaults, []);
    const parsed = JSON.parse(raw);
    if (
      !Array.isArray(parsed) &&
      (!parsed || typeof parsed !== 'object' || !Array.isArray(parsed.connections))
    )
      throw new Error('Invalid saved connection format.');
    if (!Array.isArray(parsed) && parsed.version && parsed.version !== 1)
      throw new Error('Unsupported saved connection version.');
    const seen = new Set<string>();
    const groups: ConnectionGroup[] = Array.isArray(parsed.groups)
      ? parsed.groups
          .filter(
            (g: any) =>
              g &&
              typeof g.id === 'string' &&
              typeof g.name === 'string' &&
              g.name.trim() &&
              !seen.has(g.id) &&
              seen.add(g.id),
          )
          .map((g: any) => ({ id: g.id, name: g.name.trim(), collapsed: !!g.collapsed }))
      : [];
    const list = Array.isArray(parsed) ? parsed : parsed.connections;
    const ids = new Set<string>();
    const connections = list
      .filter((c: any) => c && typeof c === 'object' && typeof c.name === 'string')
      .map((c: Connection) => sanitizeConnection(c, groups))
      .filter((c: Connection) => !ids.has(c.id) && ids.add(c.id));
    return { version: 1, connections, groups };
  } catch (error) {
    return {
      ...connectionSnapshot(defaults, []),
      error: `Saved connections could not be loaded: ${(error as Error).message}`,
    };
  }
}
export function environmentLabel(connection: Connection): string {
  return connection.environment === 'Production'
    ? 'PROD'
    : connection.environment === 'Custom'
      ? connection.customEnvironment || 'Custom'
      : connection.environment || 'Development';
}
