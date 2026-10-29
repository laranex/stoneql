import { openLive, closeLive, queryLive, liveStatus, runLiveJob, beginLiveTransaction, endLiveTransaction, describeTransactionEnd, listUnresolvedOutcomes, acknowledgeOutcome } from '../services/liveConnections';
import {NativeOperationError, hostState} from '../services/nativeHost';
import type {AccessModeDto, JournalProblemDto, LiveConnection, TransactionActionDto, TransactionEnd, UnresolvedOutcome} from '../../../../contracts/generated/v1';
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
export const LEASE_OUTCOME_UNREADABLE = 'The open transaction was ended by disconnecting, but its outcome could not be read. Treat it as unknown and verify on the server.';
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
  const liveSessions = ref<Record<string,{status:'connecting'|'connected'|'disconnected'|'error';info?:LiveConnection;error?:string;lease?:{id:string;state:'active'|'rollback_only'}}>>({});
  const liveGenerations = new Map<string,number>();
  /**
   * Unknown outcomes the native host keeps until the user acknowledges them: from before a
   * reload and, through its durable journal, from before a restart (`restored`).
   * `outcomeProfiles` names the profile of a lease or job started in this renderer session;
   * earlier sessions' outcomes have no profile name. `journalProblem`: why unknown outcomes
   * cannot be saved durably (new live writes are refused meanwhile).
   */
  const unresolvedOutcomes = ref<UnresolvedOutcome[]>([]);
  const journalProblem = ref<JournalProblemDto|null>(null);
  const unresolvedError = ref('');
  const outcomeProfiles = ref<Record<string,string>>({});
  /** Only an unconfirmed (or unreadable) end can add an unresolved outcome. */
  const unconfirmed=(end:TransactionEnd|null|undefined)=>end===null||end?.outcome==='unknown';
  let pendingTimer:ReturnType<typeof setTimeout>|undefined;
  /**
   * Jobs a reload detached while still running may record an unknown outcome after this list:
   * while the host reports any pending, list again each second. The host ends each one within
   * its job deadline, so this stops; it also stops on a listing error.
   */
  function applyUnresolved(list:{outcomes:UnresolvedOutcome[];pending?:number;journal?:JournalProblemDto|null}) {
    unresolvedOutcomes.value=list.outcomes;
    journalProblem.value=list.journal??null;
    clearTimeout(pendingTimer); pendingTimer=undefined;
    if((list.pending??0)>0)pendingTimer=setTimeout(()=>{ pendingTimer=undefined; void refreshUnresolvedOutcomes(); },1000);
  }
  async function refreshUnresolvedOutcomes() {
    if(hostState().kind!=='desktop')return;
    try {
      const list=await listUnresolvedOutcomes();
      if(!Array.isArray(list?.outcomes))throw new Error('Native protocol mismatch.');
      applyUnresolved(list); unresolvedError.value='';
    }
    catch(error){ unresolvedError.value=`Unknown outcomes could not be listed (${(error as Error).message}) Earlier outcomes may still be unresolved; verify on the server.`; }
  }
  /** Explicit user acknowledgement: hides the outcome; it never changes what was recorded. */
  async function acknowledgeUnresolvedOutcome(outcomeId:string) {
    applyUnresolved(await acknowledgeOutcome(outcomeId));
  }
  /**
   * Closes the live handle. An open transaction is ended natively by the close; its recorded
   * outcome is then read by lease id (an outcome query: nothing is sent again). Returns that
   * outcome, null when it could not be read, or undefined when no transaction was open.
   * When the close itself fails, the transaction is still ended by lease id and its outcome is
   * appended to the thrown error and the session error.
   */
  async function disconnectLiveProfile(id:string):Promise<TransactionEnd|null|undefined> {
    const lease=liveSessions.value[id]?.lease;
    liveGenerations.set(id,(liveGenerations.get(id)||0)+1);
    const handle=liveSessions.value[id]?.info?.handle;
    liveSessions.value[id]={status:'disconnected'};
    // Only an unconfirmed (or unreadable) end can add an unresolved outcome to list.
    const leaseEnd=async()=>{
      if(!lease)return undefined;
      const end=await endLiveTransaction(lease.id,'rollback').catch(()=>null);
      if(unconfirmed(end))await refreshUnresolvedOutcomes();
      return end;
    };
    if(handle) { try { await closeLive(handle); } catch(error) { if(!(error instanceof NativeOperationError && error.code==='FORBIDDEN')) {
      const end=await leaseEnd();
      if(end!==undefined)(error as Error).message+=` ${end?describeTransactionEnd(end).text:LEASE_OUTCOME_UNREADABLE}`;
      liveSessions.value[id]={status:'error',error:(error as Error).message};throw error;
    } } }
    return leaseEnd();
  }
  async function connectLiveProfile(profile:Connection, attemptId?:string, access?:AccessModeDto) {
    await disconnectLiveProfile(profile.id);
    const generation=liveGenerations.get(profile.id)!;
    liveSessions.value[profile.id]={status:'connecting'};
    try {
      const info=await openLive(profile, attemptId, access);
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
  /** Read-write job on the live handle, inside the open transaction lease when there is one. */
  async function runLiveJobFor(id:string,sql:string,onJob?:(jobId:string)=>void) {
    const current=liveSessions.value[id];
    if(current?.status!=='connected'||!current.info)throw new Error('Connect this live profile before running SQL.');
    const generation=liveGenerations.get(id);
    const lease=current.lease;
    // The lease's end as the native runtime recorded it (expired, closed, connection lost).
    // Only called once the runtime reported the lease gone, so it returns that record and
    // sends nothing.
    const leaseEnd=async(leaseId:string)=>{
      const end=await endLiveTransaction(leaseId,'rollback').catch(()=>null);
      if(unconfirmed(end))await refreshUnresolvedOutcomes();
      const session=liveSessions.value[id];
      if(session?.lease?.id===leaseId)session.lease=undefined;
      return end?describeTransactionEnd(end).text:'Its outcome could not be read; treat it as unknown and verify on the server.';
    };
    let accepted=false;
    let result;
    try { result=await runLiveJob(current.info.handle,sql,{leaseId:lease?.id,onJob:jobId=>{accepted=true;outcomeProfiles.value[jobId]=id;onJob?.(jobId);}}); }
    catch(error){
      // An accepted job may have ended unknown; a refusal for unresolved outcomes lists them.
      if(accepted||(error instanceof NativeOperationError&&(error.code==='UNRESOLVED_OUTCOMES'||error.code==='OUTCOME_JOURNAL_UNAVAILABLE')))await refreshUnresolvedOutcomes();
      // A job on a lease that already ended (for example idle-expired) is refused before
      // anything is sent; report why the transaction ended instead of a connection problem.
      if(lease&&!accepted&&error instanceof NativeOperationError&&error.code==='FORBIDDEN'&&liveGenerations.get(id)===generation)
        throw new NativeOperationError(error.code,`Nothing was sent: the transaction is no longer open. ${await leaseEnd(lease.id)}`);
      throw error;
    }
    if(result.outcome==='unknown')await refreshUnresolvedOutcomes();
    if(liveGenerations.get(id)!==generation)throw new Error('Results were discarded because the connection changed. Outcomes of sent statements are unknown to this view.');
    const session=liveSessions.value[id];
    if(lease&&session?.lease?.id===lease.id&&result.lease){
      if(result.lease==='ended')session.error=`The transaction ended during this job. ${await leaseEnd(lease.id)}`;
      else session.lease.state=result.lease;
    }
    return result;
  }
  async function beginLiveTransaction_(id:string) {
    const current=liveSessions.value[id];
    if(current?.status!=='connected'||!current.info)throw new Error('Connect this live profile first.');
    let lease;
    try { lease=await beginLiveTransaction(current.info.handle); }
    catch(error){ if(error instanceof NativeOperationError&&(error.code==='UNRESOLVED_OUTCOMES'||error.code==='OUTCOME_JOURNAL_UNAVAILABLE'))await refreshUnresolvedOutcomes(); throw error; }
    current.lease={id:lease.leaseId,state:'active'};
    outcomeProfiles.value[lease.leaseId]=id;
  }
  async function endLiveTransaction_(id:string,action:TransactionActionDto) {
    const current=liveSessions.value[id];
    if(!current?.lease)throw new Error('No transaction is open.');
    try {
      const end=await endLiveTransaction(current.lease.id,action);
      current.lease=undefined;
      if(unconfirmed(end))await refreshUnresolvedOutcomes();
      return end;
    } catch(error) {
      // The host removed the lease before sending COMMIT/ROLLBACK; only its outcome is unknown.
      if(error instanceof NativeOperationError && error.code==='DEADLINE_EXCEEDED')current.lease=undefined;
      await refreshUnresolvedOutcomes();
      throw error;
    }
  }
  const savedPageSize = Number(localStorage.getItem('stoneql-default-page-size'));
  const defaultPageSize = ref(
    [15, 25, 50, 100, 250, 500].includes(savedPageSize) ? savedPageSize : 25,
  );
  watch(defaultPageSize, (value) =>
    localStorage.setItem('stoneql-default-page-size', String(value)),
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
  const savedTheme = localStorage.getItem('stoneql-theme');
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
    unresolvedOutcomes, unresolvedError, journalProblem, outcomeProfiles, refreshUnresolvedOutcomes, acknowledgeUnresolvedOutcome,
    runLiveJob: runLiveJobFor, beginLiveTransaction: beginLiveTransaction_, endLiveTransaction: endLiveTransaction_,
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
