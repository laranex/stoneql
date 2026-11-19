import type {ResultSet} from '../../../../contracts/generated/v1';
import type { TableIndex, IndexKind, Routine, Trigger, ScheduledEvent } from './objects';
export type Engine = 'MySQL' | 'MariaDB' | 'PostgreSQL' | 'SQLite';
export type Transport =
  | { type: 'direct' }
  | {
      type: 'ssh';
      host: string;
      port: number;
      user: string;
      auth: 'agent' | 'password' | 'private-key';
      /** Session-only secrets; never persisted. */
      password?: string;
      privateKey?: string;
      passphrase?: string;
      /** Enrolled OpenSSH public host key and its SHA256 fingerprint (non-secret, persisted). */
      /** Opaque OS secret references (desktop live only, persisted). */
      passwordRef?: string;
      passphraseRef?: string;
      /** Opaque native key-file grant (session-only) and its display name (persisted). */
      keyGrant?: string;
      keyFileName?: string;
      /** Draft-only choices to save secrets in the OS store; never persisted. */
      rememberPassword?: boolean;
      rememberPassphrase?: boolean;
    }
  | { type: 'teleport'; cluster: string }
  | { type: 'aws-ssm'; instanceId: string };
export type EnvironmentTag = 'Local' | 'Development' | 'Staging' | 'Production' | 'Custom';
export interface ConnectionGroup {
  id: string;
  name: string;
  collapsed: boolean;
}
export interface Connection {
  mode?: 'mock'|'live';
  rootCertificatePem?: string;
  /**
   * Live direct MySQL/MariaDB: `false` turns TLS off. Omitted means verified TLS, which saved
   * profiles from before this option keep. For a host other than this computer TLS is off only
   * together with `remoteTlsOff`, so an older profile's stored `false` never turns it off.
   */
  tls?: boolean;
  /** Explicit "TLS off" for a host other than this computer (with `tls: false`). */
  remoteTlsOff?: boolean;
  /**
   * TLS off only: the server's RSA public key (PEM), trusted for the password exchange of a full
   * sign-in. Public, saved with the profile. `''` while it is being pasted.
   */
  serverPublicKeyPem?: string;
  /** TLS off only, explicit opt-in: fetch the server's RSA public key without verifying it. */
  publicKeyRetrieval?: boolean;
  /** Session access for one connect, derived from `readOnly` and the transport; never persisted. */
  access?: 'read_only' | 'read_write';
  /**
   * Live only, saved with the profile: `true` opens engine-enforced read-only sessions.
   * Omitted means read-write (A15); PostgreSQL over an SSH tunnel is always read-only.
   */
  readOnly?: boolean;
  /** Opaque native SQLite file grant; session-only, never persisted. */
  sqliteGrant?: string;
  id: string;
  name: string;
  engine: Engine;
  host: string;
  port: number;
  user: string;
  database: string;
  password?: string;
  /** Opaque OS secret reference for the database password (desktop live only). */
  passwordRef?: string;
  /** Draft-only: save the typed password in the OS store on save. */
  rememberPassword?: boolean;
  transport: Transport;
  color: string;
  environment?: EnvironmentTag;
  customEnvironment?: string;
  groupId?: string;
}
export type Row = Record<string, string | number | null>;
export interface Column {
  name: string;
  type: string;
  nullable: boolean;
  primary?: boolean;
  unique?: boolean;
  indexed?: boolean;
  indexKinds?: IndexKind[];
  unsigned?: boolean;
  autoIncrement?: boolean;
  collation?: string;
  comment?: string;
  enumValues?: string[];
  enumName?: string;
  defaultKind?: 'none' | 'null' | 'custom' | 'expression';
  foreign?: string;
  default?: string;
}
export interface Table {
  name: string;
  kind?: 'table' | 'view' | 'materialized-view';
  definition?: string;
  refreshedAt?: string;
  indexes?: TableIndex[];
  schema?: string;
  columns: Column[];
  rows: Row[];
  /** Live catalog only: indexes and foreign keys exactly as the server reports them. */
  live?: {
    indexes: { name: string; unique: boolean; columns: string[]; type: string; definition?: string; constraint?: boolean }[];
    foreignKeys: { name: string; columns: string[]; refTable: string; refColumns: string[]; onDelete: string; onUpdate: string }[];
    error?: string;
  };
}
export interface Database {
  views?: Table[];
  routines?: Routine[];
  triggers?: Trigger[];
  events?: ScheduledEvent[];
  name: string;
  tables: Table[];
}
export type View =
  'Browse' | 'Structure' | 'SQL' | 'Search' | 'Insert' | 'Export' | 'Import' | 'Operations' | 'Routines' | 'Triggers' | 'Events';
export interface SqlResult {
  raw?: ResultSet;
  truncated?: boolean;
  columnLabels?: Record<string,string>;
  rows: Row[];
  columns: string[];
  columnTypes?: Record<string, string>;
}
export interface QueryTab {
  id: number;
  title: string;
  sql: string;
  saved: string;
  result?: SqlResult | null;
  error?: string;
  running?: boolean;
  executedSql?: string;
  resultDatabase?: string;
  resultPage?: number;
  resultSort?: string;
  resultDescending?: boolean;
  /** Live read-write jobs: every result set, the selected one, and per-statement outcomes. */
  resultSets?: Array<SqlResult & { statement: number; result: number }>;
  resultIndex?: number;
  jobId?: string;
  job?: import('../services/liveConnections').LiveJobResult;
}
export interface Workspace {
  scope: 'server' | 'database' | 'table';
  databases: Database[];
  database: string;
  table: string;
  view: View;
  page: number;
  pageSize: number;
  treeFilter: string;
  expandedDatabases: string[];
  /** Live catalog hit the native row bound; the explorer lists only the first tables. */
  liveCatalogTruncated?: boolean;
  showHistory: boolean;
  filter: string;
  sort: string;
  descending: boolean;
  queries: QueryTab[];
  queryId: number;
  history: { sql: string; time: string; status: string }[];
}
