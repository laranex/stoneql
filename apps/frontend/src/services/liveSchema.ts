import type { Column, Engine, Row, SqlResult, Table } from '../types/index.ts';
import { quoteIdentifier, quoteValue } from '../sql/dialects.ts';

/**
 * Live schema discovery and Browse for Direct/SSH MySQL and MariaDB profiles. Every statement is
 * a plain SELECT accepted by the native read-only subset (no CAST, no DATABASE(), no backslashes),
 * so nothing here can change data. PostgreSQL and SQLite catalogs need functions that subset
 * refuses (`current_schema()`, `pragma_table_info`), so they stay SQL-only for now.
 */
export function liveCatalogSupported(engine: Engine): boolean {
  return engine === 'MySQL' || engine === 'MariaDB';
}

/** The read-only subset refuses backslashes, which MySQL string literals would need doubled. */
function literal(value: string, what: string): string {
  if (value.includes('\\')) throw new Error(`${what} containing a backslash cannot be used in live read-only queries.`);
  return quoteValue(value, 'MySQL');
}

export function liveTablesSql(database: string): string {
  return `SELECT TABLE_NAME AS name, TABLE_TYPE AS kind FROM information_schema.TABLES WHERE TABLE_SCHEMA = ${literal(database, 'A database name')} ORDER BY TABLE_NAME;`;
}

export function liveColumnsSql(database: string, table: string): string {
  return `SELECT COLUMN_NAME AS name, COLUMN_TYPE AS type, IS_NULLABLE AS nullable, COLUMN_KEY AS ckey, COLUMN_DEFAULT AS dflt, EXTRA AS extra FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = ${literal(database, 'A database name')} AND TABLE_NAME = ${literal(table, 'A table name')} ORDER BY ORDINAL_POSITION;`;
}

/** Result rows keyed by the labels the server returned (live results use positional keys). */
export function labelledRows(result: SqlResult): Row[] {
  const labels = result.columns.map((key) => result.columnLabels?.[key] ?? key);
  return result.rows.map((row) => Object.fromEntries(result.columns.map((key, index) => [labels[index]!, row[key] ?? null])));
}

function text(value: Row[string]): string {
  return value === null || value === undefined ? '' : String(value);
}

export function toLiveTables(result: SqlResult): Table[] {
  return labelledRows(result).map((row) => ({
    name: text(row.name),
    kind: text(row.kind) === 'VIEW' || text(row.kind) === 'SYSTEM VIEW' ? 'view' : 'table',
    columns: [],
    rows: [],
  }));
}

/** Column metadata exactly as the catalog reports it; the type keeps the server's spelling. */
export function toLiveColumns(result: SqlResult, engine: Engine): Column[] {
  return labelledRows(result).map((row) => {
    const type = text(row.type), extra = text(row.extra).toLowerCase(), key = text(row.ckey);
    const column: Column = {
      name: text(row.name),
      type,
      nullable: text(row.nullable) === 'YES',
      primary: key === 'PRI',
      unique: key === 'UNI',
      indexed: key === 'MUL',
      unsigned: / unsigned\b/i.test(type),
      autoIncrement: extra.includes('auto_increment'),
    };
    // MariaDB reports defaults as expressions: an unquoted NULL is SQL NULL, strings stay quoted.
    if (row.dflt === null || (engine === 'MariaDB' && row.dflt === 'NULL')) column.defaultKind = column.nullable ? 'null' : 'none';
    else {
      column.defaultKind = 'custom';
      column.default = text(row.dflt);
    }
    return column;
  });
}

export interface LiveBrowsePlan {
  sql: string;
  countSql: string;
  sort: string;
}

/**
 * Server-side page of a live table: same predicate and order as the mock preview, without CAST.
 * No `WHERE (` or `BY (`: the native subset reads a word before `(` as a function call.
 */
export function liveBrowseQuery(input: {
  table: string;
  columns: readonly Column[];
  filter: string;
  sort: string;
  descending: boolean;
  offset: number;
  limit: number;
}): LiveBrowsePlan {
  const engine: Engine = 'MySQL';
  const from = `SELECT * FROM ${quoteIdentifier(input.table, engine)}`;
  let where = '';
  if (input.filter && input.columns.length) {
    const pattern = literal(
      '%' + input.filter.toLowerCase().replaceAll('!', '!!').replaceAll('%', '!%').replaceAll('_', '!_') + '%',
      'Filter text',
    );
    where =
      '\nWHERE ' +
      input.columns
        .map((c) => `LOWER(COALESCE(${quoteIdentifier(c.name, engine)}, '')) LIKE ${pattern} ESCAPE '!'`)
        .join(' OR ');
  }
  const sorted = input.columns.find((c) => c.name === input.sort), sort = sorted?.name ?? '';
  let order = '';
  if (sorted) {
    // NULLs last only where NULL is possible: an extra expression would force a filesort and
    // keep the primary-key default page from using its index on large tables.
    const identifier = quoteIdentifier(sort, engine), direction = input.descending ? 'DESC' : 'ASC';
    order = sorted.nullable ? `\nORDER BY ${identifier} IS NULL ASC, ${identifier} ${direction}` : `\nORDER BY ${identifier} ${direction}`;
  }
  return {
    sql: `${from}${where}${order}\nLIMIT ${input.offset}, ${input.limit};`,
    countSql: `SELECT COUNT(*) AS total FROM ${quoteIdentifier(input.table, engine)}${where};`,
    sort,
  };
}
