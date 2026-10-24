import type { BrowseQueryPlan } from './browseQuery.ts';
import type { Connection, Row, Table } from '../types';
export interface ConnectionTransport {
  test(connection: Connection): Promise<{ ok: boolean; message: string }>;
}
export const mockTransport: ConnectionTransport = {
  async test(connection) {
    await new Promise((resolve) => setTimeout(resolve, 650));
    if (
      !connection.name.trim() ||
      (connection.engine !== 'SQLite' && (!connection.host.trim() || !connection.user.trim()))
    )
      return { ok: false, message: 'Name, host, and username are required.' };
    if (
      connection.transport.type === 'ssh' &&
      (!connection.transport.host || !connection.transport.user)
    )
      return { ok: false, message: 'SSH host and username are required.' };
    if (connection.host.includes('invalid'))
      return {
        ok: false,
        message: 'Mock connection refused. Use a host without “invalid” to simulate success.',
      };
    return {
      ok: true,
      message: `Mock connection successful · ${connection.engine} · No network connection was made.`,
    };
  },
};
export function filterRows(
  rows: Row[],
  filter: string,
  sort: string,
  descending: boolean,
  type?: string,
): Row[] {
  const result = rows.filter((row) =>
    Object.values(row).some((value) =>
      String(value ?? '')
        .toLowerCase()
        .includes(filter.toLowerCase()),
    ),
  );
  if (!sort) return result;
  return result.sort((a, b) => {
    if (a[sort] === null || a[sort] === undefined)
      return b[sort] === null || b[sort] === undefined ? 0 : 1;
    if (b[sort] === null || b[sort] === undefined) return -1;
    return ((type &&
      /^(tinyint|smallint|mediumint|int|integer|bigint|decimal|numeric|real|float|double)/i.test(
        type,
      )) ||
    (typeof a[sort] === 'number' && typeof b[sort] === 'number')
      ? compareExactNumbers(String(a[sort] ?? ''), String(b[sort] ?? ''))
      : String(a[sort] ?? '').localeCompare(String(b[sort] ?? ''))) * (descending ? -1 : 1);
  });
}
export function selectQuery(
  sql: string,
  tables: Table[],
  browsePlan?: BrowseQueryPlan,
): { rows: Row[]; columns: string[]; columnTypes?: Record<string, string> } {
  if (browsePlan && sql.trim() === browsePlan.sql.trim()) {
    const table = tables.find((t) => t.name === browsePlan.table);
    if (!table) throw new Error('Selected table no longer exists.');
    return {
      columns: table.columns.map((c) => c.name),
      columnTypes: Object.fromEntries(table.columns.map((c) => [c.name, c.type])),
      rows: filterRows(
        table.rows,
        browsePlan.filter,
        browsePlan.sort,
        browsePlan.descending,
        table.columns.find((c) => c.name === browsePlan.sort)?.type,
      )
        .slice(browsePlan.offset, browsePlan.offset + browsePlan.limit)
        .map((row) => ({ ...row })),
    };
  }
  const match = sql
    .trim()
    .replace(/;$/, '')
    .match(
      /^select\s+(.+?)\s+from\s+[`"]?(\w+)[`"]?(?:\s+where\s+[`"]?(\w+)[`"]?\s*=\s*['"]?([^'";]+?)['"]?)?(?:\s+order\s+by\s+[`"]?(\w+)[`"]?(?:\s+(asc|desc))?)?(?:\s+limit\s+(\d+))?$/i,
    );
  if (!match)
    throw new Error(
      "Unsupported SQL. Try SELECT * FROM users WHERE status = 'active' ORDER BY id DESC LIMIT 25;",
    );
  const table = tables.find((t) => t.name.toLowerCase() === match[2]!.toLowerCase());
  if (!table) throw new Error(`Table '${match[2]}' does not exist.`);
  const columns =
    match[1] === '*'
      ? table.columns.map((c) => c.name)
      : match[1]!.split(',').map((c) => c.trim().replaceAll('`', ''));
  for (const name of [...columns, match[3], match[5]].filter(Boolean))
    if (!table.columns.some((c) => c.name === name)) throw new Error(`Unknown column '${name}'.`);
  let rows = [...table.rows];
  if (match[3]) rows = rows.filter((r) => String(r[match[3]!]) === match[4]);
  if (match[5])
    rows = filterRows(
      rows,
      '',
      match[5],
      match[6]?.toLowerCase() === 'desc',
      table.columns.find((c) => c.name === match[5])?.type,
    );
  if (match[7]) rows = rows.slice(0, Number(match[7]));
  return {
    rows: rows.map((r) => Object.fromEntries(columns.map((c) => [c, r[c] ?? null]))),
    columnTypes: Object.fromEntries(
      table.columns.filter((c) => columns.includes(c.name)).map((c) => [c.name, c.type]),
    ),
    columns,
  };
}

export function compareExactNumbers(a: string, b: string): number {
  const pattern = /^([+-]?)(\d*)(?:\.(\d*))?$/;
  const x = a.match(pattern),
    y = b.match(pattern);
  if (!x || !y) return a.localeCompare(b);
  const xw = x[2]!.replace(/^0+/, '') || '0',
    yw = y[2]!.replace(/^0+/, '') || '0',
    xf = x[3] || '',
    yf = y[3] || '';
  const xn = x[1] === '-' && /[1-9]/.test(xw + xf),
    yn = y[1] === '-' && /[1-9]/.test(yw + yf);
  if (xn !== yn) return xn ? -1 : 1;
  const size = Math.max(xf.length, yf.length),
    left = xf.padEnd(size, '0'),
    right = yf.padEnd(size, '0');
  const result = xw.length - yw.length || xw.localeCompare(yw) || left.localeCompare(right);
  return xn ? -result : result;
}
