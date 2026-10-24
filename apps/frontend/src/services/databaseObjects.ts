import { validateIndex, syncIndexFlags, tableIndexes } from './indexes.ts';
import type { Database, Table, Engine } from '../types/index.ts';
import type { Routine, Trigger, ScheduledEvent } from '../types/objects.ts';
import { objectCapabilities, relations } from './objectCapabilities.ts';
import { selectQuery } from './mockTransport.ts';
import { validateValue } from './valueValidation.ts';
export function writableTable(table: Table) { if (table.kind && table.kind !== 'table')
  throw new Error('Views are read-only in this prototype.'); }
function validName(name: string, names: string[], original = '') {
  if (!/^[A-Za-z_]\w*$/.test(name))
    throw new Error('Use a name starting with a letter or underscore, followed by letters, numbers or underscores.');
  if (names.some(n => n !== original && n.toLowerCase() === name.toLowerCase()))
    throw new Error('An object with this name already exists.');
  if (original && !names.includes(original))
    throw new Error('The object no longer exists.');
}
function body(definition: string) { if (!definition.trim())
  throw new Error('Enter a SQL definition. It is stored as text and is not executed.'); }
export function saveView(database: Database, draft: Pick<Table, 'name' | 'definition' | 'kind'>, engine: Engine, original = '') {
  validName(draft.name, relations(database).map(t => t.name), original);
  body(draft.definition || '');
  if (!['view', 'materialized-view'].includes(draft.kind || ''))
    throw new Error('Choose a view type.');
  if (draft.kind === 'materialized-view' && !objectCapabilities[engine].materializedViews)
    throw new Error('Materialized views require PostgreSQL.');
  const old = database.views?.find(v => v.name === original);
  if (original && !old)
    throw new Error('Select an existing view.');
  if (old && old.kind !== draft.kind)
    throw new Error('Drop and recreate the view to change its kind.');
  const changed = old?.definition !== draft.definition;
  const next: Table = { ...old, ...draft, columns: changed ? [] : old?.columns || [], rows: changed ? [] : old?.rows || [], indexes: changed ? [] : old?.indexes, refreshedAt: changed ? undefined : old?.refreshedAt };
  database.views = [...(database.views || []).filter(v => v !== old), next];
  if(original && original!==next.name) for(const trigger of database.triggers||[]) if(trigger.table===original) trigger.table=next.name;
  return next;
}
export function previewView(database: Database, view: Table, engine: Engine = 'PostgreSQL') {
  // Deliberately evaluate only the existing safe SELECT subset against base mock tables.
  const result = selectQuery(view.definition || '', database.tables);
  const source = database.tables.find(t => new RegExp(`\\b${t.name}\\b`, 'i').test(view.definition || ''));
  const columns = result.columns.map(name => ({ ...source?.columns.find(c => c.name === name), name, type: result.columnTypes?.[name] || 'text', nullable: true, primary: false, unique: false, indexed: false, autoIncrement: false, foreign: undefined, indexKinds: undefined, default: undefined, defaultKind: undefined }));
  const candidate = { ...view, columns, rows: result.rows };
  for (const index of view.indexes || []) {
    const errors = validateIndex(candidate, index, engine, index.name);
    if (errors.length)
      throw new Error(errors.join(' '));
  }
  view.columns = columns;
  view.rows = result.rows;
  if (view.indexes)
    syncIndexFlags(view);
  view.refreshedAt = new Date().toISOString();
  return result;
}
export function dropView(database: Database, name: string) {
  if (!database.views?.some(v => v.name === name))
    throw new Error('View no longer exists.');
  database.views = database.views.filter(v => v.name !== name);
  database.triggers = database.triggers?.filter(t => t.table !== name);
}
export function saveRoutine(database: Database, draft: Routine, engine: Engine, original = '') {
  if (!objectCapabilities[engine].routines)
    throw new Error('Stored routines are not supported by this engine.');
  validName(draft.name, (database.routines || []).map(r => r.name), original);
  body(draft.definition);
  if (!['procedure', 'function'].includes(draft.kind))
    throw new Error('Choose procedure or function.');
  if (draft.kind === 'function' && !draft.returns.trim())
    throw new Error('Functions require a return type.');
  if (!(['MySQL', 'MariaDB'].includes(engine) ? ['SQL'] : ['sql', 'plpgsql']).includes(draft.language))
    throw new Error('Choose a supported routine language.');
  const seen = new Set<string>();
  for (const parameter of draft.parameters) {
    validName(parameter.name, []);
    if (seen.has(parameter.name.toLowerCase()))
      throw new Error('Parameter names must be distinct.');
    seen.add(parameter.name.toLowerCase());
    if (!parameter.type.trim() || !['IN', 'OUT', 'INOUT'].includes(parameter.mode))
      throw new Error('Each parameter needs a type and mode.');
    if (draft.kind === 'function' && ['MySQL', 'MariaDB'].includes(engine) && parameter.mode !== 'IN')
      throw new Error('MySQL/MariaDB function parameters are input-only.');
  }
  database.routines = [...(database.routines || []).filter(r => r.name !== original), structuredClone(draft)];
}
export function simulateRoutine(routine: Routine, parameters: Record<string, string | null>, engine: Engine) {
  if (!objectCapabilities[engine].routines)
    throw new Error('Stored routines are not supported by this engine.');
  for (const p of routine.parameters.filter(p => p.mode !== 'OUT')) {
    if (!Object.prototype.hasOwnProperty.call(parameters, p.name))
      throw new Error(`${p.name}: enter a value or select NULL.`);
    if (!/^(tinyint|smallint|mediumint|int|integer|bigint|decimal|numeric|real|float|double|char|varchar|text|tinytext|mediumtext|longtext|date|time|datetime|timestamp|timestamptz|boolean|bool|json|jsonb|enum|set)\b/i.test(p.type))
      throw new Error(`${p.name}: this parameter type cannot be validated by the mock. No call was executed.`);
    const checked = validateValue({ name: p.name, type: p.type, nullable: true }, parameters[p.name], engine);
    if (!checked.valid)
      throw new Error(`${p.name}: ${checked.error}`);
  }
  return `Simulated ${routine.kind} call: ${routine.name}. ${routine.parameters.filter(p => p.mode !== 'OUT').length} input(s) validated. Body not executed; return and OUT values are not computed.`;
}
export function saveTrigger(database: Database, draft: Trigger, engine: Engine, original = '') {
  validName(draft.name, (database.triggers || []).map(t => t.name), original);
  body(draft.definition);
  const target = relations(database).find(t => t.name === draft.table);
  if (!target)
    throw new Error('Select an existing table or view.');
  if (!objectCapabilities[engine].triggerTimings.includes(draft.timing) || !['INSERT', 'UPDATE', 'DELETE'].includes(draft.event))
    throw new Error('Unsupported trigger timing or event.');
  if (target.kind === 'materialized-view')
    throw new Error('Triggers are not supported on materialized views.');
  if (draft.timing === 'INSTEAD OF' ? target.kind !== 'view' : target.kind === 'view')
    throw new Error('Views require INSTEAD OF; ordinary tables require BEFORE or AFTER in this prototype.');
  database.triggers = [...(database.triggers || []).filter(t => t.name !== original), structuredClone(draft)];
}
export function saveEvent(database: Database, draft: ScheduledEvent, engine: Engine, original = '') {
  if (!objectCapabilities[engine].events)
    throw new Error('Scheduled events require MySQL or MariaDB.');
  validName(draft.name, (database.events || []).map(e => e.name), original);
  body(draft.definition);
  // Text is retained; this verifies a bounded scheduler form, not the engine's full grammar.
  if (!/^EVERY\s+[1-9]\d*\s+(SECOND|MINUTE|HOUR|DAY|WEEK|MONTH|YEAR)$/i.test(draft.schedule))
    throw new Error('Use EVERY followed by a positive integer and SECOND, MINUTE, HOUR, DAY, WEEK, MONTH or YEAR.');
  database.events = [...(database.events || []).filter(e => e.name !== original), structuredClone(draft)];
}
export function dropDatabaseObject(database: Database, kind: 'routines' | 'triggers' | 'events', name: string, engine: Engine) {
  if ((kind === 'routines' && !objectCapabilities[engine].routines) || (kind === 'events' && !objectCapabilities[engine].events))
    throw new Error('Object type is not supported by this engine.');
  if (!database[kind]?.some(object => object.name === name))
    throw new Error('Object no longer exists.');
  if (kind === 'routines')
    database.routines = database.routines?.filter(o => o.name !== name);
  if (kind === 'triggers')
    database.triggers = database.triggers?.filter(o => o.name !== name);
  if (kind === 'events')
    database.events = database.events?.filter(o => o.name !== name);
}
export function createMockTable(database: Database, name: string, engine: Engine) {
  validName(name, relations(database).map(t => t.name));
  const table: Table = { name, kind: 'table', columns: [{ name: 'id', type: engine === 'SQLite' ? 'integer' : engine === 'PostgreSQL' ? 'integer' : 'int unsigned', nullable: false, primary: true, autoIncrement: true }], indexes: [{ name: engine === 'PostgreSQL' ? `${name}_pkey` : 'PRIMARY', kind: 'primary', method: 'btree', columns: [{ column: 'id', order: 'ASC' }] }], rows: [] };
  database.tables.push(table);
  return table;
}

/** Copy into a new mock relation without colliding with schema-scoped index names. */
export function copyMockTable(database: Database, source: Table, name: string, includeData: boolean, engine: Engine): Table {
  writableTable(source);
  validName(name, relations(database).map(table => table.name));
  const copy: Table = JSON.parse(JSON.stringify(source));
  copy.name = name;
  if (!includeData) copy.rows = [];
  const schemaScoped = engine === 'PostgreSQL' || engine === 'SQLite';
  const usedNames = new Set(relations(database)
    .filter(table => (table.schema || '') === (source.schema || ''))
    .flatMap(table => tableIndexes(table))
    .filter(index => engine === 'PostgreSQL' || index.kind !== 'primary')
    .map(index => index.name.toLowerCase()));
  copy.indexes = tableIndexes(copy).map(index => {
    if (!schemaScoped || (engine === 'SQLite' && index.kind === 'primary')) return index;
    const base = index.kind === 'primary' ? `${name}_pkey` : `${name}_${index.name}`;
    let candidate = base, suffix = 2;
    while (usedNames.has(candidate.toLowerCase())) candidate = `${base}_${suffix++}`;
    usedNames.add(candidate.toLowerCase());
    return { ...index, name: candidate };
  });
  for (const index of copy.indexes) {
    const errors = validateIndex(copy, index, engine, index.name);
    if (errors.length) throw new Error(errors.join(' '));
  }
  database.tables.push(copy);
  return copy;
}
