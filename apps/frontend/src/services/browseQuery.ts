import type { Column, Engine } from '../types/index.ts';
import { quoteIdentifier, quoteValue } from '../sql/dialects.ts';
export interface BrowseQueryPlan {
  engine: Engine;
  table: string;
  columns: readonly Column[];
  filter: string;
  sort: string;
  descending: boolean;
  offset: number;
  limit: number;
  sql: string;
}
export function buildBrowseQuery(input: Omit<BrowseQueryPlan, 'sql'>): BrowseQueryPlan {
  const { engine, table, columns, filter, descending, offset, limit } = input;
  const sort = columns.some((c) => c.name === input.sort) ? input.sort : '';
  let sql = `SELECT * FROM ${quoteIdentifier(table, engine)}`;
  if (filter) {
    const cast = engine === 'MySQL' || engine === 'MariaDB' ? 'CHAR' : 'TEXT',
      pattern =
        '%' +
        filter.toLowerCase().replaceAll('!', '!!').replaceAll('%', '!%').replaceAll('_', '!_') +
        '%';
    sql +=
      '\nWHERE (' +
      columns
        .map(
          (c) =>
            `LOWER(COALESCE(CAST(${quoteIdentifier(c.name, engine)} AS ${cast}), '')) LIKE ${quoteValue(pattern, engine)} ESCAPE '!'`,
        )
        .join(' OR ') +
      ')';
  }
  if (sort) {
    const identifier = quoteIdentifier(sort, engine), direction = descending ? 'DESC' : 'ASC';
    sql += engine === 'MySQL' || engine === 'MariaDB'
      ? `\nORDER BY (${identifier} IS NULL) ASC, ${identifier} ${direction}`
      : `\nORDER BY ${identifier} ${direction} NULLS LAST`;
  }
  sql +=
    engine === 'MySQL' || engine === 'MariaDB'
      ? `\nLIMIT ${offset}, ${limit};`
      : `\nLIMIT ${limit} OFFSET ${offset};`;
  return { ...input, sort, sql };
}
