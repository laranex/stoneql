import type { Engine, Database } from '../types/index.ts';
import type { IndexKind, IndexMethod, ObjectSection } from '../types/objects.ts';
export interface ObjectCapabilities {
  routines: boolean;
  events: boolean;
  materializedViews: boolean;
  indexKinds: IndexKind[];
  methods: IndexMethod[];
  triggerTimings: string[];
}
export const objectCapabilities: Record<Engine, ObjectCapabilities> = {
  MySQL: { routines: true, events: true, materializedViews: false, indexKinds: ['primary', 'unique', 'normal', 'fulltext', 'spatial'], methods: ['btree'], triggerTimings: ['BEFORE', 'AFTER'] },
  MariaDB: { routines: true, events: true, materializedViews: false, indexKinds: ['primary', 'unique', 'normal', 'fulltext', 'spatial'], methods: ['btree'], triggerTimings: ['BEFORE', 'AFTER'] },
  PostgreSQL: { routines: true, events: false, materializedViews: true, indexKinds: ['primary', 'unique', 'normal'], methods: ['btree', 'hash', 'gist', 'spgist', 'gin', 'brin'], triggerTimings: ['BEFORE', 'AFTER', 'INSTEAD OF'] },
  SQLite: { routines: false, events: false, materializedViews: false, indexKinds: ['primary', 'unique', 'normal'], methods: ['btree'], triggerTimings: ['BEFORE', 'AFTER', 'INSTEAD OF'] },
};
export function objectSections(engine: Engine): ObjectSection[] {
  return [...(objectCapabilities[engine].routines ? ['Routines' as const] : []), 'Triggers', ...(objectCapabilities[engine].events ? ['Events' as const] : [])];
}
export function relations(database?: Database) { return database ? [...database.tables, ...(database.views || [])] : []; }
