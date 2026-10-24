import { relations } from '../services/objectCapabilities.ts';
import type { Column, Database, Engine } from '../types/index.ts';
import { choiceDefinition } from '../services/valueValidation.ts';
import { identifierMatches } from './dialects.ts';
export interface SqlSchemaKey {
  connectionId: string;
  database: string;
}
export interface SqlColumnMetadata {
  name: string;
  type: string;
  nullable: boolean;
  enumValues?: readonly string[];
  comment?: string;
}
export interface SqlObjectMetadata {
  name: string;
  namespace?: string;
  kind: 'table' | 'view' | 'materialized-view';
  columns: readonly SqlColumnMetadata[];
}
export interface SqlSchemaMetadata extends SqlSchemaKey {
  engine: Engine;
  defaultNamespace?: string;
  objects: readonly SqlObjectMetadata[];
}
/** A future native provider supplies metadata, never row values or query results. */
export interface SqlSchemaProvider {
  readSchema(key: SqlSchemaKey, signal: AbortSignal): Promise<SqlSchemaMetadata>;
}
export interface IdentifierPart {
  value: string;
  quoted: boolean;
}
export function mockSqlMetadata(
  connectionId: string,
  engine: Engine,
  database?: Database,
): SqlSchemaMetadata {
  return {
    connectionId,
    engine,
    database: database?.name || '',
    objects: relations(database).map((table) => ({
      name: table.name,
      namespace: table.schema || (engine === 'PostgreSQL' ? undefined : database!.name),
      kind: table.kind || 'table',
      columns: table.columns.map((column) => {
        const choice = choiceDefinition(column, engine);
        return {
          name: column.name,
          type: column.type,
          nullable: column.nullable,
          comment: column.comment,
          enumValues: choice?.kind === 'enum' ? [...choice.values] : undefined,
        };
      }),
    })),
  };
}
export class SqlSchemaIndex {
  readonly sortedObjects: readonly { search: string; object: SqlObjectMetadata }[];
  private readonly names = new Map<string, SqlObjectMetadata[]>();
  readonly metadata: SqlSchemaMetadata;
  constructor(metadata: SqlSchemaMetadata) {
    const names = new Set<string>();
    const objects = metadata.objects.map((object) => {
      if (!object.name || !['table', 'view', 'materialized-view'].includes(object.kind))
        throw new Error('Invalid SQL object metadata.');
      const key = JSON.stringify([object.namespace || '', object.name]);
      if (names.has(key)) throw new Error(`Duplicate schema object: ${object.name}`);
      names.add(key);
      const columns = new Set<string>();
      return {
        ...object,
        columns: object.columns.map((column) => {
          if (!column.name || columns.has(column.name))
            throw new Error(`Invalid or duplicate column in ${object.name}`);
          columns.add(column.name);
          return { ...column, enumValues: column.enumValues ? [...column.enumValues] : undefined };
        }),
      };
    });
    this.metadata = { ...metadata, objects };
    for (const object of objects) {
      const key = object.name.toLowerCase();
      const list = this.names.get(key) || [];
      list.push(object);
      this.names.set(key, list);
    }
    this.sortedObjects = objects
      .map((object) => ({ search: object.name.toLowerCase(), object }))
      .sort((a, b) => a.search.localeCompare(b.search));
  }
  resolve(parts: readonly IdentifierPart[]): SqlObjectMetadata | undefined {
    if (!parts.length || parts.length > 2) return;
    const name = parts.at(-1)!,
      namespace = parts.length === 2 ? parts[0] : undefined;
    const candidates = (this.names.get(name.value.toLowerCase()) || []).filter(
      (object) =>
        identifierMatches(object.name, name.value, name.quoted, this.metadata.engine) &&
        (!namespace ||
          identifierMatches(
            object.namespace || '',
            namespace.value,
            namespace.quoted,
            this.metadata.engine,
          )),
    );
    if (candidates.length === 1) return candidates[0];
    return !namespace && this.metadata.defaultNamespace
      ? candidates.find((object) => object.namespace === this.metadata.defaultNamespace)
      : undefined;
  }
  objectsStartingWith(
    prefix: string,
    namespace?: IdentifierPart,
    limit = 500,
  ): SqlObjectMetadata[] {
    const needle = prefix.toLowerCase(),
      list = this.sortedObjects;
    let low = 0,
      high = list.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if (list[mid]!.search.localeCompare(needle) < 0) low = mid + 1;
      else high = mid;
    }
    const result: SqlObjectMetadata[] = [];
    for (let i = low; i < list.length && result.length < limit; i++) {
      const item = list[i]!;
      if (!item.search.startsWith(needle)) break;
      if (
        !namespace ||
        identifierMatches(
          item.object.namespace || '',
          namespace.value,
          namespace.quoted,
          this.metadata.engine,
        )
      )
        result.push(item.object);
    }
    return result;
  }
  isAmbiguousName(object: SqlObjectMetadata): boolean {
    return (this.names.get(object.name.toLowerCase())?.length || 0) > 1;
  }
}
export class SqlSchemaCache {
  private readonly entries = new Map<string, { fingerprint: string; index: SqlSchemaIndex }>();
  readonly capacity: number;
  constructor(capacity = 16) {
    if (!Number.isInteger(capacity) || capacity < 1)
      throw new Error('Schema cache capacity must be a positive integer.');
    this.capacity = capacity;
  }
  private key(key: SqlSchemaKey) {
    return JSON.stringify([key.connectionId, key.database]);
  }
  update(metadata: SqlSchemaMetadata): SqlSchemaIndex {
    const key = this.key(metadata),
      fingerprint = JSON.stringify([metadata.engine, metadata.defaultNamespace, metadata.objects]);
    const cached = this.entries.get(key);
    this.entries.delete(key);
    if (cached?.fingerprint === fingerprint) {
      this.entries.set(key, cached);
      return cached.index;
    }
    const index = new SqlSchemaIndex(metadata);
    this.entries.set(key, { fingerprint, index });
    while (this.entries.size > this.capacity)
      this.entries.delete(this.entries.keys().next().value!);
    return index;
  }
  get(key: SqlSchemaKey): SqlSchemaIndex | undefined {
    const id = this.key(key),
      cached = this.entries.get(id);
    if (cached) {
      this.entries.delete(id);
      this.entries.set(id, cached);
    }
    return cached?.index;
  }
  invalidate(key: SqlSchemaKey): void {
    this.entries.delete(this.key(key));
  }
  clear(): void {
    this.entries.clear();
  }
  get size(): number {
    return this.entries.size;
  }
}
export const sqlSchemaCache = new SqlSchemaCache();
