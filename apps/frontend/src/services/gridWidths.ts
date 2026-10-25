export const MIN_COLUMN_WIDTH = 64,
  MAX_COLUMN_WIDTH = 600;
export const GRID_WIDTHS_KEY = 'stoneql-grid-widths:v1';
export interface GridColumn {
  name: string;
  type?: string;
  width?: number;
  resizable?: boolean;
}
export function clampWidth(width: number): number {
  return Math.round(
    Math.max(MIN_COLUMN_WIDTH, Math.min(MAX_COLUMN_WIDTH, Number.isFinite(width) ? width : 160)),
  );
}
export function defaultColumnWidth(column: GridColumn): number {
  if (column.width) return column.resizable === false ? column.width : clampWidth(column.width);
  const type = column.type?.toLowerCase() ?? '';
  return /json|text|blob/.test(type)
    ? 240
    : /timestamp|datetime|date|time/.test(type)
      ? 190
      : /enum|set|bool/.test(type)
        ? 130
        : /int|numeric|decimal|real|float|double/.test(type)
          ? 110
          : 180;
}
export function queryWidthKey(sql: string, columns: string[]): string {
  let hash = 2166136261;
  for (const c of JSON.stringify([sql, columns]))
    hash = Math.imul(hash ^ c.charCodeAt(0), 16777619);
  return (hash >>> 0).toString(36);
}
export function readGridWidths(
  storage: Pick<Storage, 'getItem'>,
  key: string,
): Record<string, number> {
  const raw = storage.getItem(GRID_WIDTHS_KEY);
  if (!raw) return {};
  const all = JSON.parse(raw);
  if (!all || typeof all !== 'object' || Array.isArray(all))
    throw new Error('Invalid saved grid-width preferences.');
  const widths = all[key];
  if (!widths || typeof widths !== 'object') return {};
  return Object.fromEntries(
    Object.entries(widths)
      .filter(([, value]) => typeof value === 'number' && Number.isFinite(value))
      .map(([name, value]) => [name, clampWidth(value as number)]),
  );
}
export function saveGridWidths(
  storage: Pick<Storage, 'getItem' | 'setItem'>,
  key: string,
  widths: Record<string, number> | null,
) {
  const raw = storage.getItem(GRID_WIDTHS_KEY),
    all = raw ? JSON.parse(raw) : {};
  if (!all || typeof all !== 'object' || Array.isArray(all))
    throw new Error('Invalid saved grid-width preferences.');
  if (widths === null) delete all[key];
  else
    all[key] = Object.fromEntries(
      Object.entries(widths).map(([name, width]) => [name, clampWidth(width)]),
    );
  storage.setItem(GRID_WIDTHS_KEY, JSON.stringify(all));
}
