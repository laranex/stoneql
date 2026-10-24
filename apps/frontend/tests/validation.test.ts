import { test } from 'node:test';
import assert from 'node:assert/strict';
import { validateValue, choiceDefinition } from '../src/services/valueValidation.ts';
import type { Column, Engine } from '../src/types/index.ts';
const col = (type: string, extra: Partial<Column> = {}): Column => ({
  name: 'value',
  type,
  nullable: false,
  ...extra,
});
function valid(
  type: string,
  value: string | null,
  engine: Engine = 'MySQL',
  extra: Partial<Column> = {},
) {
  const result = validateValue(col(type, extra), value, engine);
  assert.equal(result.valid, true, `${type} ${value}: ${'error' in result ? result.error : ''}`);
  if (result.valid) assert.equal(result.value, value);
}
function invalid(
  type: string,
  value: string | null,
  engine: Engine = 'MySQL',
  extra: Partial<Column> = {},
) {
  assert.equal(validateValue(col(type, extra), value, engine).valid, false, `${type} ${value}`);
}
test('INT and BIGINT enforce exact signed and unsigned boundaries without Number coercion', () => {
  for (const engine of ['MySQL', 'MariaDB', 'PostgreSQL'] as const) {
    valid('int', '2147483647', engine);
    valid('int', '-2147483648', engine);
    invalid('int', '2147483648', engine);
    invalid('int', '-2147483649', engine);
    valid('bigint', '9223372036854775807', engine);
    valid('bigint', '-9223372036854775808', engine);
    invalid('bigint', '9223372036854775808', engine);
  }
  valid('bigint unsigned', '18446744073709551615');
  invalid('bigint unsigned', '18446744073709551616');
  invalid('int unsigned', '-1');
  invalid('int', '1.1');
  invalid('int', ' 1');
  invalid('int', '1e2');
  valid('int', '+0001');
  valid('integer', '9223372036854775807', 'SQLite');
  invalid('integer', '9223372036854775808', 'SQLite');
  assert.equal(validateValue(col('bigint'), Number.MAX_SAFE_INTEGER + 1, 'MySQL').valid, false);
});
test('DECIMAL precision and scale remain exact even far beyond safe JavaScript integers', () => {
  valid('decimal(30,10)', '12345678901234567890.1234567890');
  invalid('decimal(30,10)', '123456789012345678901.1234567890');
  invalid('decimal(12,2)', '1.234');
  invalid('decimal(12,2)', '1.2300');
  valid('decimal(12,2)', '00001.20');
  valid('decimal(2,2)', '0.12');
  invalid('decimal(2,2)', '1.12');
  invalid('decimal(12,2)', 'NaN');
  invalid('decimal(12,2)', '1e5');
  invalid('decimal(12,2) unsigned', '-0.01');
  valid('numeric(2,1)', '123456.123', 'SQLite');
});
test('character limits count Unicode characters and preserve spaces and empty strings', () => {
  valid('varchar(2)', '😊é');
  invalid('varchar(2)', '😊éx');
  valid('char(4)', '  a ');
  valid('varchar(2)', '');
  valid('varchar(1)', 'long value', 'SQLite');
});
test('NULL and defaults are distinct from empty strings and never silently substituted', () => {
  invalid('int', '');
  valid('varchar(5)', '');
  invalid('varchar(5)', null);
  valid('int', null, 'MySQL', { nullable: true });
  invalid('int', null, 'MySQL', { nullable: true, primary: true });
  assert.deepEqual(
    validateValue(col('int', { default: '42' }), 'ignored', 'MySQL', { useDefault: true }),
    { valid: true, value: undefined },
  );
  assert.equal(validateValue(col('int'), '', 'MySQL', { useDefault: true }).valid, false);
});
test('calendar validation rejects invalid dates without timezone conversion', () => {
  valid('date', '2024-02-29');
  invalid('date', '2023-02-29');
  invalid('date', '2024-04-31');
  invalid('date', '0000-00-00');
  invalid('date', '2026-13-01');
  valid('date', '12000-02-29', 'PostgreSQL');
  valid('date', 'infinity', 'PostgreSQL');
  valid('date', '0044-03-15 BC', 'PostgreSQL');
});
test('TIME and timestamps preserve duration, fractions, and explicit PostgreSQL offsets', () => {
  valid('time', '-838:59:59');
  invalid('time', '839:00:00');
  invalid('time', '12:60:00');
  invalid('time', '-01:00:00', 'PostgreSQL');
  valid('time', '24:00:00', 'PostgreSQL');
  valid('datetime', '2026-10-24 12:34:56.123456');
  invalid('datetime', '2026-02-30 12:00:00');
  invalid('datetime', '2026-10-24 25:00:00');
  invalid('datetime(3)', '2026-10-24 12:00:00.1234');
  valid('timestamptz', '2026-10-24T12:00:00+07:00', 'PostgreSQL');
  valid('timestamp(6) with time zone', '2026-10-24 12:00:00.123456Z', 'PostgreSQL');
  invalid('timestamp', '2026-10-24 12:00:00+07:00', 'PostgreSQL');
});
test('BOOLEAN and JSON validate syntax while preserving raw textual representations', () => {
  valid('boolean', 'TRUE');
  invalid('boolean', '2');
  valid('boolean', 'yes', 'PostgreSQL');
  invalid('boolean', 'yes', 'MySQL');
  valid('json', '{"large":123456789012345678901234567890}');
  valid('json', 'null');
  invalid('json', '{bad}');
  invalid('json', '{"a":Infinity}');
});
test('ENUM/SET are schema-driven, preserve exact values, and reject invalid selections', () => {
  const type = "enum('active','inactive','can''t')";
  valid(type, 'active');
  valid(type, "can't");
  invalid(type, 'ACTIVE');
  invalid(type, 'unknown');
  valid("set('read','write')", 'read,write');
  valid("set('read','write')", '');
  invalid("set('read','write')", 'read,read');
  invalid("set('read','write')", 'read,admin');
  valid('user_status', 'active', 'PostgreSQL', {
    enumValues: ['active', 'inactive'],
    enumName: 'user_status',
  });
  invalid('user_status', 'unknown', 'PostgreSQL', { enumValues: ['active', 'inactive'] });
  assert.equal(choiceDefinition(col('varchar(20)'), 'MySQL'), null);
  assert.equal(choiceDefinition(col('varchar(20)'), 'PostgreSQL'), null);
});
test('unique and foreign key constraints use actual schema metadata and exclude the edited row', () => {
  const c = col('bigint', { primary: true });
  const row = { value: '01' },
    table = { name: 'test', columns: [c], rows: [row] };
  assert.equal(validateValue(c, '1', 'MySQL', { table }).valid, false);
  assert.equal(validateValue(c, '1', 'MySQL', { table, row }).valid, true);
  const fk = col('int', { foreign: 'test.value' });
  assert.equal(validateValue(fk, '1', 'MySQL', { tables: [table] }).valid, true);
  assert.equal(validateValue(fk, '2', 'MySQL', { tables: [table] }).valid, false);
});
