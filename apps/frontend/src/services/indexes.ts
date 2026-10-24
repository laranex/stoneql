import type { Table, Engine, Row } from '../types/index.ts';
import type { TableIndex } from '../types/objects.ts';
import { canonicalValue } from './schemaKeys.ts';
import { objectCapabilities } from './objectCapabilities.ts';
export function tableIndexes(table: Table): TableIndex[] {
  if (table.indexes)
    return table.indexes;
  return table.columns.filter(c => c.primary || c.unique || c.indexed).map(c => ({ name: c.primary ? 'PRIMARY' : `${c.name}_${c.unique ? 'unique' : 'index'}`, kind: c.primary ? 'primary' : c.unique ? 'unique' : 'normal', method: 'btree', columns: [{ column: c.name, order: 'ASC' }] }));
}
export function rowIndexError(table: Table, row: Row, exclude?: Row, focus?: string): string | undefined {
  for (const index of tableIndexes(table).filter(i => (i.kind === 'primary' || i.kind === 'unique') && (!focus || i.columns.some(p => p.column === focus)))) {
    const columns = index.columns.map(part => table.columns.find(c => c.name === part.column)!);
    if (columns.some(c => !c))
      return `Index ${index.name} references a missing column.`;
    if (columns.some(c => row[c.name] === null || row[c.name] === undefined)) {
      if (index.kind === 'primary')
        return `Primary key ${index.name} cannot contain NULL.`;
      continue;
    }
    if (table.rows.some(other => other !== exclude && columns.every(c => other[c.name] !== null && other[c.name] !== undefined && canonicalValue(other[c.name], c) === canonicalValue(row[c.name], c))))
      return 'This key value already exists.';
  }
}
export function validateIndex(table: Table, draft: TableIndex, engine: Engine, original = ''): string[] {
  const errors: string[] = [], cap = objectCapabilities[engine];
  if (table.kind && table.kind !== 'table' && table.kind !== 'materialized-view')
    errors.push('Indexes cannot be created on ordinary views.');
  if (!/^[A-Za-z_]\w*$/.test(draft.name))
    errors.push('Enter a valid index name.');
  if (tableIndexes(table).some(i => i.name !== original && i.name.toLowerCase() === draft.name.toLowerCase()))
    errors.push('Index name already exists.');
  if (['MySQL', 'MariaDB'].includes(engine) && ((draft.kind === 'primary' && draft.name !== 'PRIMARY') || (draft.kind !== 'primary' && draft.name.toUpperCase() === 'PRIMARY')))
    errors.push('MySQL/MariaDB primary keys are named PRIMARY; other indexes must use another name.');
  if (!cap.indexKinds.includes(draft.kind))
    errors.push('Index type is not supported by this engine.');
  if (!cap.methods.includes(draft.method) && !(['MySQL', 'MariaDB'].includes(engine) && ((draft.kind === 'spatial' && draft.method === 'rtree') || (draft.kind === 'fulltext' && draft.method === 'fulltext'))))
    errors.push('Index method is not supported by this engine.');
  if (!draft.columns.length || new Set(draft.columns.map(p => p.column)).size !== draft.columns.length)
    errors.push('Select one or more distinct columns in key order.');
  if (draft.columns.some(p => !table.columns.some(c => c.name === p.column) || !['ASC', 'DESC'].includes(p.order)))
    errors.push('Select existing columns and valid directions.');
  if (draft.kind === 'primary' && tableIndexes(table).some(i => i.kind === 'primary' && i.name !== original))
    errors.push('Only one primary key is allowed.');
  if (draft.kind === 'primary' && table.kind === 'materialized-view')
    errors.push('Materialized views support unique indexes, not primary-key constraints.');
  if (['primary', 'unique'].includes(draft.kind) && draft.method !== 'btree')
    errors.push('Primary and unique indexes require B-tree in this prototype.');
  if (['hash', 'spgist'].includes(draft.method) && draft.columns.length !== 1)
    errors.push('This index method supports one key column only.');
  if (draft.columns.some(p => p.order === 'DESC') && (!['primary', 'unique', 'normal'].includes(draft.kind) || draft.method !== 'btree'))
    errors.push('Descending keys require a B-tree index.');
  for (const part of draft.columns) {
    const column = table.columns.find(c => c.name === part.column);
    if (!column)
      continue;
    if (engine === 'PostgreSQL') {
      const type = column.type.toLowerCase();
      if (draft.method === 'gin' && !/jsonb|tsvector|\[\]/.test(type))
        errors.push('GIN requires jsonb, tsvector or array columns with default operator classes in this prototype.');
      if (draft.method === 'gist' && !/^(point|box|circle|polygon|inet|cidr|tsvector|.*range)$/.test(type))
        errors.push('GiST needs a supported geometric, range, network or tsvector default operator class.');
      if (draft.method === 'spgist' && !/^(point|box|polygon|inet|cidr|text|.*range)$/.test(type))
        errors.push('SP-GiST needs a supported default operator class.');
      if (draft.method === 'brin' && !/^(smallint|int|integer|bigint|numeric|decimal|real|double precision|text|varchar|char|date|timestamp|timestamptz|uuid)/.test(type))
        errors.push('BRIN needs a supported scalar default operator class in this prototype.');
    }
    if (draft.kind === 'primary' && column.nullable)
      errors.push(`${column.name}: primary-key columns must be NOT NULL.`);
    if (draft.kind === 'fulltext' && !/^(char|varchar|text|tinytext|mediumtext|longtext)\b/i.test(column.type))
      errors.push('FULLTEXT requires CHAR, VARCHAR or TEXT columns.');
    if (draft.kind === 'spatial' && (!/^(geometry|point|linestring|polygon|multipoint|multilinestring|multipolygon|geometrycollection)\b/i.test(column.type) || column.nullable))
      errors.push('SPATIAL requires non-null geometry columns.');
  }
  if (draft.kind === 'spatial' && (draft.columns.length !== 1 || draft.method !== 'rtree'))
    errors.push('SPATIAL requires one geometry column and R-tree.');
  if (draft.kind === 'fulltext' && draft.method !== 'fulltext')
    errors.push('FULLTEXT requires the engine-managed full-text method.');
  if (errors.length)
    return errors;
  const candidate = { ...table, indexes: [...tableIndexes(table).filter(i => i.name !== original), draft] };
  for (const row of table.rows) {
    const error = rowIndexError(candidate, row, row);
    if (error) {
      errors.push(error);
      break;
    }
  }
  return errors;
}
export function syncIndexFlags(table: Table) {
  const indexes = tableIndexes(table);
  for (const column of table.columns) {
    column.autoIncrement ??= false;
    column.indexKinds = indexes.filter(i => i.columns.some(p => p.column === column.name)).map(i => i.kind);
    column.primary = indexes.some(i => i.kind === 'primary' && i.columns.some(p => p.column === column.name));
    column.unique = indexes.some(i => i.kind === 'unique' && i.columns.length === 1 && i.columns[0]!.column === column.name);
    column.indexed = indexes.some(i => i.kind !== 'primary' && i.kind !== 'unique' && i.columns.some(p => p.column === column.name));
  }
}
export function saveIndex(table: Table, draft: TableIndex, engine: Engine, original = '', database?: import('../types/index.ts').Database) {
  if (original && !tableIndexes(table).some(i => i.name === original))
    throw new Error('Index no longer exists.');
  if (database && (engine === 'PostgreSQL' || (engine === 'SQLite' && draft.kind !== 'primary'))) {
    const others = [...database.tables, ...(database.views || [])].filter(other => other !== table && (other.schema || '') === (table.schema || ''));
    if (others.some(other => tableIndexes(other).some(index => (index.kind !== 'primary' || engine === 'PostgreSQL') && index.name.toLowerCase() === draft.name.toLowerCase())))
      throw new Error('Index name already exists in this schema.');
  }
  const errors = validateIndex(table, draft, engine, original);
  if (errors.length)
    throw new Error(errors.join(' '));
  const next = original ? tableIndexes(table).map(i => i.name === original ? structuredClone(draft) : i) : [...tableIndexes(table), structuredClone(draft)];
  assertIndexDependencies(table, next, database, engine);
  table.indexes = next;
  syncIndexFlags(table);
}
export function dropIndex(table: Table, name: string, database?: import('../types/index.ts').Database, engine: Engine = 'MySQL') {
  const existing = tableIndexes(table).find(i => i.name === name);
  if (!existing)
    throw new Error('Index no longer exists.');
  const next = tableIndexes(table).filter(i => i.name !== name);
  assertIndexDependencies(table, next, database, engine);
  table.indexes = next;
  syncIndexFlags(table);
}
export function reconcileColumnIndexes(table: Table, column: import('../types/index.ts').Column, original: string) {
  const before = table.columns.find(c => c.name === original);
  const requested = column.primary ? 'primary' : column.unique ? 'unique' : column.indexed ? 'normal' : undefined;
  const previous = before?.primary ? 'primary' : before?.unique ? 'unique' : before?.indexed ? 'normal' : undefined;
  const existing = tableIndexes(table);
  if (existing.some(i => i.columns.length > 1 && i.columns.some(p => p.column === original)) && requested !== previous)
    throw new Error('Change composite indexes in the Indexes section first.');
  const next = existing.filter(i => !(i.columns.length === 1 && i.columns[0]!.column === original && requested !== previous)).map(i => ({ ...i, columns: i.columns.map(p => ({ ...p, column: p.column === original ? column.name : p.column })) }));
  if (requested && (!original || requested !== previous))
    next.push({ name: requested === 'primary' ? 'PRIMARY' : `${column.name}_${requested}`, kind: requested, method: 'btree', columns: [{ column: column.name, order: 'ASC' }] });
  return next;
}
export function assertIndexDependencies(table: Table, next: TableIndex[], database?: import('../types/index.ts').Database, engine: Engine = 'MySQL') {
  for (const column of table.columns) {
    if (column.autoIncrement && engine !== 'PostgreSQL' && !next.some(i => i.columns[0]?.column === column.name && (engine === 'SQLite' ? i.kind === 'primary' && i.columns.length === 1 : ['primary', 'unique', 'normal'].includes(i.kind))))
      throw new Error('Keep a leading index on the generated column.');
    if (database?.tables.some(t => t.columns.some(c => c.foreign === `${table.name}.${column.name}`)) && !next.some(i => ['primary', 'unique'].includes(i.kind) && i.columns.length === 1 && i.columns[0]!.column === column.name))
      throw new Error('This index is required by a foreign-key reference.');
  }
}
