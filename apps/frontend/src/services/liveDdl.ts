import type { Engine } from '../types/index.ts';
import { quoteIdentifier } from '../sql/dialects.ts';

/**
 * Live column DDL for MySQL/MariaDB (read-write sessions). Existing columns are edited from the
 * server's own definition text (`SHOW CREATE TABLE`), so collation, comment, `ON UPDATE`,
 * generated expressions and defaults survive unless the user changes them. A rename alone uses
 * `RENAME COLUMN`; a definition change restates the edited definition with `CHANGE COLUMN`.
 * The server enforces privileges; StoneQL only builds and previews the statement.
 */
export type ColumnPosition = { kind: 'last' } | { kind: 'first' } | { kind: 'after'; column: string };

export function showCreateTableSql(engine: Engine, table: string): string {
  return `SHOW CREATE TABLE ${quoteIdentifier(table, engine)};`;
}

/** Column name → its exact definition (everything after the name) from SHOW CREATE TABLE. */
export function columnDefinitions(createTable: string): Map<string, string> {
  const definitions = new Map<string, string>();
  for (const line of createTable.split('\n')) {
    if (!line.startsWith('  `')) continue;
    let i = 3, name = '';
    for (; i < line.length; i++) {
      if (line[i] === '`') {
        if (line[i + 1] === '`') { name += '`'; i++; continue; }
        break;
      }
      name += line[i];
    }
    if (i >= line.length) continue;
    const definition = line.slice(i + 1).trim().replace(/,$/, '');
    definitions.set(name, definition);
  }
  return definitions;
}

/**
 * Refuses a definition that could end the statement or hide text from the preview: a `;`,
 * backtick or comment outside string literals. Quoted text (`COMMENT 'a;b'`) is fine.
 */
export function definitionProblem(definition: string): string {
  const text = definition.trim();
  if (!text) return 'Enter the column definition, for example VARCHAR(255) NULL.';
  if (text.length > 8192) return 'The definition is too long.';
  let quote = '';
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!;
    if (quote) {
      if (c === '\\') { i++; continue; }
      if (c === quote) { if (text[i + 1] === quote) i++; else quote = ''; }
      continue;
    }
    if (c === "'" || c === '"') { quote = c; continue; }
    if (c === ';') return 'A definition cannot contain ";" outside quotes.';
    if (c === '`') return 'A definition cannot contain a backtick outside quotes.';
    if (c === '#' || (c === '-' && text[i + 1] === '-') || (c === '/' && text[i + 1] === '*')) return 'A definition cannot contain a comment; use COMMENT \'…\'.';
  }
  if (quote) return 'A quoted value in the definition is not closed.';
  return '';
}
export function columnNameProblem(name: string): string {
  if (!name.trim()) return 'Enter a column name.';
  if (name.length > 64) return 'Column names are at most 64 characters.';
  if (/[\u0000-\u001f]/.test(name) || name !== name.trimEnd()) return 'Column names cannot contain control characters or end with a space.';
  return '';
}

function position(engine: Engine, at: ColumnPosition): string {
  return at.kind === 'first' ? ' FIRST' : at.kind === 'after' ? ` AFTER ${quoteIdentifier(at.column, engine)}` : '';
}
export function addColumnSql(engine: Engine, table: string, name: string, definition: string, at: ColumnPosition): string {
  return `ALTER TABLE ${quoteIdentifier(table, engine)} ADD COLUMN ${quoteIdentifier(name, engine)} ${definition.trim()}${position(engine, at)};`;
}
/** '' when nothing changed. A rename alone keeps the definition untouched on the server. */
export function editColumnSql(engine: Engine, table: string, column: string, name: string, before: string, after: string): string {
  const renamed = name !== column, redefined = after.trim() !== before.trim();
  if (!renamed && !redefined) return '';
  if (!redefined) return `ALTER TABLE ${quoteIdentifier(table, engine)} RENAME COLUMN ${quoteIdentifier(column, engine)} TO ${quoteIdentifier(name, engine)};`;
  return `ALTER TABLE ${quoteIdentifier(table, engine)} CHANGE COLUMN ${quoteIdentifier(column, engine)} ${quoteIdentifier(name, engine)} ${after.trim()};`;
}
export function dropColumnSql(engine: Engine, table: string, column: string): string {
  return `ALTER TABLE ${quoteIdentifier(table, engine)} DROP COLUMN ${quoteIdentifier(column, engine)};`;
}
export function createDatabaseSql(engine: Engine, name: string, charset = '', collation = ''): string {
  const characterSet = charset ? ` CHARACTER SET ${charset}` : '', collate = collation ? ` COLLATE ${collation}` : '';
  return `CREATE DATABASE ${quoteIdentifier(name, engine)}${characterSet}${collate};`;
}
/** Character set / collation names: letters, digits and underscores only. */
export function charsetProblem(value: string): string {
  return value && !/^[A-Za-z0-9_]{1,64}$/.test(value) ? 'Use a character set or collation name such as utf8mb4 or utf8mb4_0900_ai_ci.' : '';
}
