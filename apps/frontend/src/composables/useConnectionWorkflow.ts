import { ref, watch, type Ref } from 'vue';
import { useWorkspace, LEASE_OUTCOME_UNREADABLE } from '../stores/workspace';
import { openLive, closeLive, queryLive, readWriteRoute, liveSupported, tlsOptional, usesTls, pickSqliteFile, pickSshKeyFile, scanHostKey, hostKeyStatus, trustScannedHostKey, forgetTrustedHostKey, cancelConnect, storeSecret, deleteSecret, describeTransactionEnd } from '../services/liveConnections';
import { hostState, mockConnectionsAvailable } from '../services/nativeHost';
import { mockTransport } from '../services/mockTransport';
import { referenceStore, secretStoreStatus } from '../services/secretStore';
import { connectionColors } from '../services/connectionPreferences';
import { liveDatabasesSql, serverDatabase, toDatabaseNames } from '../services/liveSchema';
import type { Connection, Transport } from '../types';
import type { SecretPurpose, SecretStoreKindDto, SshHostKey, SshTrustedHostKey, TransactionEnd } from '../../../../contracts/generated/v1';
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
/** Service name of saved references (Keychain service, Secret Service `service` attribute), so a user can remove a stuck item by hand. */
export const CREDENTIAL_SERVICE = 'me.thixpin.stoneql.credentials';
/** Deletes every reference; returns those that could not be removed (failures are never swallowed). */
async function deleteReferences(references: string[]): Promise<string[]> {
  const failed: string[] = [];
  for (const reference of references) {
    try { await deleteSecret(reference); } catch { failed.push(reference); }
  }
  return failed;
}
const storeName = (store: SecretStoreKindDto | undefined) =>
  store === 'keychain' ? `the Keychain (service ${CREDENTIAL_SERVICE})`
  : store === 'secret_service' ? `the desktop keyring (Secret Service, service ${CREDENTIAL_SERVICE})`
  : store === 'encrypted_file' ? 'StoneQL’s encrypted file'
  : store === 'dpapi_file' ? `StoneQL’s encrypted files (%APPDATA%\\${CREDENTIAL_SERVICE})`
  : `the system credential store (service ${CREDENTIAL_SERVICE})`;
const storeHint = (store: SecretStoreKindDto | undefined) =>
  store === 'keychain' ? 'The item stays in the Keychain until you delete it there.'
  : store === 'secret_service' ? 'The item stays in the desktop keyring until you delete it there.'
  : store === 'encrypted_file' ? 'The item stays in the encrypted file until you reset saved passwords.'
  : store === 'dpapi_file' ? 'The file stays in that folder until you delete it there.'
  : 'The item stays there until you delete it.';
/** Names the store each leftover reference is in (`label`: saved, old or newly saved) and how to remove it. */
async function credentialsLeft(references: string[], label = 'saved'): Promise<{ text: string; hint: string }> {
  let kind: SecretStoreKindDto | undefined;
  try { kind = (await secretStoreStatus()).kind; } catch { kind = undefined; }
  const counts = new Map<SecretStoreKindDto | undefined, number>();
  for (const reference of references) {
    const store = referenceStore(reference, kind);
    counts.set(store, (counts.get(store) ?? 0) + 1);
  }
  const parts = [...counts].map(([store, count]) => `${count} ${label} credential${count === 1 ? '' : 's'} could not be removed from ${storeName(store)}`);
  return { text: `${parts.join('; ')}.`, hint: [...counts.keys()].map(storeHint).join(' ') };
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
  const nativeHostState = hostState(), demo = mockConnectionsAvailable();
  const connectionDraftInitial = ref(''),
    connectionDialog = ref(false),
    connectionDraft = ref<Connection>(),
    connectionError = ref(''),
    testing = ref(false),
    /** A native file panel is opening or open (it takes about a second to appear). */
    picking = ref(false),
    testMessage = ref(''),
    /** Databases listed by the last successful live test, offered for Default database. */
    databaseOptions = ref<string[]>([]),
    testOk = ref(false),
    connectionEditing = ref(false),
    pendingHostKey = ref<SshHostKey>(),
    /** Native trust of the draft's SSH endpoint: null = not enrolled, undefined = unknown/loading. */
    hostKeyTrust = ref<SshTrustedHostKey | null>(),
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
  function newConnection(existing?: Connection) {
    connectionEditing.value = !!existing;
    connectionDraft.value = existing
      ? JSON.parse(JSON.stringify(existing))
      : {
          id: crypto.randomUUID(),
          // The desktop app has live connections only; the browser demo starts in Mock mode.
          mode: demo ? 'mock' : 'live',
          name: '',
          engine: 'MySQL',
          host: '127.0.0.1',
          port: 3306,
          user: demo ? 'root' : '',
          password: '',
          database: demo ? 'acme_store' : '',
          // New connections to this computer start with TLS off; other hosts always verify TLS.
          tls: false,
          color: connectionColors[0]!.value,
          environment: 'Development',
          customEnvironment: '',
          groupId: '',
          transport: { type: 'direct' },
        };
    connectionDraft.value!.mode ||= demo ? 'mock' : 'live';
    // Access is derived when connecting (A15: from `readOnly` and the transport), never kept.
    connectionDraft.value!.access = undefined;
    // Remember toggles mirror existing references; they are draft-only and never persisted.
    if (connectionDraft.value!.mode === 'live') {
      // New connections remember the password in the system keychain unless unticked; an
      // existing profile shows whether it has a saved reference.
      connectionDraft.value!.rememberPassword = existing ? !!connectionDraft.value!.passwordRef : true;
      const ssh = sshOf(connectionDraft.value);
      if (ssh) { ssh.rememberPassword = !!ssh.passwordRef; ssh.rememberPassphrase = !!ssh.passphraseRef; }
    }
    pendingHostKey.value = undefined;
    hostKeyTrust.value = undefined;
    testMessage.value = '';
    databaseOptions.value = [];
    connectionError.value = '';
    connectionDraftInitial.value = JSON.stringify(connectionDraft.value);
    connectionDialog.value = true;
    void refreshHostKeyTrust();
  }
  /** Reads the native trust state for the draft's SSH endpoint (no network). */
  async function refreshHostKeyTrust() {
    const draft = connectionDraft.value, ssh = liveSsh(draft);
    if (!ssh || !ssh.host.trim() || nativeHostState.kind !== 'desktop') { hostKeyTrust.value = null; return; }
    const endpoint = `${ssh.host.trim()}:${ssh.port}`;
    try {
      const status = await hostKeyStatus(ssh.host.trim(), ssh.port);
      const now = liveSsh(connectionDraft.value);
      if (connectionDraft.value === draft && now && `${now.host.trim()}:${now.port}` === endpoint) hostKeyTrust.value = status.trusted;
    } catch (error) { if (connectionDraft.value === draft) { hostKeyTrust.value = null; connectionError.value = (error as Error).message; } }
  }
  // F-04: editing the SSH host or port invalidates the shown trust and any pending scan at once;
  // the native state of the new endpoint is read after a short pause.
  let trustTimer: ReturnType<typeof setTimeout> | undefined;
  watch(() => { const ssh = liveSsh(); return ssh ? `${ssh.host}:${ssh.port}` : ''; }, (now, before) => {
    if (now === before) return;
    pendingHostKey.value = undefined;
    hostKeyTrust.value = undefined;
    clearTimeout(trustTimer);
    trustTimer = setTimeout(() => void refreshHostKeyTrust(), 300);
  });
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
  /** Explicit enrollment: the host confirms natively and stores the key it scanned itself. */
  async function trustHostKey() {
    const draft = connectionDraft.value, ssh = sshOf(draft), key = pendingHostKey.value;
    if (!ssh || !key || testing.value) return;
    connectionError.value = '';
    testing.value = true;
    try {
      const status = await trustScannedHostKey(ssh.host.trim(), ssh.port, key.fingerprint);
      if (connectionDraft.value === draft) { hostKeyTrust.value = status.trusted; pendingHostKey.value = undefined; testMessage.value = ''; }
    } catch (error) {
      // Declined in the native dialog: nothing changed; the pending key stays for review.
      if ((error as { code?: string }).code !== 'CANCELLED') connectionError.value = (error as Error).message;
    } finally { testing.value = false; }
  }
  function rejectHostKey() { pendingHostKey.value = undefined; }
  /** Rotation is deliberate: a changed key is refused until the old one is forgotten here. */
  function forgetHostKey() {
    const draft = connectionDraft.value, ssh = sshOf(draft);
    if (!ssh || testing.value) return;
    const host = ssh.host.trim(), port = ssh.port;
    confirm({
      title: 'Forget SSH host key?',
      acceptLabel: 'Forget host key',
      message: `Forget the trusted host key for ${host}:${port}? All profiles using this SSH server will be refused until it is verified again. Saved SSH passwords for it must be re-entered.`,
      action: async () => {
        try {
          const status = await forgetTrustedHostKey(host, port);
          if (connectionDraft.value === draft) { hostKeyTrust.value = status.trusted; pendingHostKey.value = undefined; }
        } catch (error) { connectionError.value = (error as Error).message; }
      },
    });
  }
  async function chooseSshKeyFile() {
    const draft = connectionDraft.value, ssh = sshOf(draft);
    if (!draft || !ssh || testing.value) return;
    connectionError.value = '';
    testing.value = true; picking.value = true;
    try {
      const selected = await pickSshKeyFile();
      if (selected.grant && connectionDraft.value === draft) { ssh.keyGrant = selected.grant; ssh.keyFileName = selected.fileName || ''; }
    } catch (error) { connectionError.value = (error as Error).message; }
    finally { testing.value = false; picking.value = false; }
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
      const failed = await deleteReferences(created);
      if (failed.length) throw new Error(`${(error as Error).message} ${(await credentialsLeft(failed, 'newly saved')).text}`);
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
    testing.value=true;picking.value=true;
    try {
      const selected=await pickSqliteFile();
      if(selected.grant&&connectionDraft.value===draft){draft.sqliteGrant=selected.grant;draft.database=selected.fileName||'';testMessage.value='';}
    }catch(error){connectionError.value=(error as Error).message;}
    finally{testing.value=false;picking.value=false;}
  }
  /** The first required live field left empty, named for the user; '' when complete. */
  function liveMissing(draft: Connection, test = false): string {
    if (draft.engine === 'SQLite') return '';
    if (!draft.host.trim()) return 'Enter the host.';
    if (!draft.user.trim()) return 'Enter the username.';
    if (!test && !draft.database.trim()) return 'Choose or enter the default database. Test connection lists the databases you can use.';
    return '';
  }
  /** Read-write for a profile not marked read-only on a read-write route, when the host allows it. */
  function liveAccess(draft: Connection): Connection['access'] {
    const enabled = nativeHostState.kind === 'desktop' && nativeHostState.info.readWriteSql === true;
    return enabled && readWriteRoute(draft) && !draft.readOnly ? 'read_write' : undefined;
  }
  /** A verified-TLS failure to this computer says how to connect without TLS; never automatic. */
  function withTlsHint(error: unknown, draft?: Connection): string {
    const message = (error as Error).message;
    return draft && (error as { code?: string }).code === 'CONNECTION_UNAVAILABLE' && tlsOptional(draft) && usesTls(draft)
      ? `${message} If this local server has no certificate trusted for this host name, turn off Use TLS and try again.`
      : message;
  }
  async function testConnection() {
    if (!connectionDraft.value) return;
    // One notice per attempt: a new test replaces an earlier save error (and vice versa).
    connectionError.value = '';
    const ssh = liveSsh();
    if (ssh && !hostKeyTrust.value) {
      await fetchHostKey();
      testMessage.value = 'Verify and trust the SSH host key, then test again.'; testOk.value = false;
      return;
    }
    // Checked here so an empty field never reaches the host as an opaque invalid request.
    // The database may be left blank for a test: it is then chosen from the listed databases.
    const missing = connectionDraft.value.mode === 'live' ? liveMissing(connectionDraft.value, true) : '';
    if (missing) { testMessage.value = missing; testOk.value = false; return; }
    testing.value = true;
    try {
      if(connectionDraft.value.mode==='live') {
        attemptId.value=`attempt_${crypto.randomUUID()}`;
        const draft=connectionDraft.value, chosen=draft.database.trim();
        const info=await openLive(draft.engine==='SQLite'||chosen?draft:{...draft,database:serverDatabase(draft.engine)}, attemptId.value);
        // List the account's databases on the test session (read-only); a refusal only means no
        // list is offered, never a failed test.
        try{databaseOptions.value=draft.engine==='SQLite'?[]:toDatabaseNames(await queryLive(info.handle, liveDatabasesSql(draft.engine)));}
        catch{databaseOptions.value=[];}
        finally{await closeLive(info.handle);}
        testMessage.value=`${connectionDraft.value.engine==='SQLite'?'Read-only file opened':ssh?'Verified SSH host key and TLS connection successful':usesTls(connectionDraft.value)?'Verified TLS connection successful':'Connected without TLS (this computer only)'} · ${info.engineVersion} · Test session closed.${!chosen&&databaseOptions.value.length?' Choose the default database below.':''}`;
        testOk.value=true;
      } else {
        const result=await mockTransport.test(connectionDraft.value);
        testMessage.value=result.message;testOk.value=result.ok;
      }
    }catch(error){testMessage.value=withTlsHint(error, connectionDraft.value);testOk.value=false;}
    finally{testing.value=false;attemptId.value='';}
  }
  async function saveLiveConnection(draft: Connection, connectNow: boolean) {
    if(liveMissing(draft)){connectionError.value=liveMissing(draft);return;}
    if(nativeHostState.kind!=='desktop'||!liveSupported(draft)||!draft.database||(draft.engine==='SQLite'&&!draft.sqliteGrant)){connectionError.value=draft.engine==='SQLite'?'Live SQLite requires the desktop app, Direct transport and a file chosen with Choose file.':'Live connections require the desktop app, MySQL, MariaDB or PostgreSQL over Direct or SSH and a database name.';return;}
    const ssh=liveSsh(draft);
    if(ssh&&!hostKeyTrust.value){connectionError.value='Fetch, verify and trust the SSH host key before saving a live SSH profile.';return;}
    const old=store.connections.find(c=>c.id===draft.id);
    if(old&&old.mode!=='live'&&store.workspaces[draft.id]?.queries.some(q=>q.sql!==q.saved)&&!(await askDiscard('Changing connection mode clears this workspace. Discard its unsaved queries?')))return;
    if((connectNow||store.activeId===draft.id)&&!(await discard()))return;
    // A15: read-write unless the profile is read-only; PostgreSQL over SSH stays read-only.
    const access=liveAccess(draft);
    if(connectNow&&access==='read_write'&&draft.environment==='Production'&&!(await confirmReadWrite(draft.name)))return;
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
      const leftOver=await deleteReferences(obsolete);
      if(leftOver.length)notify(`Saved, but ${(await credentialsLeft(leftOver, 'old')).text}`,'error');
      if(connectNow){attemptId.value=`attempt_${crypto.randomUUID()}`;await store.connectLiveProfile(draft, attemptId.value, access);}
      clearTransientSecrets(draft);draft.password='';connectionDraftInitial.value=JSON.stringify(draft);
      connectionDialog.value=false;if(connectNow)manager.value=false;
      const mode=access==='read_write'?'read-write':'read-only';
      notify(connectNow?(draft.engine==='SQLite'?`Live ${mode} SQLite file opened.`:ssh?`Live ${mode} connection opened over SSH and verified TLS.`:usesTls(draft)?`Live ${mode} connection opened over verified TLS.`:`Live ${mode} connection opened without TLS (this computer only).`):'Live profile saved; connect explicitly before executing SQL.');
    }catch(error){connectionError.value=withTlsHint(error, draft);}
    finally{testing.value=false;attemptId.value='';}
  }
  async function saveConnection(connectNow = true) {
    const draft = connectionDraft.value;
    if (!draft) return;
    connectionError.value = '';
    testMessage.value = '';
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
        // Forget saved credentials with the profile. A failure keeps the profile and asks again;
        // without the desktop host the references cannot be removed, which is also reported.
        const references = secretReferences(store.connections.find((c) => c.id === id));
        const failed = nativeHostState.kind === 'desktop' ? await deleteReferences(references) : references;
        if (failed.length) {
          const left = await credentialsLeft(failed);
          confirm({
            title: 'Credentials not removed',
            acceptLabel: 'Remove anyway',
            message: `${left.text} Remove the profile anyway? ${left.hint}`,
            action: () => forgetProfile(id),
          });
          return;
        }
        forgetProfile(id);
      },
    });
  }
  function forgetProfile(id: string) {
    store.connections = store.connections.filter((c) => c.id !== id);
    store.opened = store.opened.filter((c) => c !== id);
    delete store.workspaces[id];
    if (store.activeId === id && store.connections[0]) store.connect(store.connections[0].id);
    if (!store.connections.length) manager.value = true;
    notify('Connection removed.');
  }
  return {
    connectionDialog, connectionDraft, connectionDraftInitial, connectionError, connectionEditing,
    testing, picking, testMessage, testOk, databaseOptions, pendingHostKey, hostKeyTrust, refreshHostKeyTrust, attemptId,
    newConnection, closeConnectionDialog, changeTransport, readCertificate, chooseSqliteFile,
    fetchHostKey, trustHostKey, rejectHostKey, forgetHostKey, chooseSshKeyFile, cancelAttempt,
    testConnection, saveConnection, removeConnection, closeConnection,
  };
}
