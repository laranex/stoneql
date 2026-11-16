<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Upload, X, CheckCircle2, AlertCircle, LoaderCircle, FileText } from 'lucide-vue-next';
import { useWorkspace } from '../stores/workspace';
import { pickImportFile, startSqlImport, sqlImportStatus, cancelLiveJob } from '../services/liveConnections';
import type { SqlImportStatus } from '../../../../contracts/generated/v1';
/**
 * M007 d5 live SQL import (MySQL/MariaDB, read-write sessions). The file is chosen in the native
 * open dialog; the host reads it once, checks every statement and only then runs that same
 * snapshot one statement at a time with autocommit, stopping at the first failure.
 */
const store = useWorkspace();
const grant = ref(''), fileName = ref(''), error = ref(''), running = ref(false);
const status = ref<SqlImportStatus | null>(null), total = ref(0);
let importId = '', stopped = false;
const session = computed(() => store.liveSessions[store.activeId]);
const mysqlFamily = computed(() => store.connection.engine === 'MySQL' || store.connection.engine === 'MariaDB');
const readWrite = computed(() => session.value?.status === 'connected' && session.value.info?.access === 'read_write');
const production = computed(() => store.connection.environment === 'Production');
watch(() => store.activeId, () => { if (!running.value) { grant.value = ''; fileName.value = ''; status.value = null; error.value = ''; } });

async function choose() {
  error.value = ''; status.value = null;
  try {
    const picked = await pickImportFile();
    if (picked.grant) { grant.value = picked.grant; fileName.value = picked.fileName ?? ''; }
  } catch (e) { error.value = (e as Error).message; }
}
async function run() {
  const handle = session.value?.info?.handle;
  if (!handle || !grant.value) return;
  if (session.value?.lease) { error.value = 'Commit or roll back the open transaction first; an import runs on its own connection with autocommit.'; return; }
  error.value = ''; status.value = null; running.value = true; stopped = false;
  // A grant is one-shot: the host consumes it whether or not the import starts.
  const used = grant.value; grant.value = '';
  try {
    const accepted = await startSqlImport(handle, used);
    importId = accepted.importId; total.value = accepted.statementCount;
    while (!stopped) {
      status.value = await sqlImportStatus(importId);
      if (status.value.done) break;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  } catch (e) { error.value = (e as Error).message; }
  finally { running.value = false; }
}
async function cancel() {
  if (!importId) return;
  try { await cancelLiveJob(importId); } catch (e) { error.value = (e as Error).message; }
}
onBeforeUnmount(() => { stopped = true; });

const committed = (count: number) => `${count} earlier statement${count === 1 ? ' was' : 's were'} committed`;
const result = computed(() => {
  const s = status.value;
  if (!s?.done) return null;
  const at = (index: number, line: number) => `Statement ${index + 1} (line ${line})`;
  if (s.outcome === 'succeeded') return { kind: 'success', text: `Imported ${s.completed} statements from ${fileName.value} (${s.affectedRows} rows affected).` };
  if (s.outcome === 'failed' && s.failed) return { kind: 'error', text: `${at(s.failed.index, s.failed.line)} failed${s.failed.failure?.sqlState ? ` (SQLSTATE ${s.failed.failure.sqlState})` : ''}. ${committed(s.completed)}; later statements were not run.` };
  if (s.outcome === 'cancelled') return { kind: 'info', text: `Import cancelled before statement ${s.completed + 1}. ${committed(s.completed)}; nothing else was run.` };
  if (s.unknown) return { kind: 'error', text: `${at(s.unknown.index, s.unknown.line)} has an unknown outcome${s.cancellation !== 'none' ? ' (interrupted while running)' : ' (the connection was lost)'}: it may or may not have been applied. ${committed(s.completed)}; later statements were not run. It is listed under unknown outcomes.` };
  return { kind: 'error', text: `The import's outcome is unknown: ${s.completed} statements reported success, but the session state could not be confirmed. It is listed under unknown outcomes.` };
});
</script>
<template>
  <div class="export-workflow" data-testid="live-sql-import">
    <div v-if="!mysqlFamily" class="notice info" role="status">Live SQL import is available for MySQL and MariaDB connections.</div>
    <div v-else-if="!readWrite" class="notice info" role="status">Live SQL import needs a read-write session. Reconnect with read-write access (pending safety approval in current builds).</div>
    <template v-else>
      <div class="notice info"><Upload :size="15" /><span><strong>Live SQL import</strong> into <code>{{ session?.info?.database }}</code>: the whole file is checked first and nothing runs if any statement is refused. Statements then run one by one with autocommit and the import stops at the first failure; earlier statements stay committed. Files up to 1024 MiB, statements up to 16 MiB.</span></div>
      <div v-if="production" class="notice error" role="alert"><AlertCircle :size="14" /> PROD: this import changes data on <strong>{{ store.connection.name }}</strong>.</div>
      <div class="export-actions">
        <button class="button" type="button" :disabled="running" @click="choose"><FileText :size="13" /> Choose SQL file…</button>
        <span v-if="grant" class="muted" data-testid="sql-import-file">{{ fileName }}</span>
        <button class="button primary" type="button" :disabled="running || !grant" @click="run"><Upload :size="13" /> Import</button>
        <button v-if="running" class="button" type="button" @click="cancel"><X :size="13" /> Cancel import</button>
      </div>
      <p v-if="running" role="status"><LoaderCircle :size="14" class="spin" /> Importing… {{ status?.completed ?? 0 }} of {{ total }} statements</p>
      <p v-if="error" class="notice error" role="alert"><AlertCircle :size="14" /> {{ error }}</p>
      <p v-if="result" :class="result.kind === 'success' ? 'accent-text' : result.kind === 'info' ? '' : 'notice error'" :role="result.kind === 'success' || result.kind === 'info' ? 'status' : 'alert'" data-testid="sql-import-result">
        <CheckCircle2 v-if="result.kind === 'success'" :size="14" /><AlertCircle v-else :size="14" /> {{ result.text }}
      </p>
    </template>
  </div>
</template>
