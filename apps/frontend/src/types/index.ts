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
      hostKey?: string;
      hostKeyFingerprint?: string;
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
  showHistory: boolean;
  filter: string;
  sort: string;
  descending: boolean;
  queries: QueryTab[];
  queryId: number;
  history: { sql: string; time: string; status: string }[];
}
