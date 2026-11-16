import type { Column, Engine, Row, SqlResult, Table } from '../types/index.ts';
import { quoteIdentifier, quoteValue } from '../sql/dialects.ts';

/**
 * Live schema discovery and Browse for Direct/SSH MySQL, MariaDB and PostgreSQL profiles. Every
 * statement is a plain SELECT accepted by the native read-only subset (no casts, no
 * `DATABASE()`/`current_schema()` calls, no backslashes, no `word (` outside allowlisted
 * functions), so nothing here can change data. PostgreSQL uses the session's `current_schema`
 * (a parameterless SQL function written without parentheses). SQLite stays SQL-only.
 */
export function liveCatalogSupported(engine: Engine): boolean {
  return engine === 'MySQL' || engine === 'MariaDB' || engine === 'PostgreSQL';
}
const mysqlFamily = (engine: Engine) => engine === 'MySQL' || engine === 'MariaDB';

/** The read-only subset refuses backslashes, which MySQL string literals would need doubled. */
function literal(value: string, what: string, engine: Engine = 'MySQL'): string {
  if (value.includes('\\')) throw new Error(`${what} containing a backslash cannot be used in live read-only queries.`);
  return quoteValue(value, engine);
}

export function liveTablesSql(engine: Engine, database: string): string {
  if (!mysqlFamily(engine))
    return `SELECT table_name AS name, table_type AS kind FROM information_schema.tables WHERE table_schema = current_schema ORDER BY table_name;`;
  return `SELECT TABLE_NAME AS name, TABLE_TYPE AS kind FROM information_schema.TABLES WHERE TABLE_SCHEMA = ${literal(database, 'A database name')} ORDER BY TABLE_NAME;`;
}

/**
 * PostgreSQL primary-key membership comes from pg_catalog (information_schema.table_constraints
 * hides keys from SELECT-only accounts); `indkey` is matched as text through the allowlisted
 * CONCAT because `= ANY (...)` and casts are outside the read-only subset.
 */
export function liveColumnsSql(engine: Engine, database: string, table: string): string {
  if (!mysqlFamily(engine)) {
    const name = literal(table, 'A table name', engine);
    return `SELECT c.column_name AS name, c.data_type AS type, c.udt_name AS udt, c.character_maximum_length AS len, c.numeric_precision AS prec, c.numeric_scale AS scale, c.datetime_precision AS dprec, c.is_nullable AS nullable, c.column_default AS dflt, c.is_identity AS ident, (SELECT COUNT(*) FROM pg_catalog.pg_index i JOIN pg_catalog.pg_class r ON r.oid = i.indrelid JOIN pg_catalog.pg_namespace n ON n.oid = r.relnamespace JOIN pg_catalog.pg_attribute a ON a.attrelid = r.oid WHERE i.indisprimary AND n.nspname = c.table_schema AND r.relname = c.table_name AND a.attname = c.column_name AND CONCAT(' ', i.indkey, ' ') LIKE CONCAT('% ', a.attnum, ' %')) AS pk FROM information_schema.columns c WHERE c.table_schema = current_schema AND c.table_name = ${name} ORDER BY c.ordinal_position;`;
  }
  return `SELECT COLUMN_NAME AS name, COLUMN_TYPE AS type, IS_NULLABLE AS nullable, COLUMN_KEY AS ckey, COLUMN_DEFAULT AS dflt, EXTRA AS extra FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = ${literal(database, 'A database name')} AND TABLE_NAME = ${literal(table, 'A table name')} ORDER BY ORDINAL_POSITION;`;
}

/** Result rows keyed by the labels the server returned (live results use positional keys). */
export function labelledRows(result: SqlResult): Row[] {
  const labels = result.columns.map((key) => result.columnLabels?.[key] ?? key);
  return result.rows.map((row) => Object.fromEntries(result.columns.map((key, index) => [labels[index]!, row[key] ?? null])));
}

/**
 * Catalog rows. MySQL 8 reports several information_schema columns (TABLE_TYPE, COLUMN_TYPE,
 * COLUMN_KEY, COLUMN_DEFAULT) with the binary character set, so the native result carries them as
 * binary cells; they are catalog text and are decoded as strict UTF-8 here. User data is never
 * reinterpreted this way.
 */
export function catalogRows(result: SqlResult): Row[] {
  if (!result.raw) return labelledRows(result);
  const decoder = new TextDecoder('utf-8', { fatal: true });
  const labels = result.raw.columns.map((c) => c.name);
  return result.raw.rows.map((cells) =>
    Object.fromEntries(
      cells.map((cell, index) => {
        let value: string | null;
        if (cell.kind === 'null') value = null;
        else if (cell.kind === 'binary') {
          const bytes = new Uint8Array(cell.hex.length / 2);
          for (let i = 0; i < bytes.length; i++) bytes[i] = parseInt(cell.hex.slice(i * 2, i * 2 + 2), 16);
          value = decoder.decode(bytes);
        } else if (cell.kind === 'boolean') value = cell.value ? 'true' : 'false';
        else if (cell.kind === 'unknown') throw new Error(`Unexpected ${cell.databaseType} value in the live catalog.`);
        else value = cell.text;
        return [labels[index]!, value];
      }),
    ),
  );
}

function text(value: Row[string]): string {
  return value === null || value === undefined ? '' : String(value);
}

export function toLiveTables(result: SqlResult): Table[] {
  return catalogRows(result).map((row) => ({
    name: text(row.name),
    kind: text(row.kind) === 'VIEW' || text(row.kind) === 'SYSTEM VIEW' ? 'view' : 'table',
    columns: [],
    rows: [],
  }));
}

/** Column metadata exactly as the catalog reports it; the type keeps the server's spelling. */
/** PostgreSQL type text from information_schema, as psql spells it (no type is inferred). */
function postgresType(row: Row): string {
  const type = text(row.type), length = text(row.len), precision = text(row.prec), scale = text(row.scale);
  if (type === 'USER-DEFINED' || type === 'ARRAY') return text(row.udt);
  if (length && ['character varying', 'character', 'bit', 'bit varying'].includes(type)) return `${type}(${length})`;
  if (type === 'numeric' && precision) return `numeric(${precision},${scale || '0'})`;
  return type;
}
function postgresColumns(rows: Row[]): Column[] {
  return rows.map((row) => {
    const dflt = row.dflt === null || row.dflt === undefined ? null : text(row.dflt);
    const column: Column = {
      name: text(row.name),
      type: postgresType(row),
      nullable: text(row.nullable) === 'YES',
      primary: Number(row.pk) > 0,
      autoIncrement: text(row.ident) === 'YES' || /^nextval\(/.test(dflt ?? ''),
    };
    // PostgreSQL reports defaults as expressions (for example 'x'::text); shown verbatim.
    if (dflt === null) column.defaultKind = column.nullable ? 'null' : 'none';
    else {
      column.defaultKind = 'custom';
      column.default = dflt;
    }
    return column;
  });
}

export function toLiveColumns(result: SqlResult, engine: Engine): Column[] {
  if (!mysqlFamily(engine)) return postgresColumns(catalogRows(result));
  return catalogRows(result).map((row) => {
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
  engine?: Engine;
  table: string;
  columns: readonly Column[];
  filter: string;
  sort: string;
  descending: boolean;
  offset: number;
  limit: number;
}): LiveBrowsePlan {
  const engine: Engine = input.engine ?? 'MySQL', mysql = mysqlFamily(engine);
  const from = `SELECT * FROM ${quoteIdentifier(input.table, engine)}`;
  let where = '';
  if (input.filter && input.columns.length) {
    const pattern = literal(
      '%' + input.filter.toLowerCase().replaceAll('!', '!!').replaceAll('%', '!%').replaceAll('_', '!_') + '%',
      'Filter text',
      engine,
    );
    // PostgreSQL: CONCAT turns any type into text and NULL into '' (COALESCE(int, '') fails).
    const text = (identifier: string) => (mysql ? `COALESCE(${identifier}, '')` : `CONCAT(${identifier})`);
    where =
      '\nWHERE ' +
      input.columns
        .map((c) => `LOWER(${text(quoteIdentifier(c.name, engine))}) LIKE ${pattern} ESCAPE '!'`)
        .join(' OR ');
  }
  const sorted = input.columns.find((c) => c.name === input.sort), sort = sorted?.name ?? '';
  let order = '';
  if (sorted) {
    // NULLs last only where NULL is possible: an extra expression would force a filesort and
    // keep the primary-key default page from using its index on large tables.
    const identifier = quoteIdentifier(sort, engine), direction = input.descending ? 'DESC' : 'ASC';
    order = !sorted.nullable
      ? `\nORDER BY ${identifier} ${direction}`
      : mysql
        ? `\nORDER BY ${identifier} IS NULL ASC, ${identifier} ${direction}`
        : `\nORDER BY ${identifier} ${direction} NULLS LAST`;
  }
  return {
    sql: `${from}${where}${order}\n${mysql ? `LIMIT ${input.offset}, ${input.limit}` : `LIMIT ${input.limit} OFFSET ${input.offset}`};`,
    countSql: `SELECT COUNT(*) AS total FROM ${quoteIdentifier(input.table, engine)}${where};`,
    sort,
  };
}
