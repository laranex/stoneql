import { canonicalValue } from './schemaKeys.ts';
export { canonicalValue } from './schemaKeys.ts';
import { rowIndexError, tableIndexes } from './indexes.ts';
import type { Column, Engine, Row, Table } from '../types/index.ts';
import { parseValues } from './schemaValues.ts';
export type ValidationResult =
  { valid: true; value: string | number | null | undefined } | { valid: false; error: string };
export interface ValidationContext {
  row?: Row;
  table?: Table;
  tables?: Table[];
  useDefault?: boolean;
}
export function choiceDefinition(
  column: Column,
  engine: Engine,
): { kind: 'enum' | 'set'; values: string[] } | null {
  if (engine === 'PostgreSQL' && column.enumValues)
    return { kind: 'enum', values: [...column.enumValues] };
  if (!['MySQL', 'MariaDB'].includes(engine)) return null;
  const match = column.type.match(/^(enum|set)\s*\(([\s\S]*)\)$/i);
  if (!match) return null;
  try {
    return { kind: match[1]!.toLowerCase() as 'enum' | 'set', values: parseValues(match[2]!) };
  } catch {
    return { kind: match[1]!.toLowerCase() as 'enum' | 'set', values: [] };
  }
}
export function hasColumnDefault(column: Column): boolean {
  return (
    !!(column.autoIncrement ?? column.primary) ||
    column.default !== undefined ||
    column.defaultKind === 'null'
  );
}
function validDate(value: string, engine: Engine): boolean {
  if (engine === 'PostgreSQL' && ['infinity', '-infinity'].includes(value)) return true;
  const m = value.match(/^(\d{4,7})-(\d{2})-(\d{2})( BC)?$/);
  if (!m) return false;
  const y = Number(m[1]),
    month = Number(m[2]),
    day = Number(m[3]);
  if (!y || month < 1 || month > 12) return false;
  if (engine !== 'PostgreSQL' && (m[4] || y < 1000 || y > 9999)) return false;
  if (engine === 'PostgreSQL' && y > (m[4] ? 4713 : 5874897)) return false;
  const leap = y % 4 === 0 && (y % 100 !== 0 || y % 400 === 0);
  return (
    day >= 1 && day <= [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1]!
  );
}
function validTime(value: string, engine: Engine, duration = false, timezone = false): boolean {
  const m = value.match(/^([+-]?)(\d{2,3}):(\d{2}):(\d{2})(?:\.(\d{1,6}))?(Z|[+-]\d{2}:\d{2})?$/);
  if (!m) return false;
  const h = Number(m[2]),
    min = Number(m[3]),
    sec = Number(m[4]);
  if (min > 59 || sec > 59) return false;
  if (duration && ['MySQL', 'MariaDB'].includes(engine)) {
    if (h > 838 || m[6]) return false;
  } else if (
    m[1] ||
    (h > 23 &&
      !(engine === 'PostgreSQL' && h === 24 && min === 0 && sec === 0 && !/[1-9]/.test(m[5] ?? '')))
  )
    return false;
  if (m[6]) {
    if (!timezone) return false;
    if (m[6] !== 'Z') {
      const [hours, minutes] = m[6].slice(1).split(':').map(Number);
      if (hours! > 15 || minutes! > 59) return false;
    }
  }
  return true;
}
export function validateValue(
  column: Column,
  input: string | number | null | undefined,
  engine: Engine,
  context: ValidationContext = {},
): ValidationResult {
  const fail = (error: string): ValidationResult => ({
    valid: false,
    error: `${column.name}: ${error}`,
  });
  if (context.useDefault) {
    if (!hasColumnDefault(column)) return fail('No database default is defined.');
    if (column.defaultKind === 'null' && !column.nullable)
      return fail('NULL default conflicts with NOT NULL.');
    return { valid: true, value: undefined };
  }
  if (input === undefined) return fail('Enter a value or explicitly choose NULL/default.');
  if (input === null)
    return column.nullable && !column.primary
      ? { valid: true, value: null }
      : fail('NULL is not allowed.');
  const raw = String(input),
    type = column.type.trim().toLowerCase(),
    base = type
      .replace(/\([^)]*\)/g, '')
      .replace(/\s+unsigned\b/, '')
      .trim();
  const unsigned = engine !== 'SQLite' && (column.unsigned || /\bunsigned\b/.test(type));
  const choice = choiceDefinition(column, engine);
  if (choice) {
    const selections = choice.kind === 'set' ? (raw === '' ? [] : raw.split(',')) : [raw];
    if (
      selections.some((v) => !choice.values.includes(v)) ||
      new Set(selections).size !== selections.length
    )
      return fail(
        `Choose ${choice.kind === 'set' ? 'distinct values' : 'a value'} from the schema definition.`,
      );
  } else if (/^(tinyint|smallint|mediumint|int|integer|bigint)$/.test(base)) {
    if (!/^[+-]?\d+$/.test(raw))
      return fail('Enter a whole number without spaces or decimal points.');
    if (typeof input === 'number' && !Number.isSafeInteger(input))
      return fail('Use the exact integer as text; this JavaScript number is unsafe.');
    const bits =
      engine === 'SQLite'
        ? 64
        : base === 'tinyint'
          ? 8
          : base === 'smallint'
            ? 16
            : base === 'mediumint'
              ? 24
              : base === 'bigint'
                ? 64
                : 32;
    const min = unsigned ? 0n : -(1n << BigInt(bits - 1)),
      max = unsigned ? (1n << BigInt(bits)) - 1n : (1n << BigInt(bits - 1)) - 1n;
    const value = BigInt(raw);
    if (value < min || value > max) return fail(`Out of range (${min} to ${max}).`);
  } else if (/^(decimal|numeric)$/.test(base)) {
    if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(raw))
      return fail('Enter an exact decimal without exponent notation or spaces.');
    if (unsigned && raw.startsWith('-') && /[1-9]/.test(raw))
      return fail('Negative values are not allowed for UNSIGNED.');
    const args = type.match(/\((\d+)\s*,\s*(\d+)\)/);
    if (args && engine !== 'SQLite') {
      const p = Number(args[1]),
        s = Number(args[2]),
        [whole = '', fraction = ''] = raw.replace(/^[+-]/, '').split('.');
      if (whole.replace(/^0+/, '').length > p - s || fraction.length > s)
        return fail(
          `DECIMAL(${p},${s}) allows ${p - s} integer digits and ${s} fractional digits; values are never rounded.`,
        );
    }
  } else if (/^(real|float|double|double precision)$/.test(base)) {
    if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(raw))
      return fail('Enter a valid finite numeric value.');
    const value = Number(raw),
      single =
        (engine === 'PostgreSQL' && base === 'real') || (engine !== 'SQLite' && base === 'float');
    if (
      !Number.isFinite(value) ||
      (single && Math.abs(value) > 3.4028234663852886e38) ||
      (value === 0 && /[1-9]/.test(raw.split(/[eE]/)[0]!))
    )
      return fail('Floating-point value is out of range.');
    if (unsigned && value < 0) return fail('Negative values are not allowed for UNSIGNED.');
  } else if (/^(varchar|char|character|character varying)$/.test(base)) {
    const length = type.match(/\((\d+)\)/);
    if (length && engine !== 'SQLite' && [...raw].length > Number(length[1]))
      return fail(`Maximum length is ${length[1]} characters.`);
  } else if (base === 'date') {
    if (!validDate(raw, engine)) return fail('Enter a valid calendar date (YYYY-MM-DD).');
  } else if (/^(time|timetz|time with time zone|time without time zone)$/.test(base)) {
    if (
      !validTime(raw, engine, true, engine === 'PostgreSQL' && /timetz|with time zone/.test(base))
    )
      return fail('Enter a valid time (HH:MM:SS, with an optional fractional part).');
  } else if (
    /^(datetime|timestamp|timestamptz|timestamp with time zone|timestamp without time zone)$/.test(
      base,
    )
  ) {
    if (!(engine === 'PostgreSQL' && ['infinity', '-infinity'].includes(raw))) {
      const match = raw.match(/^(\d{4,7}-\d{2}-\d{2})[ T](.+)$/);
      if (
        !match ||
        !validDate(match[1]!, engine) ||
        !validTime(
          match[2]!,
          engine,
          false,
          engine === 'PostgreSQL' && /timestamptz|with time zone/.test(base),
        )
      )
        return fail('Enter a valid timestamp without changing its timezone or raw format.');
    }
  } else if (base === 'boolean' || base === 'bool') {
    const allowed =
      engine === 'PostgreSQL'
        ? ['true', 'false', 't', 'f', '1', '0', 'yes', 'no', 'on', 'off']
        : ['true', 'false', '1', '0'];
    if (!allowed.includes(raw.toLowerCase()))
      return fail('Enter a valid boolean value (true, false, 1, or 0).');
  } else if (base === 'json' || base === 'jsonb') {
    try {
      JSON.parse(raw);
    } catch {
      return fail('Enter valid JSON.');
    }
  }
  const temporalPrecision = type.match(/^(?:time|datetime|timestamp)\((\d+)\)/);
  if (temporalPrecision) {
    const fraction = raw.match(/\.(\d+)(?:Z|[+-]\d{2}:\d{2})?$/);
    if (fraction && fraction[1]!.length > Number(temporalPrecision[1]))
      return fail(`At most ${temporalPrecision[1]} fractional-second digits are allowed.`);
  }
  if (context.table) {
    const target = context.row ? context.table : {...context.table,indexes:tableIndexes(context.table).filter(i=>i.columns.length===1&&i.columns[0]!.column===column.name)};
    const error = rowIndexError(target, { ...context.row, [column.name]: input }, context.row, column.name);
    if (error) return fail(error);
  }
  if (column.foreign && context.tables) {
    const [tableName, columnName] = column.foreign.split('.');
    const target = context.tables.find((t) => t.name === tableName),
      targetColumn = target?.columns.find((c) => c.name === columnName);
    if (
      target &&
      targetColumn &&
      !target.rows.some(
        (r) => canonicalValue(r[columnName!], targetColumn) === canonicalValue(input, targetColumn),
      )
    )
      return fail(`No referenced record exists in ${column.foreign}.`);
  }
  return { valid: true, value: input };
}
