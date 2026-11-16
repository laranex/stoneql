import type { Column, Engine } from '../types/index.ts';
import { quoteIdentifier } from '../sql/dialects.ts';
import { liveEditableColumn, liveLiteral, type LiveValue } from './liveMutations.ts';

/**
 * M007 live CSV import (alpha slice): parses the file the user picked in the browser file input
 * (the renderer never sees a path) and builds multi-row INSERT batches with the same exact
 * literals as live editing. The caller runs every batch inside one transaction lease, so the
 * import is all-or-nothing. CSV convention (matching StoneQL's export): an unquoted `NULL` is
 * SQL NULL; every other field is literal text, so `""` is the empty string.
 */
export const MAX_IMPORT_BYTES = 64 * 1024 * 1024; // Proposed: the renderer holds the file.
export const MAX_BATCH_SQL_BYTES = 60 * 1024; // Below the 64 KiB native job script limit.

export interface CsvCell { value: string; quoted: boolean }

/** RFC 4180 fields with quoting kept, so NULL and the text "NULL" stay distinct. */
export function csvCells(text: string): CsvCell[][] {
  const records: CsvCell[][] = [];
  let record: CsvCell[] = [], value = '', quoted = false, closed = false, started = false, wasQuoted = false;
  const field = () => { record.push({ value, quoted: wasQuoted }); value = ''; closed = false; started = false; wasQuoted = false; };
  const line = () => { field(); records.push(record); record = []; };
  text = text.replace(/^﻿/, '');
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!;
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') { value += '"'; i++; } else { quoted = false; closed = true; }
      } else value += c;
    } else if (c === ',') field();
    else if (c === '\n' || c === '\r') { if (c === '\r' && text[i + 1] === '\n') i++; line(); }
    else if (c === '"' && !started && !closed) { quoted = true; started = true; wasQuoted = true; }
    else { if (closed || c === '"') throw new Error(`CSV record ${records.length + 1}: unexpected character outside a quoted field.`); value += c; started = true; }
  }
  if (quoted) throw new Error('CSV has an unclosed quoted field.');
  if (started || closed || record.length) line();
  return records;
}

export interface ImportPlan { batches: { sql: string; first: number; last: number }[]; records: number }
/** Validates headers against the live table and builds batches; record numbers count the header as 1. */
export function liveCsvImportPlan(engine: Engine, table: string, columns: readonly Column[], text: string): ImportPlan {
  const records = csvCells(text);
  const header = records.shift()?.map((cell) => cell.value) ?? [];
  if (!header.length || new Set(header).size !== header.length) throw new Error('The first CSV line must name distinct columns of the table.');
  for (const name of header) {
    const column = columns.find((c) => c.name === name);
    if (!column) throw new Error(`CSV column ${name} is not a column of ${table}.`);
    if (!liveEditableColumn(column)) throw new Error(`CSV column ${name} holds binary data, which CSV cannot carry exactly; use SQL.`);
  }
  const prefix = `INSERT INTO ${quoteIdentifier(table, engine)} (${header.map((n) => quoteIdentifier(n, engine)).join(', ')}) VALUES\n`;
  const batches: ImportPlan['batches'] = [];
  let rows: string[] = [], first = 2, size = prefix.length;
  const flush = (last: number) => { if (rows.length) batches.push({ sql: `${prefix}${rows.join(',\n')};`, first, last }); rows = []; size = prefix.length; first = last + 1; };
  records.forEach((cells, index) => {
    const number = index + 2;
    if (cells.length !== header.length) throw new Error(`CSV record ${number}: expected ${header.length} fields, received ${cells.length}.`);
    const values = cells.map((cell) => liveLiteral(engine, (!cell.quoted && cell.value === 'NULL' ? null : cell.value) as LiveValue));
    const row = `(${values.join(', ')})`;
    if (prefix.length + row.length + 2 > MAX_BATCH_SQL_BYTES) throw new Error(`CSV record ${number} is too large for one statement (${MAX_BATCH_SQL_BYTES / 1024} KiB).`);
    if (size + row.length + 2 > MAX_BATCH_SQL_BYTES) flush(number - 1);
    rows.push(row);
    size += row.length + 2;
  });
  flush(records.length + 1);
  return { batches, records: records.length };
}
