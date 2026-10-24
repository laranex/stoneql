import { relations } from './objectCapabilities.ts';
import type { Database, Engine } from '../types';
import type { ExportConfig } from '../types/export.ts';
import { capabilities } from '../types/export.ts';
export function effectiveExport(
  config: ExportConfig,
  engine: Engine,
  scopeTables?: string[],
): ExportConfig {
  const c = structuredClone(config),
    cap = capabilities[engine];
  if (c.mode === 'quick')
    Object.assign(c, {
      content: 'both',
      tables: scopeTables ? [...scopeTables] : [...c.tables],
      compression: 'none',
      filename: '@DATABASE@_@TABLE@_@DATE@',
      drop: false,
      create: true,
      insertStyle: 'extended',
      transaction: true,
      foreignKeys: cap.foreignKeys,
      autoIncrement: cap.autoIncrement,
      views: false,
      triggers: false,
      routines: false,
      events: false,
    });
  for (const key of [
    'autoIncrement',
    'foreignKeys',
    'views',
    'triggers',
    'routines',
    'events',
  ] as const)
    if (!cap[key]) c[key] = false;
  if (!cap.insertStyles.includes(c.insertStyle)) c.insertStyle = 'extended';
  if (c.content === 'data') {
    c.create = false;
    c.drop = false;
    c.autoIncrement = false;
  }
  if (c.content === 'structure') c.transaction = false;
  if (c.format !== 'SQL') {
    c.drop = false;
    c.create = false;
    c.transaction = false;
    c.foreignKeys = false;
    c.autoIncrement = false;
    c.views = false;
    c.triggers = false;
    c.routines = false;
    c.events = false;
  }
  return c;
}
export function validateExport(c: ExportConfig, database: Database): string[] {
  const errors: string[] = [];
  if (!c.tables.length) errors.push('Select at least one table.');
  if (c.tables.some((t) => !relations(database).some((table) => table.name === t)))
    errors.push('One or more selected tables no longer exist.');
  if (!c.filename.trim() || /[\\/:*?"<>|]/.test(c.filename))
    errors.push('Enter a filename without path separators or reserved characters.');
  if (c.format === 'CSV' && c.content !== 'data')
    errors.push('CSV supports Data only. Select Data only or choose SQL / JSON.');
  if (
    c.format === 'CSV' &&
    (c.csv.delimiter.length !== 1 ||
      !['', '"', "'"].includes(c.csv.enclosure) ||
      c.csv.delimiter === c.csv.enclosure)
  )
    errors.push('CSV needs a single delimiter and a different optional quote enclosure.');
  if (c.format === 'SQL' && c.content !== 'data' && !c.create)
    errors.push('Enable CREATE TABLE to include the selected structure.');
  return errors;
}
export function exportSummary(c: ExportConfig, database: Database, engine: Engine) {
  const tables = relations(database).filter((t) => c.tables.includes(t.name) && (!t.kind || t.kind==='table' || c.content==='data' || c.format!=='SQL'));
  const includeDefinitions=c.format==='SQL'&&c.content!=='data';
  const views=includeDefinitions&&(c.views||c.scope==='table')?(database.views||[]).filter(v=>c.scope==='database'||c.tables.includes(v.name)):[];
  const triggerCount=includeDefinitions&&c.triggers?(database.triggers||[]).filter(t=>[...tables,...views].some(object=>object.name===t.table)).length:0;
  const rows = c.content === 'structure' ? 0 : tables.reduce((n, t) => n + t.rows.length, 0);
  const structureCount = c.content === 'data' ? 0 : tables.length;
  const extension =
    c.format.toLowerCase() +
    (c.compression === 'none' ? '' : c.compression === 'gzip' ? '.gz' : '.zip');
  const filename =
    c.filename
      .replaceAll('@DATABASE@', database.name)
      .replaceAll('@TABLE@', c.tables.length === 1 ? c.tables[0]! : 'tables')
      .replaceAll('@DATE@', new Date().toISOString().slice(0, 10)) +
    '.' +
    extension;
  return {
    tables: tables.length,
    rows,
    structures: structureCount,
    filename,
    engine,
    compression: c.compression,
    format: c.format,
    content: c.content,
    estimatedBytes: Math.max(100, rows * 120 + structureCount * 450),
    objects: { views: views.length, triggers: triggerCount, routines: includeDefinitions&&c.routines&&c.scope==='database'?(database.routines||[]).length:0, events: includeDefinitions&&c.events&&c.scope==='database'?(database.events||[]).length:0 },
  };
}
export interface SimulatedExportTransport {
  run(
    config: ExportConfig,
    database: Database,
    engine: Engine,
    signal: AbortSignal,
    onProgress: (progress: number) => void,
    fail?: boolean,
  ): Promise<ReturnType<typeof exportSummary>>;
}
export const simulatedExportTransport: SimulatedExportTransport = {
  async run(c, db, engine, signal, onProgress, fail = false) {
    const errors = validateExport(c, db);
    if (errors.length) throw new Error(errors.join(' '));
    for (let i = 1; i <= 10; i++) {
      if (signal.aborted) throw new DOMException('Export simulation cancelled.', 'AbortError');
      await new Promise<void>((resolve, reject) => {
        const cancel = () => {
          clearTimeout(timer);
          reject(new DOMException('Export simulation cancelled.', 'AbortError'));
        };
        const timer = setTimeout(() => {
          signal.removeEventListener('abort', cancel);
          resolve();
        }, 160);
        signal.addEventListener('abort', cancel, { once: true });
      });
      if (fail && i === 6)
        throw new Error('Simulated destination unavailable. Adjust the configuration or retry.');
      onProgress(i * 10);
    }
    return exportSummary(c, db, engine);
  },
};
