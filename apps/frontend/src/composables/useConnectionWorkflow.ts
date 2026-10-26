import { ref, type Ref } from 'vue';
import { useWorkspace } from '../stores/workspace';
import { openLive, closeLive, liveSupported } from '../services/liveConnections';
import { hostState } from '../services/nativeHost';
import { mockTransport } from '../services/mockTransport';
import { connectionColors } from '../services/connectionPreferences';
import type { Connection } from '../types';
export interface ConnectionWorkflowOptions {
  manager: Ref<boolean>;
  notify: (message: string, type?: string) => void;
  discard: () => Promise<boolean>;
  askDiscard: (message: string, acceptLabel?: string) => Promise<boolean>;
  confirm: (request: { title: string; acceptLabel: string; message: string; action: () => void | Promise<void> }) => void;
}
/** Connection dialog plus mock/live test, save, close and removal orchestration. */
export function useConnectionWorkflow({ manager, notify, discard, askDiscard, confirm }: ConnectionWorkflowOptions) {
  const store = useWorkspace();
  const nativeHostState = hostState();
  const connectionDraftInitial = ref(''),
    connectionDialog = ref(false),
    connectionDraft = ref<Connection>(),
    connectionError = ref(''),
    testing = ref(false),
    testMessage = ref(''),
    testOk = ref(false),
    connectionEditing = ref(false);
  async function disconnectIfLive(id: string) {
    if (store.connections.find((c) => c.id === id)?.mode !== 'live') return true;
    try { await store.disconnectLiveProfile(id); return true; }
    catch (error) { notify((error as Error).message, 'error'); return false; }
  }
  async function closeConnection(id: string) {
    const ws = store.workspaces[id];
    if (
      ws?.queries.some((q) => q.sql !== q.saved) &&
      !(await askDiscard(
        'Close this workspace with unsaved query text? The text will remain in this session.',
        'Close workspace',
      ))
    )
      return;
    if (store.activeId === id && !(await discard())) return;
    if (!(await disconnectIfLive(id))) return;
    store.opened = store.opened.filter((x) => x !== id);
    if (store.activeId === id) {
      if (store.opened[0]) store.connect(store.opened[0]);
      else manager.value = true;
    }
  }
  function newConnection(existing?: Connection) {
    connectionEditing.value = !!existing;
    connectionDraft.value = existing
      ? JSON.parse(JSON.stringify(existing))
      : {
          id: crypto.randomUUID(),
          mode: 'mock',
          name: '',
          engine: 'MySQL',
          host: '127.0.0.1',
          port: 3306,
          user: 'root',
          password: '',
          database: 'acme_store',
          color: connectionColors[0]!.value,
          environment: 'Development',
          customEnvironment: '',
          groupId: '',
          transport: { type: 'direct' },
        };
    connectionDraft.value!.mode ||= 'mock';
    testMessage.value = '';
    connectionError.value = '';
    connectionDraftInitial.value = JSON.stringify(connectionDraft.value);
    connectionDialog.value = true;
  }
  async function closeConnectionDialog() {
    if(testing.value)return;
    if (
      JSON.stringify(connectionDraft.value) !== connectionDraftInitial.value &&
      !(await askDiscard('Discard connection configuration changes?'))
    )
      return;
    connectionDialog.value = false;
    if(connectionDraft.value?.mode==='live'){connectionDraft.value.password=undefined;connectionDraft.value=undefined;}
  }
  function changeTransport(type: string) {
    if (!connectionDraft.value || testing.value) return;
    connectionDraft.value.transport =
      type === 'ssh'
        ? { type: 'ssh', host: '', port: 22, user: '', auth: 'agent' }
        : { type: 'direct' };
  }
  async function readCertificate(event:Event) {
    const input=event.target as HTMLInputElement;
    const file=input.files?.[0];
    if(!file||!connectionDraft.value)return;
    connectionError.value='';
    if(file.size>16384){connectionError.value='CA certificate must be at most 16 KiB.';input.value='';return;}
    const draft=connectionDraft.value;
    try {
      const pem=await file.text();
      if(!pem.includes('-----BEGIN CERTIFICATE-----')||pem.includes('PRIVATE KEY')){connectionError.value='Choose a PEM CA certificate, not a private key.';input.value='';return;}
      if(connectionDraft.value===draft)draft.rootCertificatePem=pem;
    }catch{connectionError.value='The selected CA certificate could not be read.';}
  }
  async function testConnection() {
    if (!connectionDraft.value) return;
    testing.value = true;
    try {
      if(connectionDraft.value.mode==='live') {
        const info=await openLive(connectionDraft.value);
        await closeLive(info.handle);
        testMessage.value=`Verified TLS connection successful · ${info.engineVersion} · Test session closed.`;
        testOk.value=true;
      } else {
        const result=await mockTransport.test(connectionDraft.value);
        testMessage.value=result.message;testOk.value=result.ok;
      }
    }catch(error){testMessage.value=(error as Error).message;testOk.value=false;}
    finally{testing.value=false;}
  }
  async function saveLiveConnection(draft: Connection, connectNow: boolean) {
    if(nativeHostState.kind!=='desktop'||!liveSupported(draft)||!draft.database){connectionError.value='Live connections require the desktop app, Direct MySQL/MariaDB and a database name.';return;}
    const old=store.connections.find(c=>c.id===draft.id);
    if(old&&old.mode!=='live'&&store.workspaces[draft.id]?.queries.some(q=>q.sql!==q.saved)&&!(await askDiscard('Changing connection mode clears this workspace. Discard its unsaved queries?')))return;
    if((connectNow||store.activeId===draft.id)&&!(await discard()))return;
    testing.value=true;
    try {
      if(old?.mode==='live')await store.disconnectLiveProfile(draft.id);
      const saved={...draft,password:undefined};
      const index=store.connections.findIndex(c=>c.id===draft.id);
      if(index<0)store.connections.push(saved);else store.connections[index]=saved;
      if(old?.mode!=='live')delete store.workspaces[draft.id];
      if(connectNow || store.activeId===draft.id)store.connect(draft.id);
      const workspace=store.workspaces[draft.id];
      if(workspace){workspace.database=draft.database;workspace.databases=[{name:draft.database,tables:[]}];workspace.table='';workspace.scope='database';workspace.view='SQL';for(const tab of workspace.queries){tab.result=null;tab.error='';}}
      if(connectNow)await store.connectLiveProfile(draft);
      draft.password='';connectionDraftInitial.value=JSON.stringify(draft);
      connectionDialog.value=false;if(connectNow)manager.value=false;
      notify(connectNow?'Live read-only connection opened over verified TLS.':'Live profile saved; connect explicitly before executing SQL.');
    }catch(error){connectionError.value=(error as Error).message;}
    finally{testing.value=false;}
  }
  async function saveConnection(connectNow = true) {
    const draft = connectionDraft.value;
    if (!draft) return;
    connectionError.value = '';
    if (!draft.name.trim() || (draft.engine !== 'SQLite' && (!draft.host.trim() || !draft.user.trim()))) { connectionError.value = 'Name, host, and username are required.'; return; }
    if (draft.environment === 'Custom' && !draft.customEnvironment?.trim()) { connectionError.value = 'Enter a custom environment label.'; return; }
    if (draft.transport.type === 'ssh' && (!draft.transport.host.trim() || !draft.transport.user.trim())) { connectionError.value = 'SSH host and username are required.'; return; }
    if(draft.mode==='live') return saveLiveConnection(draft, connectNow);
    const previous=store.connections.find(c=>c.id===draft.id);
    if(previous?.mode==='live') {
      if(store.workspaces[draft.id]?.queries.some(q=>q.sql!==q.saved)&&!(await askDiscard('Changing connection mode clears this workspace. Discard its unsaved queries?')))return;
      if(store.activeId===draft.id && !(await discard()))return;
      try{await store.disconnectLiveProfile(draft.id);}catch(error){connectionError.value=(error as Error).message;return;}
      delete store.workspaces[draft.id];
    }
    if (connectNow) {
      testing.value = true;
      const result = await mockTransport.test(draft);
      testing.value = false;
      if (!result.ok) { connectionError.value = result.message; return; }
      if (!(await discard())) return;
    }
    const index = store.connections.findIndex(x => x.id === draft.id);
    if (index >= 0) store.connections[index] = draft;
    else store.connections.push(draft);
    if (connectNow || (previous?.mode==='live' && store.activeId===draft.id)) store.connect(draft.id);
    connectionDialog.value = false;
    if (connectNow) manager.value = false;
    notify(connectNow ? 'Configuration saved; mock connection opened.' : 'Configuration saved without connecting. Secrets remain in memory only.');
  }
  function removeConnection(id: string) {
    confirm({
      title: 'Remove connection?',
      acceptLabel: 'Remove connection',
      message: 'The connection and its session data will be removed.',
      action: async () => {
        if (!(await disconnectIfLive(id))) return;
        store.connections = store.connections.filter((c) => c.id !== id);
        store.opened = store.opened.filter((c) => c !== id);
        delete store.workspaces[id];
        if (store.activeId === id && store.connections[0]) store.connect(store.connections[0].id);
        if (!store.connections.length) manager.value = true;
        notify('Connection removed.');
      },
    });
  }
  return {
    connectionDialog, connectionDraft, connectionDraftInitial, connectionError, connectionEditing,
    testing, testMessage, testOk,
    newConnection, closeConnectionDialog, changeTransport, readCertificate,
    testConnection, saveConnection, removeConnection, closeConnection,
  };
}
