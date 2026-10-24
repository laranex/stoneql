import type { Engine } from './index';
export type ExportFormat = 'SQL' | 'CSV' | 'JSON';
export interface ExportConfig {
  scope: 'database' | 'table';
  mode: 'quick' | 'custom';
  tables: string[];
  content: 'both' | 'structure' | 'data';
  format: ExportFormat;
  compression: 'none' | 'zip' | 'gzip';
  filename: string;
  drop: boolean;
  ifExists: boolean;
  create: boolean;
  autoIncrement: boolean;
  insertStyle: 'standard' | 'extended' | 'ignore' | 'replace';
  transaction: boolean;
  foreignKeys: boolean;
  views: boolean;
  triggers: boolean;
  routines: boolean;
  events: boolean;
  csv: { delimiter: string; enclosure: string; headers: boolean; nullValue: string };
  json: { pretty: boolean; metadata: boolean };
}
export interface ExportCapabilities {
  autoIncrement: boolean;
  foreignKeys: boolean;
  views: boolean;
  triggers: boolean;
  routines: boolean;
  events: boolean;
  insertStyles: ExportConfig['insertStyle'][];
}
export const capabilities: Record<Engine, ExportCapabilities> = {
  MySQL: {
    autoIncrement: true,
    foreignKeys: true,
    views: true,
    triggers: true,
    routines: true,
    events: true,
    insertStyles: ['standard', 'extended', 'ignore', 'replace'],
  },
  MariaDB: {
    autoIncrement: true,
    foreignKeys: true,
    views: true,
    triggers: true,
    routines: true,
    events: true,
    insertStyles: ['standard', 'extended', 'ignore', 'replace'],
  },
  PostgreSQL: {
    autoIncrement: false,
    foreignKeys: false,
    views: true,
    triggers: true,
    routines: true,
    events: false,
    insertStyles: ['standard', 'extended'],
  },
  SQLite: {
    autoIncrement: true,
    foreignKeys: true,
    views: true,
    triggers: true,
    routines: false,
    events: false,
    insertStyles: ['standard', 'extended', 'ignore', 'replace'],
  },
};
export function defaultExport(): ExportConfig {
  return {
    scope: 'table',
    mode: 'quick',
    tables: [],
    content: 'both',
    format: 'SQL',
    compression: 'none',
    filename: '@DATABASE@_@TABLE@_@DATE@',
    drop: false,
    ifExists: true,
    create: true,
    autoIncrement: true,
    insertStyle: 'extended',
    transaction: true,
    foreignKeys: true,
    views: false,
    triggers: false,
    routines: false,
    events: false,
    csv: { delimiter: ',', enclosure: '"', headers: true, nullValue: 'NULL' },
    json: { pretty: true, metadata: false },
  };
}
