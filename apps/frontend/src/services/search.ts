import type { Table, Row } from '../types/index.ts';
import { compareExactNumbers } from './mockTransport.ts';
export const searchOperators = ['contains', '=', '!=', '>', '>=', '<', '<=', 'LIKE', 'NOT LIKE', 'IN', 'NOT IN', 'BETWEEN', 'IS NULL', 'IS NOT NULL'] as const;
export type SearchOperator = typeof searchOperators[number];
export interface SearchCondition { column: string; operator: SearchOperator; value: string }
export function searchRows(table: Table, conditions: SearchCondition[]): Row[] {
  const predicates = conditions.filter(c => c.value !== '' || c.operator !== 'contains').map(condition => {
    const { column, operator, value } = condition;
    const definition = table.columns.find(c => c.name === column);
    if (!definition || !searchOperators.includes(operator)) throw new Error('Invalid search condition.');
    const numeric = /^(tinyint|smallint|mediumint|int|integer|bigint|decimal|numeric|real|float|double)/i.test(definition.type);
    const compare = (a: string, b: string) => numeric ? compareExactNumbers(a,b) : a.localeCompare(b);
    let pattern: RegExp | undefined;
    if (operator.includes('LIKE')) {
      let source = '^', escaped = false;
      for (const character of value) {
        if (!escaped && character === '\\') { escaped = true; continue; }
        source += !escaped && character === '%' ? '.*' : !escaped && character === '_' ? '.' : character.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); escaped = false;
      }
      if (escaped) throw new Error(`${column}: LIKE cannot end with an escape character.`);
      pattern = new RegExp(source + '$', 'isu');
    }
    const parts = value.split(',').map(v=>v.trim());
    if (operator === 'BETWEEN' && parts.length !== 2) throw new Error(`${column}: BETWEEN requires two comma-separated bounds.`);
    if (numeric && !['contains','LIKE','NOT LIKE','IS NULL','IS NOT NULL'].includes(operator) && parts.some(v=>!/^[-+]?\d+(?:\.\d+)?$/.test(v))) throw new Error(`${column}: enter valid numeric search values.`);
    return (row: Row) => {
      const raw = row[column];
      if (operator === 'IS NULL') return raw === null;
      if (operator === 'IS NOT NULL') return raw !== null && raw !== undefined;
      if (raw === null || raw === undefined) return false;
      const text = String(raw), order = compare(text,value);
      switch(operator) {
        case '=': return order === 0;
        case '!=': return order !== 0;
        case '>': return order > 0;
        case '>=': return order >= 0;
        case '<': return order < 0;
        case '<=': return order <= 0;
        case 'LIKE': return pattern!.test(text);
        case 'NOT LIKE': return !pattern!.test(text);
        case 'IN': return parts.some(v=>compare(text,v)===0);
        case 'NOT IN': return parts.every(v=>compare(text,v)!==0);
        case 'BETWEEN': return compare(text,parts[0]!)>=0 && compare(text,parts[1]!)<=0;
        default: return text.toLowerCase().includes(value.toLowerCase());
      }
    };
  });
  return table.rows.filter(row=>predicates.every(predicate=>predicate(row)));
}
