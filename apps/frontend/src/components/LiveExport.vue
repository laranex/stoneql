<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Download, X, CheckCircle2, AlertCircle, LoaderCircle } from 'lucide-vue-next';
import { useWorkspace } from '../stores/workspace';
import { pickExportFile, startExport, exportStatus, cancelExport } from '../services/liveConnections';
import type { TransferFormatDto, TransferStatus } from '../../../../contracts/generated/v1';
/**
 * M007 live export of the open table: the native host streams it inside a read-only snapshot
 * into a file chosen in the native save dialog (written to a temporary file, renamed when
 * complete). SQL or CSV only; GZIP, JSON and XLSX are not offered.
 */
const store = useWorkspace();
const format = ref<TransferFormatDto>('sql'), structure = ref(true);
const status = ref<TransferStatus | null>(null), error = ref(''), running = ref(false);
let transferId = '', stopped = false;
const mysqlFamily = computed(() => store.connection.engine === 'MySQL' || store.connection.engine === 'MariaDB');
const withStructure = computed(() => format.value === 'sql' && mysqlFamily.value && structure.value);
watch(() => [store.activeId, store.workspace.table], () => { if (!running.value) { status.value = null; error.value = ''; } });
async function run() {
  const handle = store.liveSessions[store.activeId]?.info?.handle, table = store.workspace.table;
  if (!handle || !table) { error.value = 'Connect this live profile first.'; return; }
  error.value = ''; status.value = null;
  try {
    const picked = await pickExportFile(`${table}.${format.value}`, format.value);
    if (!picked.grant) return;
    running.value = true; stopped = false;
    transferId = (await startExport(handle, picked.grant, table, format.value, withStructure.value)).transferId;
    while (!stopped) {
      status.value = await exportStatus(transferId);
      if (status.value.state !== 'running') break;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  } catch (e) { error.value = (e as Error).message; }
  finally { running.value = false; }
}
async function cancel() {
  if (!transferId) return;
  try { status.value = await cancelExport(transferId); } catch (e) { error.value = (e as Error).message; }
}
onBeforeUnmount(() => { stopped = true; });
</script>
<template>
  <div class="export-workflow" data-testid="live-export">
    <div class="notice info"><Download :size="15" /><span><strong>Live export</strong> of <code>{{ store.workspace.table }}</code>: rows are read in one read-only snapshot and written to the file you choose. The file appears only when the export is complete.</span></div>
    <section class="export-section">
      <h3>Format</h3>
      <div class="export-body">
        <div class="export-line">
          <label><input type="radio" v-model="format" value="sql" :disabled="running" /> SQL</label>
          <label><input type="radio" v-model="format" value="csv" :disabled="running" /> CSV</label>
          <label v-if="format === 'sql' && mysqlFamily"><input type="checkbox" v-model="structure" :disabled="running" /> Include CREATE TABLE</label>
        </div>
        <p class="muted" v-if="format === 'csv'">CSV: every value is quoted; NULL is the unquoted word NULL, so an empty string stays distinct. Binary columns cannot be exported as CSV.</p>
        <p class="muted" v-else-if="!mysqlFamily">PostgreSQL: data only (table structure needs pg_dump).</p>
      </div>
    </section>
    <div class="export-actions">
      <button class="button primary" type="button" :disabled="running" @click="run"><Download :size="13" /> Choose file and export</button>
      <button v-if="running" class="button" type="button" @click="cancel"><X :size="13" /> Cancel export</button>
    </div>
    <p v-if="running" role="status"><LoaderCircle :size="14" class="spin" /> Exporting… {{ status?.rows ?? '0' }} rows</p>
    <p v-if="error" class="notice error" role="alert"><AlertCircle :size="14" /> {{ error }}</p>
    <template v-if="status && !running">
      <p v-if="status.state === 'succeeded'" class="accent-text" role="status"><CheckCircle2 :size="14" /> Exported {{ status.rows }} rows ({{ status.bytes }} bytes) to {{ status.fileName }}.</p>
      <p v-else-if="status.state === 'cancelled'" role="status">Export cancelled; no file was created or replaced.</p>
      <p v-else class="notice error" role="alert"><AlertCircle :size="14" /> Export failed{{ status.detail ? `: ${status.detail}` : '' }}. No file was created or replaced.</p>
    </template>
  </div>
</template>
