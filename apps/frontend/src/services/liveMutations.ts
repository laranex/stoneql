import type { Column, Engine, Row } from '../types/index.ts';
import { quoteIdentifier } from '../sql/dialects.ts';
import { describeJob, type LiveJobResult } from './liveConnections.ts';

/**
 * Generated single-row CRUD for live MySQL, MariaDB and PostgreSQL tables (M006). Values are
 * written as literals whose meaning does not depend on server settings:
 * - MySQL/MariaDB: `_utf8mb4 X'…'` (a character string; unaffected by NO_BACKSLASH_ESCAPES),
 * - PostgreSQL: `E'…'` with backslashes and quotes escaped (unaffected by
 *   standard_conforming_strings).
 * The server converts them to the column type, so numbers, dates and ENUM/SET text keep their
 * exact spelling. Rows are addressed by their full primary key; binary and unrecognised types
 * stay read-only. Statements run as read-write jobs, which the native gate controls.
 */
export type LiveValue = string | null | { default: true };

const mysqlFamily = (engine: Engine) => engine === 'MySQL' || engine === 'MariaDB';
const binaryType = /^(tiny|medium|long)?blob|^(var)?binary|^bit\b|^bytea$|^geometry|^point|^linestring|^polygon/i;

export function liveLiteral(engine: Engine, value: LiveValue): string {
  if (value === null) return 'NULL';
  if (typeof value === 'object') return 'DEFAULT';
  if (mysqlFamily(engine)) {
    const hex = Array.from(new TextEncoder().encode(value), (byte) => byte.toString(16).padStart(2, '0')).join('');
    return `_utf8mb4 X'${hex}'`;
  }
  return `E'${value.replaceAll('\\', '\\\\').replaceAll("'", "''")}'`;
}

/** Binary and spatial values are shown as hex text; editing them is not supported live. */
export function liveEditableColumn(column: Column): boolean {
  return !binaryType.test(column.type.trim());
}

/** Why a live table cannot be edited, or null when it can. */
export function liveEditBlocker(engine: Engine, columns: readonly Column[], kind?: string): string | null {
  if (engine === 'SQLite') return 'Live editing supports MySQL, MariaDB and PostgreSQL.';
  if (kind && kind !== 'table') return 'Views are read-only.';
  const keys = columns.filter((c) => c.primary);
  if (!keys.length) return 'This table has no primary key, so a row cannot be addressed safely. Use SQL to change it.';
  if (keys.some((c) => !liveEditableColumn(c))) return 'A binary primary key cannot be addressed from the grid. Use SQL to change it.';
  return null;
}

function where(engine: Engine, columns: readonly Column[], row: Row): string {
  return columns
    .filter((c) => c.primary)
    .map((c) => {
      const value = row[c.name];
      if (value === null || value === undefined) throw new Error(`Primary key ${c.name} has no value; the row cannot be addressed.`);
      return `${quoteIdentifier(c.name, engine)} = ${liveLiteral(engine, String(value))}`;
    })
    .join(' AND ');
}
const limitOne = (engine: Engine) => (mysqlFamily(engine) ? ' LIMIT 1' : '');

export function liveUpdateSql(engine: Engine, table: string, columns: readonly Column[], row: Row, changes: Record<string, LiveValue>): string {
  const names = Object.keys(changes);
  if (!names.length) throw new Error('Nothing to update.');
  for (const name of names) {
    const column = columns.find((c) => c.name === name);
    if (!column || !liveEditableColumn(column)) throw new Error(`Column ${name} cannot be edited live.`);
  }
  const set = names.map((name) => `${quoteIdentifier(name, engine)} = ${liveLiteral(engine, changes[name]!)}`).join(', ');
  return `UPDATE ${quoteIdentifier(table, engine)} SET ${set} WHERE ${where(engine, columns, row)}${limitOne(engine)};`;
}

export function liveDeleteSql(engine: Engine, table: string, columns: readonly Column[], row: Row): string {
  return `DELETE FROM ${quoteIdentifier(table, engine)} WHERE ${where(engine, columns, row)}${limitOne(engine)};`;
}

/** Columns left out of `values` take their server default (omitted from the column list). */
export function liveInsertSql(engine: Engine, table: string, columns: readonly Column[], values: Record<string, LiveValue>): string {
  const names = columns.map((c) => c.name).filter((name) => name in values && !(typeof values[name] === 'object' && values[name] !== null));
  for (const name of names) {
    if (!liveEditableColumn(columns.find((c) => c.name === name)!)) throw new Error(`Column ${name} cannot be written live.`);
  }
  if (!names.length) return mysqlFamily(engine) ? `INSERT INTO ${quoteIdentifier(table, engine)} () VALUES ();` : `INSERT INTO ${quoteIdentifier(table, engine)} DEFAULT VALUES;`;
  return `INSERT INTO ${quoteIdentifier(table, engine)} (${names.map((n) => quoteIdentifier(n, engine)).join(', ')}) VALUES (${names.map((n) => liveLiteral(engine, values[n]!)).join(', ')});`;
}

export type LiveMutationOutcome = { ok: true; text: string } | { ok: false; kind: 'conflict' | 'error'; text: string };
/**
 * Success only when the job succeeded and changed exactly one row. A succeeded statement that
 * matched no row is a conflict (changed or deleted elsewhere); failures and unknown outcomes
 * keep the job's own truthful description (never "saved").
 */
export function liveMutationOutcome(job: LiveJobResult, action: 'updated' | 'inserted' | 'deleted'): LiveMutationOutcome {
  if (job.outcome !== 'succeeded') return { ok: false, kind: 'error', text: describeJob(job, false).text };
  const affected = job.statements[0]?.affectedRows ?? '0';
  if (affected === '1') return { ok: true, text: `1 row ${action}.` };
  if (affected === '0') return { ok: false, kind: 'conflict', text: `No row was ${action}: it was changed or deleted since it was loaded. Refresh and try again.` };
  return { ok: false, kind: 'error', text: `${affected} rows were ${action}; expected exactly one. Verify on the server.` };
}
