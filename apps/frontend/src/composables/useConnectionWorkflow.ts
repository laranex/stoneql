import { ref, type Ref } from 'vue';
import { useWorkspace, LEASE_OUTCOME_UNREADABLE } from '../stores/workspace';
import { openLive, closeLive, liveSupported, pickSqliteFile, pickSshKeyFile, scanHostKey, cancelConnect, storeSecret, deleteSecret, describeTransactionEnd } from '../services/liveConnections';
import { hostState } from '../services/nativeHost';
import { mockTransport } from '../services/mockTransport';
import { connectionColors } from '../services/connectionPreferences';
import type { Connection, Transport } from '../types';
import type { SecretPurpose, SshHostKey, TransactionEnd } from '../../../../contracts/generated/v1';
type SshTransport = Extract<Transport, { type: 'ssh' }>;
/** Opaque OS secret references held by a profile. */
function secretReferences(connection?: Connection): string[] {
  const transport = connection?.transport;
  return [connection?.passwordRef, transport?.type === 'ssh' ? transport.passwordRef : undefined, transport?.type === 'ssh' ? transport.passphraseRef : undefined].filter((value): value is string => !!value);
}
/** Session-only secrets leave the draft when the dialog closes or a profile is saved. */
function clearTransientSecrets(connection: Connection) {
  connection.password = undefined;
  if (connection.transport.type === 'ssh') {
    connection.transport.password = undefined;
    connection.transport.passphrase = undefined;
    connection.transport.privateKey = undefined;
  }
  // The read-write choice is per session too: the next connect starts read-only again.
  connection.access = undefined;
}
export const LEASE_DISCONNECT_WARNING = 'This connection has an open transaction. Disconnecting rolls it back; the outcome is shown afterwards.';
/** Shows the outcome of a transaction ended by a disconnect (null: it could not be read). */
export function reportLeaseEnd(end: TransactionEnd | null | undefined, notify: (message: string, type?: string) => void) {
  if (end === undefined) return;
  if (end === null) { notify(LEASE_OUTCOME_UNREADABLE, 'error'); return; }
  const notice = describeTransactionEnd(end);
  notify(notice.text, notice.kind === 'success' ? 'success' : 'error');
}
export interface ConnectionWorkflowOptions {
  manager: Ref<boolean>;
  notify: (message: string, type?: string) => void;
  discard: () => Promise<boolean>;
  askDiscard: (message: string, acceptLabel?: string) => Promise<boolean>;
  confirm: (request: { title: string; acceptLabel: string; message: string; action: () => void | Promise<void> }) => void;
  /** PROD-tagged profiles confirm before opening read-write (a UX safeguard, not authorization). */
  confirmReadWrite: (profileName: string) => Promise<boolean>;
}
/** Connection dialog plus mock/live test, save, close and removal orchestration. */
export function useConnectionWorkflow({ manager, notify, discard, askDiscard, confirm, confirmReadWrite }: ConnectionWorkflowOptions) {
  const store = useWorkspace();
  const nativeHostState = hostState();
  const connectionDraftInitial = ref(''),
    connectionDialog = ref(false),
    connectionDraft = ref<Connection>(),
    connectionError = ref(''),
    testing = ref(false),
    testMessage = ref(''),
    testOk = ref(false),
    connectionEditing = ref(false),
    pendingHostKey = ref<SshHostKey>(),
    attemptId = ref('');
  const sshOf = (draft = connectionDraft.value): SshTransport | undefined => draft?.transport.type === 'ssh' ? draft.transport : undefined;
  const liveSsh = (draft = connectionDraft.value) => draft?.mode === 'live' && draft.engine !== 'SQLite' ? sshOf(draft) : undefined;
  /** An open transaction is rolled back by disconnecting: ask first (unless already asked). */
  async function confirmLeaseDiscard(id: string) {
    return !store.liveSessions[id]?.lease || askDiscard(LEASE_DISCONNECT_WARNING, 'Disconnect and roll back');
  }
  /** Disconnects and reports the open transaction's recorded outcome, if there was one. */
  async function disconnectReporting(id: string) {
    reportLeaseEnd(await store.disconnectLiveProfile(id), notify);
  }
  async function disconnectIfLive(id: string, askLease = true) {
    if (store.connections.find((c) => c.id === id)?.mode !== 'live') return true;
    if (askLease && !(await confirmLeaseDiscard(id))) return false;
    try { await disconnectReporting(id); return true; }
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
  function newConnection(existing?: Connection, access?: Connection['access']) {
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
    // Read-write over SSH is gated off until tunnelled cancellation is fixture-tested.
    connectionDraft.value!.access = connectionDraft.value!.mode === 'live' && connectionDraft.value!.transport.type !== 'ssh' ? access : undefined;
    // Remember toggles mirror existing references; they are draft-only and never persisted.
    if (connectionDraft.value!.mode === 'live') {
      connectionDraft.value!.rememberPassword = !!connectionDraft.value!.passwordRef;
      const ssh = sshOf(connectionDraft.value);
      if (ssh) { ssh.rememberPassword = !!ssh.passwordRef; ssh.rememberPassphrase = !!ssh.passphraseRef; }
    }
    pendingHostKey.value = undefined;
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
    pendingHostKey.value = undefined;
    if(connectionDraft.value?.mode==='live'){clearTransientSecrets(connectionDraft.value);connectionDraft.value=undefined;}
  }
  /** Shows the server's host key for out-of-band verification. Nothing is trusted here. */
  async function fetchHostKey() {
    const draft = connectionDraft.value, ssh = sshOf(draft);
    if (!draft || !ssh || testing.value) return;
    connectionError.value = '';
    if (!ssh.host.trim()) { connectionError.value = 'Enter the SSH host first.'; return; }
    testing.value = true;
    try {
      const key = await scanHostKey(ssh.host.trim(), ssh.port);
      if (connectionDraft.value === draft) pendingHostKey.value = key;
    } catch (error) { connectionError.value = (error as Error).message; }
    finally { testing.value = false; }
  }
  /** Explicit enrollment after the user confirmed the displayed fingerprint. */
  function trustHostKey() {
    const ssh = sshOf(), key = pendingHostKey.value;
    if (!ssh || !key) return;
    ssh.hostKey = key.publicKey;
    ssh.hostKeyFingerprint = key.fingerprint;
    pendingHostKey.value = undefined;
    testMessage.value = '';
  }
  function rejectHostKey() { pendingHostKey.value = undefined; }
  /** Rotation is deliberate: a changed key is refused until the old one is forgotten here. */
  function forgetHostKey() {
    const ssh = sshOf();
    if (!ssh || testing.value) return;
    ssh.hostKey = undefined;
    ssh.hostKeyFingerprint = undefined;
    pendingHostKey.value = undefined;
  }
  async function chooseSshKeyFile() {
    const draft = connectionDraft.value, ssh = sshOf(draft);
    if (!draft || !ssh || testing.value) return;
    connectionError.value = '';
    testing.value = true;
    try {
      const selected = await pickSshKeyFile();
      if (selected.grant && connectionDraft.value === draft) { ssh.keyGrant = selected.grant; ssh.keyFileName = selected.fileName || ''; }
    } catch (error) { connectionError.value = (error as Error).message; }
    finally { testing.value = false; }
  }
  async function cancelAttempt() {
    if (!attemptId.value) return;
    try { await cancelConnect(attemptId.value); } catch (error) { connectionError.value = (error as Error).message; }
  }
  /** Stores newly typed secrets the user chose to remember; returns references to forget after a successful save. */
  async function persistSecrets(draft: Connection, old?: Connection): Promise<string[]> {
    const created: string[] = [], obsolete: string[] = [];
    const keep = async (remember: boolean | undefined, value: string | undefined, current: string | undefined, purpose: SecretPurpose) => {
      if (!remember) { if (current) obsolete.push(current); return undefined; }
      if (!value) return current;
      const reference = await storeSecret(purpose, draft, value);
      created.push(reference);
      if (current) obsolete.push(current);
      return reference;
    };
    try {
      draft.passwordRef = await keep(draft.rememberPassword, draft.password, draft.passwordRef, 'database_password');
      const ssh = sshOf(draft);
      if (ssh) {
        ssh.passwordRef = await keep(ssh.auth === 'password' && ssh.rememberPassword, ssh.password, ssh.passwordRef, 'ssh_password');
        ssh.passphraseRef = await keep(ssh.auth === 'private-key' && ssh.rememberPassphrase, ssh.passphrase, ssh.passphraseRef, 'ssh_passphrase');
      }
    } catch (error) {
      for (const reference of created) await deleteSecret(reference).catch(() => undefined);
      throw error;
    }
    const kept = secretReferences(draft);
    for (const reference of secretReferences(old)) if (!kept.includes(reference) && !obsolete.includes(reference)) obsolete.push(reference);
    return obsolete.filter((reference) => !kept.includes(reference));
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
  async function chooseSqliteFile() {
    const draft=connectionDraft.value;
    if(!draft||testing.value)return;
    connectionError.value='';
    testing.value=true;
    try {
      const selected=await pickSqliteFile();
      if(selected.grant&&connectionDraft.value===draft){draft.sqliteGrant=selected.grant;draft.database=selected.fileName||'';testMessage.value='';}
    }catch(error){connectionError.value=(error as Error).message;}
    finally{testing.value=false;}
  }
  async function testConnection() {
    if (!connectionDraft.value) return;
    const ssh = liveSsh();
    if (ssh && !ssh.hostKey) {
      await fetchHostKey();
      testMessage.value = 'Verify and trust the SSH host key, then test again.'; testOk.value = false;
      return;
    }
    testing.value = true;
    try {
      if(connectionDraft.value.mode==='live') {
        attemptId.value=`attempt_${crypto.randomUUID()}`;
        const info=await openLive(connectionDraft.value, attemptId.value);
        await closeLive(info.handle);
        testMessage.value=`${connectionDraft.value.engine==='SQLite'?'Read-only file opened':ssh?'Verified SSH host key and TLS connection successful':'Verified TLS connection successful'} · ${info.engineVersion} · Test session closed.`;
        testOk.value=true;
      } else {
        const result=await mockTransport.test(connectionDraft.value);
        testMessage.value=result.message;testOk.value=result.ok;
      }
    }catch(error){testMessage.value=(error as Error).message;testOk.value=false;}
    finally{testing.value=false;attemptId.value='';}
  }
  async function saveLiveConnection(draft: Connection, connectNow: boolean) {
    if(nativeHostState.kind!=='desktop'||!liveSupported(draft)||!draft.database||(draft.engine==='SQLite'&&!draft.sqliteGrant)){connectionError.value=draft.engine==='SQLite'?'Live SQLite requires the desktop app, Direct transport and a file chosen with Choose file.':'Live connections require the desktop app, MySQL, MariaDB or PostgreSQL over Direct or SSH and a database name.';return;}
    const ssh=liveSsh(draft);
    if(ssh&&!ssh.hostKey){connectionError.value='Fetch, verify and trust the SSH host key before saving a live SSH profile.';return;}
    const old=store.connections.find(c=>c.id===draft.id);
    if(old&&old.mode!=='live'&&store.workspaces[draft.id]?.queries.some(q=>q.sql!==q.saved)&&!(await askDiscard('Changing connection mode clears this workspace. Discard its unsaved queries?')))return;
    if((connectNow||store.activeId===draft.id)&&!(await discard()))return;
    if(connectNow&&draft.access==='read_write'&&draft.environment==='Production'&&!(await confirmReadWrite(draft.name)))return;
    if(old?.mode==='live'&&!(await confirmLeaseDiscard(draft.id)))return;
    testing.value=true;
    try {
      if(old?.mode==='live')await disconnectReporting(draft.id);
      const obsolete=await persistSecrets(draft, old);
      const saved:Connection={...draft,password:undefined,rememberPassword:undefined,access:undefined,transport:ssh?{...ssh,password:undefined,passphrase:undefined,privateKey:undefined,rememberPassword:undefined,rememberPassphrase:undefined}:draft.transport};
      const index=store.connections.findIndex(c=>c.id===draft.id);
      if(index<0)store.connections.push(saved);else store.connections[index]=saved;
      if(old?.mode!=='live')delete store.workspaces[draft.id];
      if(connectNow || store.activeId===draft.id)store.connect(draft.id);
      const workspace=store.workspaces[draft.id];
      if(workspace){workspace.database=draft.database;workspace.databases=[{name:draft.database,tables:[]}];workspace.table='';workspace.scope='database';workspace.view='SQL';for(const tab of workspace.queries){tab.result=null;tab.error='';}}
      for(const reference of obsolete)await deleteSecret(reference).catch(()=>undefined);
      const access=ssh?undefined:draft.access;
      if(connectNow){attemptId.value=`attempt_${crypto.randomUUID()}`;await store.connectLiveProfile(draft, attemptId.value, access);}
      clearTransientSecrets(draft);draft.password='';connectionDraftInitial.value=JSON.stringify(draft);
      connectionDialog.value=false;if(connectNow)manager.value=false;
      const mode=access==='read_write'?'read-write':'read-only';
      notify(connectNow?(draft.engine==='SQLite'?`Live ${mode} SQLite file opened.`:ssh?`Live ${mode} connection opened over SSH and verified TLS.`:`Live ${mode} connection opened over verified TLS.`):'Live profile saved; connect explicitly before executing SQL.');
    }catch(error){connectionError.value=(error as Error).message;}
    finally{testing.value=false;attemptId.value='';}
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
      if(!(await confirmLeaseDiscard(draft.id)))return;
      try{await disconnectReporting(draft.id);}catch(error){connectionError.value=(error as Error).message;return;}
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
      message: `The connection and its session data will be removed.${store.liveSessions[id]?.lease ? ` ${LEASE_DISCONNECT_WARNING}` : ''}`,
      action: async () => {
        // The lease warning is part of this confirmation; no second dialog.
        if (!(await disconnectIfLive(id, false))) return;
        // Forget saved credentials with the profile; a failure leaves only an unreachable item.
        if (nativeHostState.kind === 'desktop') for (const reference of secretReferences(store.connections.find((c) => c.id === id))) await deleteSecret(reference).catch(() => undefined);
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
    testing, testMessage, testOk, pendingHostKey, attemptId,
    newConnection, closeConnectionDialog, changeTransport, readCertificate, chooseSqliteFile,
    fetchHostKey, trustHostKey, rejectHostKey, forgetHostKey, chooseSshKeyFile, cancelAttempt,
    testConnection, saveConnection, removeConnection, closeConnection,
  };
}
