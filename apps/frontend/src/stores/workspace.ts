import { openLive, closeLive, queryLive, liveStatus } from '../services/liveConnections';
import {NativeOperationError} from '../services/nativeHost';
import type {LiveConnection} from '../../../../contracts/generated/v1';
import { relations } from '../services/objectCapabilities';
import { saveIndex as applyIndex, dropIndex as removeIndex, assertIndexDependencies } from '../services/indexes';
import * as objectActions from '../services/databaseObjects';
import type { TableIndex, Routine, Trigger, ScheduledEvent } from '../types/objects';
import type { Table } from '../types';
import { computed, ref, watch } from 'vue';
import {
  insertRecords,
  updateRecord as mutateRecord,
  deleteRecords,
  copyRecords,
  mockDefault,
  renameMockTable,
  removeMockTable,
  removeMockColumn,
} from '../services/recordMutations';
import { defineStore } from 'pinia';
import { connections as samples, createDatabases } from '../data/sample';
import type { Connection, Workspace, Row, Column } from '../types';
import { applyColumnDefinition, expressionValue } from '../services/columnDefinition';
import type { ColumnDraft } from '../services/columnDefinition';
import {
  loadConnectionPreferences,
  connectionSnapshot,
  CONNECTION_STORAGE_KEY,
} from '../services/connectionPreferences';
export const useWorkspace = defineStore('workspace', () => {
  const loaded = loadConnectionPreferences(localStorage, samples);
  const connections = ref<Connection[]>(loaded.connections);
  const groups = ref(loaded.groups);
  const storageError = ref(loaded.error || '');
  function persistConnections() {
    try {
      localStorage.setItem(
        CONNECTION_STORAGE_KEY,
        JSON.stringify(connectionSnapshot(connections.value, groups.value)),
      );
      storageError.value = '';
    } catch (error) {
      storageError.value = `Connection preferences were not saved: ${(error as Error).message}`;
    }
  }
  if (!loaded.error) persistConnections();
  watch([connections, groups], persistConnections, { deep: true });
  function createGroup(name: string) {
    const value = name.trim();
    if (!value || groups.value.some((g) => g.name.toLowerCase() === value.toLowerCase()))
      throw new Error('Enter a unique group name.');
    const group = { id: crypto.randomUUID(), name: value, collapsed: false };
    groups.value.push(group);
    return group;
  }
  function renameGroup(id: string, name: string) {
    const value = name.trim(),
      group = groups.value.find((g) => g.id === id);
    if (
      !group ||
      !value ||
      groups.value.some((g) => g.id !== id && g.name.toLowerCase() === value.toLowerCase())
    )
      throw new Error('Enter a unique group name.');
    group.name = value;
  }
  function deleteGroup(id: string) {
    connections.value.forEach((c) => {
      if (c.groupId === id) c.groupId = '';
    });
    groups.value = groups.value.filter((g) => g.id !== id);
  }
  function moveConnection(id: string, groupId: string) {
    if (groupId && !groups.value.some((g) => g.id === groupId))
      throw new Error('Group no longer exists.');
    const c = connections.value.find((c) => c.id === id);
    if (c) c.groupId = groupId;
  }
  function reorderConnection(id: string, direction: number) {
    const c = connections.value.find((c) => c.id === id);
    if (!c) return;
    const siblings = connections.value.filter((x) => x.groupId === c.groupId),
      i = siblings.findIndex((x) => x.id === id),
      other = siblings[i + direction];
    if (!other) return;
    const a = connections.value.indexOf(c),
      b = connections.value.indexOf(other);
    [connections.value[a], connections.value[b]] = [connections.value[b]!, connections.value[a]!];
  }
  function reorderGroup(id: string, direction: number) {
    const i = groups.value.findIndex((g) => g.id === id),
      other = groups.value[i + direction];
    if (i < 0 || !other) return;
    [groups.value[i], groups.value[i + direction]] = [other, groups.value[i]!];
  }
  const liveSessions = ref<Record<string,{status:'connecting'|'connected'|'disconnected'|'error';info?:LiveConnection;error?:string}>>({});
  const liveGenerations = new Map<string,number>();
  async function disconnectLiveProfile(id:string) {
    liveGenerations.set(id,(liveGenerations.get(id)||0)+1);
    const handle=liveSessions.value[id]?.info?.handle;
    liveSessions.value[id]={status:'disconnected'};
    if(handle) { try { await closeLive(handle); } catch(error) { if(!(error instanceof NativeOperationError && error.code==='FORBIDDEN')) {liveSessions.value[id]={status:'error',error:(error as Error).message};throw error;} } }
  }
  async function connectLiveProfile(profile:Connection) {
    await disconnectLiveProfile(profile.id);
    const generation=liveGenerations.get(profile.id)!;
    liveSessions.value[profile.id]={status:'connecting'};
    try {
      const info=await openLive(profile);
      if(liveGenerations.get(profile.id)!==generation){await closeLive(info.handle);throw new Error('Connection configuration changed during startup.');}
      liveSessions.value[profile.id]={status:'connected',info};
    }catch(error){
      if(liveGenerations.get(profile.id)===generation)liveSessions.value[profile.id]={status:'error',error:(error as Error).message};
      throw error;
    }
  }
  async function refreshLiveStatus(id:string) {
    const current=liveSessions.value[id];
    if(current?.status!=='connected'||!current.info)return;
    const generation=liveGenerations.get(id);
    try {
      const status=await liveStatus(current.info.handle);
      if(liveGenerations.get(id)===generation && !status.connected){
        liveGenerations.set(id,(generation||0)+1);
        liveSessions.value[id]={status:'disconnected',error:'Live session expired or closed. Reconnect before running SQL.'};
      }
    }catch(error){if(liveGenerations.get(id)===generation)liveSessions.value[id]={...current,status:'error',error:(error as Error).message};}
  }
  async function executeLive(id:string,sql:string) {
    const current=liveSessions.value[id];
    if(current?.status!=='connected'||!current.info)throw new Error('Connect this live profile before running SQL.');
    const generation=liveGenerations.get(id);
    try {
      const result=await queryLive(current.info.handle,sql);
      if(liveGenerations.get(id)!==generation)throw new Error('Query results were discarded because the connection changed.');
      return result;
    }catch(error){
      if(error instanceof NativeOperationError && ['FORBIDDEN','CONNECTION_UNAVAILABLE'].includes(error.code) && liveGenerations.get(id)===generation){
        try{await disconnectLiveProfile(id);}catch{};
        liveSessions.value[id]={status:'error',error:error.message};
      }
      throw error;
    }
  }
  const savedPageSize = Number(localStorage.getItem('dbadmin-default-page-size'));
  const defaultPageSize = ref(
    [15, 25, 50, 100, 250, 500].includes(savedPageSize) ? savedPageSize : 25,
  );
  watch(defaultPageSize, (value) =>
    localStorage.setItem('dbadmin-default-page-size', String(value)),
  );
  const activeId = ref(connections.value[0]?.id || '');
  const opened = ref(connections.value.slice(0, 2).map((c) => c.id));
  const workspaces = ref<Record<string, Workspace>>({});
  function initialize(id: string) {
    const profile=connections.value.find(c=>c.id===id);
    const live=profile?.mode==='live';
    const initialSql=live?'SELECT 1;':'SELECT * FROM users\nORDER BY id ASC\nLIMIT 25;';
    if (!workspaces.value[id])
      workspaces.value[id] = {
        databases: live ? [{name:profile!.database,tables:[]}] : createDatabases(profile?.engine),
        scope: live?'database':'table',
        database: live?profile!.database:'acme_store',
        table: live?'':'users',
        view: live?'SQL':'Browse',
        page: 1,
        pageSize: defaultPageSize.value,
        treeFilter: '',
        expandedDatabases: ['acme_store'],
        showHistory: false,
        filter: '',
        sort: 'id',
        descending: false,
        queries: [
          {
            id: 1,
            title: 'Query 1',
            sql: initialSql,
            saved: initialSql,
          },
        ],
        queryId: 1,
        history: [],
      };
  }
  opened.value.forEach(initialize);
  const connection = computed(() => connections.value.find((c) => c.id === activeId.value)!);
  const workspace = computed(() => workspaces.value[activeId.value]!);
  const database = computed(() =>
    workspace.value.databases.find((d) => d.name === workspace.value.database),
  );
  const table = computed(() =>
    relations(database.value).find((t) => t.name === workspace.value.table),
  );
  const savedTheme = localStorage.getItem('dbadmin-theme');
  const theme = ref(savedTheme === 'light' || savedTheme === 'dark' ? savedTheme : 'system');
  const pageSize = computed({
    get: () => workspace.value.pageSize,
    set: (value: number) => {
      workspace.value.pageSize = value;
    },
  });
  const shortcuts = ref(true);
  const sqlPreviewExpanded = ref(true);
  function connect(id: string) {
    initialize(id);
    if (!opened.value.includes(id)) opened.value.push(id);
    activeId.value = id;
  }
  function selectTable(db: string, name: string) {
    Object.assign(workspace.value, {
      scope: 'table',
      database: db,
      table: name,
      view: 'Browse',
      page: 1,
      filter: '',
      sort:
        workspace.value.databases
          .find((d) => d.name === db)
          ?.tables.find((t) => t.name === name)
          ?.columns.find((c) => c.primary)?.name || '',
      descending: false,
    });
  }
  function updateColumn(draft: ColumnDraft, original = '') {
    if (!table.value) throw new Error('Select a table first.');
    const name = table.value.name;
    const staged = JSON.parse(JSON.stringify(table.value)) as Table;
    const column = applyColumnDefinition(staged, draft, connection.value.engine, original);
    const dependencyDatabase = database.value ? {...database.value,tables:database.value.tables.map(t=>({...t,columns:t.columns.map(c=>({...c,foreign:c.foreign===`${name}.${original}`?`${name}.${column.name}`:c.foreign}))}))} : undefined;
    assertIndexDependencies(staged, staged.indexes || [], dependencyDatabase, connection.value.engine);
    const rows = table.value.rows;
    rows.forEach((row,index)=>{for(const key of Object.keys(row)) delete row[key];Object.assign(row,staged.rows[index]);});
    Object.assign(table.value, staged, {rows});
    if (original && original !== column.name)
      for (const child of database.value?.tables ?? [])
        for (const definition of child.columns)
          if (definition.foreign === `${name}.${original}`)
            definition.foreign = `${name}.${column.name}`;
    return column;
  }
  function mutationContext() {
    if (!database.value || !table.value) throw new Error('Select a table first.');
    return {
    db: database.value, table: table.value, engine: connection.value.engine };
  }
  function defaultValue(c: Column): Row[string] {
    return mockDefault(mutationContext().table, c);
  }
  function insert(values: Row) {
    const { db, table, engine } = mutationContext();
    return insertRecords(db, table, [values], engine)[0];
  }
  function insertMany(values: Row[]) {
    const { db, table, engine } = mutationContext();
    return insertRecords(db, table, values, engine);
  }
  function updateRow(row: Row, patch: Row) {
    const { db, table, engine } = mutationContext();
    mutateRecord(db, table, row, patch, engine);
  }
  function deleteRows(rows: Row[]) {
    const { db, table } = mutationContext();
    deleteRecords(db, table, rows);
  }
  function copyRows(rows: Row[]) {
    const { db, table, engine } = mutationContext();
    return copyRecords(db, table, rows, engine);
  }
  function copyTable(name: string, includeData: boolean) {
    const context = mutationContext();
    return objectActions.copyMockTable(context.db, context.table, name, includeData, context.engine);
  }
  function renameTable(name: string) {
    const { db, table } = mutationContext();
    renameMockTable(db, table, name);
    workspace.value.table = name;
  }
  function removeTable(drop: boolean) {
    const { db, table, engine } = mutationContext();
    removeMockTable(db, table, engine, drop);
    if (drop) {
      workspace.value.table = db.tables[0]?.name || '';
      workspace.value.view = workspace.value.table ? 'Browse' : 'Structure';
      if (!workspace.value.table) workspace.value.scope = 'database';
    }
  }
  function removeColumn(column: Column) {
    const { db, table } = mutationContext();
    removeMockColumn(db, table, column);
  }
  function saveTableIndex(draft:TableIndex, original='') {
    const context=mutationContext();applyIndex(context.table,JSON.parse(JSON.stringify(draft)),context.engine,original,context.db);
  }
  function dropTableIndex(name:string) { const context=mutationContext();removeIndex(context.table,name,context.db,context.engine); }
  function objectDatabase() { if(!database.value) throw new Error('Select a database.');return database.value; }
  function createTable(name:string) {const table=objectActions.createMockTable(objectDatabase(),name,connection.value.engine);selectTable(workspace.value.database,table.name);workspace.value.view='Structure';return table;}
  function saveView(draft:Pick<Table,'name'|'kind'|'definition'>,original='') { const view=objectActions.saveView(objectDatabase(),draft,connection.value.engine,original);if(original&&workspace.value.table===original) workspace.value.table=view.name;return view; }
  function previewView(name:string) { const db=objectDatabase(),view=db.views?.find(v=>v.name===name);if(!view) throw new Error('View no longer exists.');return objectActions.previewView(db,view,connection.value.engine); }
  function dropView(name:string) { objectActions.dropView(objectDatabase(),name);if(workspace.value.table===name) {workspace.value.table=database.value?.tables[0]?.name||'';workspace.value.scope='database';workspace.value.view='Structure';} }
  function saveRoutine(draft:Routine,original='') {objectActions.saveRoutine(objectDatabase(),JSON.parse(JSON.stringify(draft)),connection.value.engine,original);}
  function saveTrigger(draft:Trigger,original='') {objectActions.saveTrigger(objectDatabase(),JSON.parse(JSON.stringify(draft)),connection.value.engine,original);}
  function saveEvent(draft:ScheduledEvent,original='') {objectActions.saveEvent(objectDatabase(),JSON.parse(JSON.stringify(draft)),connection.value.engine,original);}
  function dropObject(kind:'routines'|'triggers'|'events',name:string) {objectActions.dropDatabaseObject(objectDatabase(),kind,name,connection.value.engine);}
  return {
    liveSessions, connectLiveProfile, disconnectLiveProfile, executeLive, refreshLiveStatus,
    saveTableIndex, dropTableIndex, createTable, saveView, previewView, dropView, saveRoutine, saveTrigger, saveEvent, dropObject,
    connections,
    groups,
    storageError,
    createGroup,
    renameGroup,
    deleteGroup,
    moveConnection,
    reorderConnection,
    reorderGroup,
    activeId,
    opened,
    workspaces,
    connection,
    workspace,
    database,
    table,
    theme,
    pageSize,
    defaultPageSize,
    shortcuts,
    sqlPreviewExpanded,
    connect,
    selectTable,
    insert,
    defaultValue,
    insertMany,
    updateRow,
    deleteRows,
    copyRows,
    renameTable,
    copyTable,
    removeTable,
    removeColumn,
    updateColumn,
  };
});
