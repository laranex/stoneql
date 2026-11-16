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

/** A live table's index as the server reports it; `definition` is PostgreSQL's own text. */
export interface LiveIndex { name: string; unique: boolean; columns: string[]; type: string; definition?: string }
/** A live foreign key; `onDelete`/`onUpdate` are the server's rule names. */
export interface LiveForeignKey { name: string; columns: string[]; refTable: string; refColumns: string[]; onDelete: string; onUpdate: string }

export function liveIndexesSql(engine: Engine, database: string, table: string): string {
  if (!mysqlFamily(engine))
    return `SELECT indexname AS name, indexdef AS definition FROM pg_catalog.pg_indexes WHERE schemaname = current_schema AND tablename = ${literal(table, 'A table name', engine)} ORDER BY indexname;`;
  return `SELECT INDEX_NAME AS name, NON_UNIQUE AS nonunique, COLUMN_NAME AS col, INDEX_TYPE AS type FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = ${literal(database, 'A database name')} AND TABLE_NAME = ${literal(table, 'A table name')} ORDER BY INDEX_NAME, SEQ_IN_INDEX;`;
}

/**
 * PostgreSQL foreign keys come from pg_constraint (information_schema hides their rules from
 * SELECT-only accounts). Key arrays print as `{1,2}`; the braces are cut with the comma form of
 * SUBSTRING (FOR, ANY and subscripts are outside the read-only subset) and members matched as
 * `,n,`. Without subscripts the pairing of a composite key's columns is not recoverable, so its
 * columns are listed in attribute order (documented limit).
 */
export function liveForeignKeysSql(engine: Engine, database: string, table: string): string {
  if (!mysqlFamily(engine))
    return `SELECT c.conname AS name, a.attname AS col, fr.relname AS ref_table, fa.attname AS ref_col, c.confdeltype AS on_delete, c.confupdtype AS on_update, a.attnum AS pos FROM pg_catalog.pg_constraint c JOIN pg_catalog.pg_class r ON r.oid = c.conrelid JOIN pg_catalog.pg_namespace n ON n.oid = r.relnamespace JOIN pg_catalog.pg_class fr ON fr.oid = c.confrelid JOIN pg_catalog.pg_attribute a ON a.attrelid = c.conrelid JOIN pg_catalog.pg_attribute fa ON fa.attrelid = c.confrelid WHERE c.contype = 'f' AND n.nspname = current_schema AND r.relname = ${literal(table, 'A table name', engine)} AND CONCAT(',', SUBSTRING(CONCAT(c.conkey), 2, CHAR_LENGTH(CONCAT(c.conkey)) - 2), ',') LIKE CONCAT('%,', a.attnum, ',%') AND CONCAT(',', SUBSTRING(CONCAT(c.confkey), 2, CHAR_LENGTH(CONCAT(c.confkey)) - 2), ',') LIKE CONCAT('%,', fa.attnum, ',%') ORDER BY c.conname, a.attnum, fa.attnum;`;
  return `SELECT k.CONSTRAINT_NAME AS name, k.COLUMN_NAME AS col, k.REFERENCED_TABLE_NAME AS ref_table, k.REFERENCED_COLUMN_NAME AS ref_col, r.DELETE_RULE AS on_delete, r.UPDATE_RULE AS on_update FROM information_schema.KEY_COLUMN_USAGE k LEFT JOIN information_schema.REFERENTIAL_CONSTRAINTS r ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME WHERE k.TABLE_SCHEMA = ${literal(database, 'A database name')} AND k.TABLE_NAME = ${literal(table, 'A table name')} AND k.REFERENCED_TABLE_NAME IS NOT NULL ORDER BY k.CONSTRAINT_NAME, k.ORDINAL_POSITION;`;
}

export function toLiveIndexes(result: SqlResult, engine: Engine): LiveIndex[] {
  const rows = catalogRows(result);
  if (!mysqlFamily(engine))
    return rows.map((row) => {
      const definition = text(row.definition);
      return { name: text(row.name), unique: /^CREATE UNIQUE INDEX /.test(definition), columns: [], type: /USING (\w+)/.exec(definition)?.[1] ?? '', definition };
    });
  const indexes = new Map<string, LiveIndex>();
  for (const row of rows) {
    const name = text(row.name);
    const index = indexes.get(name) ?? { name, unique: text(row.nonunique) === '0', columns: [], type: text(row.type) };
    index.columns.push(text(row.col));
    indexes.set(name, index);
  }
  return [...indexes.values()];
}

/** PostgreSQL stores rules as one-letter codes (pg_constraint docs); shown by their SQL names. */
const postgresRules: Record<string, string> = { a: 'NO ACTION', r: 'RESTRICT', c: 'CASCADE', n: 'SET NULL', d: 'SET DEFAULT' };
/** MariaDB hides REFERENTIAL_CONSTRAINTS from SELECT-only accounts while keys stay visible. */
export const RULE_NOT_VISIBLE = 'not visible to this account';
export function toLiveForeignKeys(result: SqlResult, engine: Engine): LiveForeignKey[] {
  const keys = new Map<string, LiveForeignKey>();
  for (const row of catalogRows(result)) {
    const name = text(row.name);
    const rule = (value: string) => (!value ? RULE_NOT_VISIBLE : mysqlFamily(engine) ? value : postgresRules[value] ?? value);
    const key = keys.get(name) ?? { name, columns: [], refTable: text(row.ref_table), refColumns: [], onDelete: rule(text(row.on_delete)), onUpdate: rule(text(row.on_update)) };
    // PostgreSQL rows are the column × referenced-column product of one key; keep each once.
    if (!key.columns.includes(text(row.col))) key.columns.push(text(row.col));
    if (!key.refColumns.includes(text(row.ref_col))) key.refColumns.push(text(row.ref_col));
    keys.set(name, key);
  }
  return [...keys.values()];
}

/**
 * Schema-changing keywords in SQL text (strings and comments removed), used only to decide when
 * the live catalog must be re-read. Not a security check: the server remains the authority.
 */
export function ddlStatements(sql: string): string[] {
  const code = sql.replace(/'(?:[^'\\]|\\.|'')*'|"(?:[^"\\]|\\.)*"|`[^`]*`|--[^\n]*|#[^\n]*|\/\*[\s\S]*?\*\//g, ' ');
  return [...new Set((code.match(/\b(CREATE|ALTER|DROP|RENAME|TRUNCATE)\b/gi) || []).map((word) => word.toUpperCase()))];
}
