import { MySQL, MariaSQL, PostgreSQL, SQLite } from '@codemirror/lang-sql';
import type { Engine } from '../types/index.ts';
export const sqlDialects = { MySQL, MariaDB: MariaSQL, PostgreSQL, SQLite };
export const sqlFunctions: Record<Engine, readonly string[]> = {
  MySQL: ['IFNULL','NOW','CONCAT','DATE_FORMAT','GROUP_CONCAT','JSON_EXTRACT'],
  MariaDB: ['IFNULL','NOW','CONCAT','DATE_FORMAT','GROUP_CONCAT','JSON_EXTRACT'],
  PostgreSQL: ['STRING_AGG','DATE_TRUNC','TO_CHAR','JSONB_EXTRACT_PATH_TEXT','NOW','CONCAT','UNNEST'],
  SQLite: ['IFNULL','STRFTIME','DATETIME','JULIANDAY','GROUP_CONCAT','JSON_EXTRACT'],
};
export const commonFunctions = ['COUNT','SUM','AVG','MIN','MAX','COALESCE','NULLIF','LOWER','UPPER','LENGTH','ABS','ROUND'];
export function quoteIdentifier(name: string, engine: Engine, quote?: string): string {
  const q = quote || (engine === 'MySQL' || engine === 'MariaDB' ? '`' : '"');
  const close = q === '[' ? ']' : q;
  return q + name.replaceAll(close, close + close) + close;
}
export function identifierSQL(name: string, engine: Engine): string {
  const reserved = new Set((sqlDialects[engine].spec.keywords || '').toLowerCase().split(/\s+/));
  return /^[a-z_][\w$]*$/i.test(name) && !reserved.has(name.toLowerCase()) && !(engine === 'PostgreSQL' && name !== name.toLowerCase()) ? name : quoteIdentifier(name,engine);
}
export function quoteValue(value: string, engine: Engine): string {
  const escaped = engine === 'MySQL' || engine === 'MariaDB' ? value.replaceAll('\\','\\\\') : value;
  return "'" + escaped.replaceAll("'","''") + "'";
}
export function identifierMatches(stored: string, value: string, quoted: boolean, engine: Engine): boolean {
  return engine === 'PostgreSQL' ? stored === (quoted ? value : value.toLowerCase()) : stored.toLowerCase() === value.toLowerCase();
}
