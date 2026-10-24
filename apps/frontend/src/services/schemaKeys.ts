import type { Column } from '../types/index.ts';
export function canonicalValue(value: unknown, column: Column): string {
  const raw = String(value);
  if (/^(tinyint|smallint|mediumint|int|integer|bigint)\b/i.test(column.type) &&
    /^[+-]?\d+$/.test(raw))
    return BigInt(raw).toString();
  if (/^(decimal|numeric)\b/i.test(column.type) && /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(raw)) {
    const negative = raw.startsWith('-'), [whole = '', fraction = ''] = raw.replace(/^[+-]/, '').split('.');
    const digits = `${whole.replace(/^0+/, '') || '0'}.${fraction.replace(/0+$/, '')}`;
    return negative && /[1-9]/.test(digits) ? '-' + digits : digits;
  }
  return column.collation?.endsWith('_ci') ? raw.toLocaleLowerCase() : raw;
}
