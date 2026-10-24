export type IndexKind = 'primary' | 'unique' | 'normal' | 'fulltext' | 'spatial';
export type IndexMethod = 'btree' | 'hash' | 'gist' | 'spgist' | 'gin' | 'brin' | 'rtree' | 'fulltext';
export interface IndexPart {
  column: string;
  order: 'ASC' | 'DESC';
}
export interface TableIndex {
  name: string;
  kind: IndexKind;
  method: IndexMethod;
  columns: IndexPart[];
}
export interface RoutineParameter {
  name: string;
  type: string;
  mode: 'IN' | 'OUT' | 'INOUT';
}
export interface Routine {
  name: string;
  kind: 'procedure' | 'function';
  parameters: RoutineParameter[];
  returns: string;
  language: string;
  definition: string;
}
export interface Trigger {
  name: string;
  table: string;
  timing: 'BEFORE' | 'AFTER' | 'INSTEAD OF';
  event: 'INSERT' | 'UPDATE' | 'DELETE';
  definition: string;
}
export interface ScheduledEvent {
  name: string;
  schedule: string;
  enabled: boolean;
  definition: string;
}
export type ObjectSection = 'Routines' | 'Triggers' | 'Events';
