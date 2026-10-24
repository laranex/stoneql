import type { Row, Table } from '../types/index.ts';
/** Strict CSV parsing. Values remain raw strings; omitted columns use schema defaults. */
export function csvRecords(text: string, table: Table): Row[] {
  const records: string[][] = [];
  let record: string[] = [], value = '', quoted = false, closed = false, started = false;
  const field = () => { record.push(value); value = ''; closed = false; started = false; };
  const line = () => { field(); records.push(record); record = []; };
  text = text.replace(/^\uFEFF/, '');
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!;
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') { value += '"'; i++; } else { quoted = false; closed = true; }
      } else value += c;
    } else if (c === ',') field();
    else if (c === '\n' || c === '\r') { if (c === '\r' && text[i + 1] === '\n') i++; line(); }
    else if (c === '"' && !started && !closed) { quoted = true; started = true; }
    else { if (closed || c === '"') throw new Error(`CSV record ${records.length + 1}: unexpected character outside a quoted field.`); value += c; started = true; }
  }
  if (quoted) throw new Error('CSV has an unclosed quoted field.');
  if (started || closed || record.length) line();
  const headers = records.shift();
  if (!headers?.length || new Set(headers).size !== headers.length || headers.some(h => !table.columns.some(c => c.name === h)))
    throw new Error('CSV headers must be distinct column names from the selected table.');
  return records.map((values, index) => {
    if (values.length !== headers.length) throw new Error(`CSV record ${index + 2}: expected ${headers.length} fields, received ${values.length}.`);
    return Object.fromEntries(headers.map((name, i) => [name, values[i]!]));
  });
}
