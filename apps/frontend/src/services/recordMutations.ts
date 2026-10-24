import { writableTable } from './databaseObjects.ts';
import { rowIndexError, tableIndexes } from './indexes.ts';
import type { Column, Database, Engine, Row, Table } from '../types/index.ts';
import { canonicalValue, validateValue } from './valueValidation.ts';
import { expressionValue } from './columnDefinition.ts';
export class RecordConstraintError extends Error {
  readonly code: 'validation' | 'foreign-key-restrict';
  readonly rowIndex?: number;
  readonly column?: string;
  constructor(
    message: string,
    code: 'validation' | 'foreign-key-restrict' = 'validation',
    rowIndex?: number,
    column?: string,
  ) {
    super(message);
    this.name = 'RecordConstraintError';
    this.code = code;
    this.rowIndex = rowIndex;
    this.column = column;
  }
}
export function mockDefault(table: Table, column: Column, rows: Row[] = table.rows): Row[string] {
  if (column.autoIncrement ?? column.primary) {
    let max = 0n;
    for (const row of rows) {
      const value = row[column.name];
      if (value === null || value === undefined) continue;
      if (!/^[+-]?\d+$/.test(String(value)))
        throw new RecordConstraintError(
          `Cannot generate ${column.name}: existing key is not an integer.`,
        );
      const n = BigInt(String(value));
      if (n > max) max = n;
    }
    return String(max + 1n);
  }
  if (column.defaultKind === 'null') return null;
  if (column.defaultKind === 'expression' || column.default === 'CURRENT_TIMESTAMP')
    return expressionValue(column.default || 'CURRENT_TIMESTAMP');
  return column.default ?? null;
}
function references(database: Database, parent: Table, columnName?: string) {
  return database.tables.flatMap((child) =>
    child.columns
      .filter(
        (column) =>
          column.foreign?.startsWith(parent.name + '.') &&
          (!columnName || column.foreign === `${parent.name}.${columnName}`),
      )
      .map((column) => ({
        child,
        column,
        parentColumn: parent.columns.find((c) => `${parent.name}.${c.name}` === column.foreign),
      })),
  );
}
export function assertCanDelete(database: Database, parent: Table, rows: Row[]): void {
  const selected = new Set(rows);
  for (const { child, column, parentColumn } of references(database, parent)) {
    if (!parentColumn) continue;
    const keys = new Set(rows.map((row) => canonicalValue(row[parentColumn.name], parentColumn)));
    if (
      child.rows.some(
        (row) =>
          !(child === parent && selected.has(row)) &&
          row[column.name] !== null &&
          row[column.name] !== undefined &&
          keys.has(canonicalValue(row[column.name], parentColumn)),
      )
    )
      throw new RecordConstraintError(
        `Cannot delete or update a parent row: ${child.name}.${column.name} references ${parent.name}.${parentColumn.name} (RESTRICT).`,
        'foreign-key-restrict',
      );
  }
}
export function insertRecords(
  database: Database,
  table: Table,
  inputs: readonly Row[],
  engine: Engine,
): Row[] {
  writableTable(table);
  const staged: Row[] = [];
  for (const input of inputs) {
    const row: Row = {};
    for (const column of table.columns)
      row[column.name] = Object.prototype.hasOwnProperty.call(input, column.name)
        ? input[column.name]!
        : mockDefault(table, column, [...table.rows, ...staged]);
    staged.push(row);
  }
  const contextTable = { ...table, rows: [...table.rows, ...staged] },
    tables = database.tables.map((t) => (t === table ? contextTable : t));
  staged.forEach((row, index) => {
    for (const column of table.columns) {
      const result = validateValue(column, row[column.name], engine, {
        row,
        table: contextTable,
        tables,
      });
      if (!result.valid)
        throw new RecordConstraintError(result.error, 'validation', index + 1, column.name);
    }
  });
  table.rows.push(...staged);
  return staged;
}
export function updateRecord(
  database: Database,
  table: Table,
  row: Row,
  patch: Row,
  engine: Engine,
): void {
  writableTable(table);
  if (!table.rows.includes(row)) throw new RecordConstraintError('The record no longer exists.');
  const next = { ...row, ...patch };
  const indexError = rowIndexError(table,next,row);if(indexError) throw new RecordConstraintError(indexError);
  const candidate = {...table, rows:table.rows.map(r=>r===row?next:r)};
  for (const column of table.columns) {
    const result = validateValue(column, next[column.name], engine, {
      row: next,
      table: candidate,
      tables: database.tables.map(t=>t===table?candidate:t),
    });
    if (!result.valid)
      throw new RecordConstraintError(result.error, 'validation', undefined, column.name);
  }
  for (const { child, column, parentColumn } of references(database, table)) {
    if (
      !parentColumn ||
      canonicalValue(row[parentColumn.name], parentColumn) ===
        canonicalValue(next[parentColumn.name], parentColumn)
    )
      continue;
    if (
      child.rows.some(
        (childRow) =>
          childRow !== row &&
          childRow[column.name] !== null &&
          childRow[column.name] !== undefined &&
          canonicalValue(childRow[column.name], parentColumn) ===
            canonicalValue(row[parentColumn.name], parentColumn),
      )
    )
      throw new RecordConstraintError(
        `Cannot delete or update a parent row: ${child.name}.${column.name} references ${table.name}.${parentColumn.name} (RESTRICT).`,
        'foreign-key-restrict',
      );
  }
  Object.assign(row, next);
}
export function deleteRecords(database: Database, table: Table, rows: Row[]): void {
  writableTable(table);
  assertCanDelete(database, table, rows);
  const selected = new Set(rows);
  table.rows = table.rows.filter((row) => !selected.has(row));
}
export function copyRecords(database: Database, table: Table, rows: Row[], engine: Engine): Row[] {
  return insertRecords(
    database,
    table,
    rows.map((row) =>
      Object.fromEntries(
        table.columns
          .filter((c) => !(c.autoIncrement ?? c.primary))
          .map((c) => [c.name, row[c.name] ?? null]),
      ),
    ),
    engine,
  );
}
export function renameMockTable(database: Database, table: Table, name: string): void {
  writableTable(table);
  if (
    !/^[A-Za-z_]\w*$/.test(name) ||
    database.tables.some((t) => t !== table && t.name.toLowerCase() === name.toLowerCase())
  )
    throw new RecordConstraintError('Choose a valid, unique table name.');
  if(database.views?.some(view=>view.name.toLowerCase()===name.toLowerCase())) throw new RecordConstraintError('An object with this name already exists.');
  const previous = table.name;
  table.name = name;
  for(const trigger of database.triggers||[]) if(trigger.table===previous) trigger.table=name;
  for (const child of database.tables)
    for (const column of child.columns)
      if (column.foreign?.startsWith(previous + '.'))
        column.foreign = name + column.foreign.slice(previous.length);
}
export function removeMockTable(
  database: Database,
  table: Table,
  engine: Engine,
  drop: boolean,
): void {
  writableTable(table);
  const ref = references(database, table).find((ref) => ref.child !== table);
  if (ref && (drop || engine !== 'SQLite'))
    throw new RecordConstraintError(
      `Cannot ${drop ? 'drop' : 'truncate'} ${table.name}: ${ref.child.name}.${ref.column.name} references this table (RESTRICT).`,
      'foreign-key-restrict',
    );
  assertCanDelete(database, table, table.rows);
  if (drop) {
    database.tables = database.tables.filter((t) => t !== table);
    database.triggers = database.triggers?.filter(trigger=>trigger.table!==table.name);
  }
  else table.rows = [];
}
export function removeMockColumn(database: Database, table: Table, column: Column): void {
  writableTable(table);
  if (column.primary)
    throw new RecordConstraintError('Remove the primary key before dropping this column.');
  const ref = references(database, table, column.name).find(
    (ref) => ref.child !== table || ref.column !== column,
  );
  if (ref)
    throw new RecordConstraintError(
      `Cannot drop ${column.name}: referenced by ${ref.child.name}.${ref.column.name}.`,
      'foreign-key-restrict',
    );
  if(tableIndexes(table).some(index=>index.columns.some(part=>part.column===column.name))) throw new RecordConstraintError('Drop indexes using this column in the Indexes section first.');
  table.columns = table.columns.filter((c) => c !== column);
  table.rows.forEach((row) => delete row[column.name]);
}
