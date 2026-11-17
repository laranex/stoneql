<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue';
import { Download, X, CheckCircle2, AlertCircle, LoaderCircle } from 'lucide-vue-next';
import { useWorkspace } from '../stores/workspace';
import { pickExportFile, startExport, exportStatus, cancelExport } from '../services/liveConnections';
import type { ExportOptionsDto, InsertVerbDto, TransferFormatDto, TransferStatus } from '../../../../contracts/generated/v1';
/**
 * M007 live export (phpMyAdmin-style quick/custom): the open table, or chosen tables / every
 * table of the database. The native host streams it inside one read-only snapshot into a file
 * chosen in the native save dialog (written to a temporary file, renamed when complete),
 * optionally GZIP-compressed. Every option is checked natively again.
 */
const props = defineProps<{ whole?: boolean }>();
const store = useWorkspace();
const engine = computed(() => store.connection.engine);
const mysqlFamily = computed(() => engine.value === 'MySQL' || engine.value === 'MariaDB');
const baseTables = computed(() => (store.database?.tables ?? []).filter((t) => (t.kind ?? 'table') === 'table').map((t) => t.name));
const c = reactive({
  scope: (props.whole ? 'database' : 'table') as 'table' | 'database',
  mode: 'quick' as 'quick' | 'custom',
  content: 'both' as 'both' | 'structure' | 'data',
  tables: [] as string[],
  format: 'sql' as TransferFormatDto,
  drop: false, ifExists: true, create: true, autoIncrement: true,
  insertStyle: 'extended' as 'standard' | 'extended' | 'ignore' | 'replace',
  transaction: false, foreignKeys: true,
  csv: { delimiter: ',', enclosure: '"', nullText: 'NULL', headers: true },
  gzip: false,
});
const status = ref<TransferStatus | null>(null), error = ref(''), running = ref(false);
let transferId = '', stopped = false;
watch(() => [store.activeId, store.workspace.database, props.whole] as const, () => {
  c.scope = props.whole ? 'database' : 'table';
  c.tables = [...baseTables.value];
}, { immediate: true });
watch(baseTables, (names) => { if (!c.tables.length) c.tables = [...names]; });
watch(() => [store.activeId, store.workspace.table], () => { if (!running.value) { status.value = null; error.value = ''; } });
/** PostgreSQL structure needs pg_dump: data only. CSV takes one table. */
const structureAvailable = computed(() => mysqlFamily.value && c.format === 'sql');
const custom = computed(() => c.mode === 'custom');
/** Quick always exports structure and data (data only where structure is unavailable). */
const content = computed(() => (!structureAvailable.value ? 'data' : custom.value ? c.content : 'both'));
const selected = computed(() => (c.scope === 'table' ? [store.workspace.table] : custom.value ? c.tables : null));
const csvAllowed = computed(() => c.scope === 'table' || (custom.value && c.tables.length === 1));
watch(csvAllowed, (allowed) => { if (!allowed && c.format === 'csv') c.format = 'sql'; });
const subject = computed(() => (c.scope === 'table' ? store.workspace.table : store.workspace.database));
const problem = computed(() => {
  if (!store.liveSessions[store.activeId]?.info?.handle || store.liveSessions[store.activeId]?.status !== 'connected') return 'Connect this live profile first.';
  if (c.scope === 'database' && custom.value && !c.tables.length) return 'Select at least one table.';
  if (c.format === 'sql' && content.value !== 'data' && !c.create && !c.drop) return 'Choose CREATE TABLE or DROP TABLE, or export data only.';
  if (c.format === 'csv') {
    if ([...c.csv.delimiter].length !== 1) return 'Enter one character to separate columns.';
    if (/[\r\n]/.test(c.csv.nullText) || c.csv.nullText.includes(c.csv.delimiter) || (c.csv.enclosure && c.csv.nullText.includes(c.csv.enclosure))) return 'The NULL text cannot contain the separator, the enclosure or a line break.';
    if (c.csv.enclosure && c.csv.enclosure === c.csv.delimiter) return 'The enclosure must differ from the separator.';
  }
  return '';
});
function options(): ExportOptionsDto {
  const quick = !custom.value, verb: InsertVerbDto = c.insertStyle === 'ignore' ? 'insert_ignore' : c.insertStyle === 'replace' ? 'replace' : 'insert';
  return {
    dropTable: !quick && c.drop && content.value !== 'data',
    ifExists: c.ifExists,
    rowsPerInsert: !quick && c.insertStyle === 'standard' ? 1 : 500,
    insertVerb: quick ? 'insert' : verb,
    autoIncrement: quick || c.autoIncrement,
    transaction: !quick && c.transaction && content.value !== 'structure',
    // Several tables import in any order with references checked afterwards (MySQL/MariaDB).
    disableForeignKeys: mysqlFamily.value && (quick ? c.scope === 'database' : c.foreignKeys),
    csvDelimiter: quick ? ',' : c.csv.delimiter,
    csvEnclosure: quick ? '"' : c.csv.enclosure || null,
    csvNull: quick ? 'NULL' : c.csv.nullText,
    csvHeader: quick || c.csv.headers,
    compress: c.gzip,
  };
}
const extension = computed(() => `${c.format}${c.gzip ? '.gz' : ''}`);
async function run() {
  const handle = store.liveSessions[store.activeId]?.info?.handle;
  if (problem.value || !handle) { error.value = problem.value || 'Connect this live profile first.'; return; }
  error.value = ''; status.value = null;
  try {
    const picked = await pickExportFile(`${subject.value || 'export'}.${extension.value}`, c.format, c.gzip);
    if (!picked.grant) return;
    running.value = true; stopped = false;
    const structure = c.format === 'sql' && content.value !== 'data' && (!custom.value || c.create);
    const data = content.value !== 'structure';
    transferId = (await startExport(handle, picked.grant, selected.value, c.format, structure, data, options())).transferId;
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
    <div class="notice info"><Download :size="15" /><span><strong>Live export</strong> of {{ c.scope === 'database' ? (custom ? 'chosen tables of ' : 'every table of ') : '' }}<code>{{ subject }}</code>: rows are read in one read-only snapshot and written to the file you choose. The file appears only when the export is complete.<template v-if="c.scope === 'database'"> Views, routines, triggers and events are not included yet.</template></span></div>
    <form @submit.prevent="run">
      <fieldset class="export-fieldset" :disabled="running">
        <section class="export-section">
          <h3>Export method</h3>
          <div class="export-body">
            <div v-if="!whole" class="export-line">
              <span class="export-label">Export from:</span
              ><label><input type="radio" v-model="c.scope" value="table" /> Current table</label
              ><label><input type="radio" v-model="c.scope" value="database" /> Database</label>
            </div>
            <div class="export-method-row">
              <div class="export-modes">
                <label class="export-choice"><input type="radio" v-model="c.mode" value="quick" /><span><strong>Quick</strong> — display only the minimal options</span></label
                ><label class="export-choice"><input type="radio" v-model="c.mode" value="custom" /><span><strong>Custom</strong> — display all possible options</span></label>
              </div>
              <label v-if="custom" class="export-type-control"
                >Export type<select v-model="c.content" aria-label="Export content" :disabled="!structureAvailable">
                  <option value="both">Structure and Data</option>
                  <option value="structure">Structure Only</option>
                  <option value="data">Data Only</option>
                </select></label>
            </div>
            <p v-if="!mysqlFamily" class="capability-note">PostgreSQL: data only (table structure needs pg_dump).</p>
          </div>
        </section>
        <section v-if="custom && c.scope === 'database'" class="export-section">
          <h3>Tables <span class="export-count">{{ c.tables.length }} of {{ baseTables.length }} selected</span></h3>
          <div class="export-body">
            <div class="table-selection-actions">
              <button type="button" @click="c.tables = [...baseTables]">Select all</button><span>/</span><button type="button" @click="c.tables = []">Deselect all</button>
            </div>
            <div class="export-table-list" aria-label="Tables to export">
              <label v-for="name in baseTables" :key="name"><input type="checkbox" v-model="c.tables" :value="name" /><span>{{ name }}</span></label>
            </div>
          </div>
        </section>
        <section class="export-section">
          <h3>Format</h3>
          <div class="export-body">
            <div class="export-line">
              <select v-model="c.format" aria-label="Export format">
                <option value="sql">SQL</option>
                <option value="csv" :disabled="!csvAllowed">CSV{{ csvAllowed ? '' : ' (one table)' }}</option>
                <option value="json">JSON</option></select
              ><span class="muted">{{ c.format === 'sql' ? 'Database structure and SQL statements' : c.format === 'csv' ? 'Comma-separated values for one table; binary columns cannot be exported as CSV' : 'Rows as JSON objects; values as exact strings, NULL as null; binary columns cannot be exported as JSON' }}</span>
            </div>
          </div>
        </section>
        <template v-if="custom">
          <section v-if="c.format === 'sql'" class="export-section">
            <h3>Format-specific options <span class="muted">SQL · {{ engine }}</span></h3>
            <div class="export-body">
              <template v-if="mysqlFamily">
                <h4>Object creation options</h4>
                <label class="export-choice"><input type="checkbox" v-model="c.drop" :disabled="content === 'data'" /> Add DROP TABLE statement</label
                ><label class="export-choice indented"><input type="checkbox" v-model="c.ifExists" :disabled="!c.drop || content === 'data'" /> Add IF EXISTS to DROP TABLE</label
                ><label class="export-choice"><input type="checkbox" v-model="c.create" :disabled="content === 'data'" /> Add CREATE TABLE statement</label
                ><label class="export-choice"><input type="checkbox" v-model="c.autoIncrement" :disabled="content === 'data' || !c.create" /> Include AUTO_INCREMENT values where applicable</label>
              </template>
              <h4>Data creation options</h4>
              <div class="export-line">
                <span class="export-label">INSERT statement style:</span
                ><select v-model="c.insertStyle" aria-label="INSERT statement style" :disabled="content === 'structure'">
                  <option value="standard">Complete INSERT — one row per statement</option>
                  <option value="extended">Extended INSERT — multiple rows per statement</option>
                  <option v-if="mysqlFamily" value="ignore">INSERT IGNORE</option>
                  <option v-if="mysqlFamily" value="replace">REPLACE</option>
                </select>
              </div>
              <label class="export-choice"><input type="checkbox" v-model="c.transaction" :disabled="content === 'structure'" /> Enclose data in a transaction</label
              ><p v-if="c.transaction" class="capability-note">StoneQL's SQL import runs each statement on its own and refuses transaction statements; import this file with the mysql or psql client.</p
              ><label v-if="mysqlFamily" class="export-choice"><input type="checkbox" v-model="c.foreignKeys" /> Temporarily disable foreign key checks</label>
              <p v-else class="capability-note">Global foreign key disabling is unavailable for PostgreSQL; constraints remain enabled.</p>
            </div>
          </section>
          <section v-else-if="c.format === 'csv'" class="export-section">
            <h3>Format-specific options — CSV</h3>
            <div class="export-body">
              <div class="export-line">
                <label>Columns separated with<input v-model="c.csv.delimiter" maxlength="1" aria-label="CSV delimiter" class="short-input" /></label
                ><label>Columns enclosed with<select v-model="c.csv.enclosure" aria-label="CSV enclosure">
                  <option value='"'>Double quote (")</option>
                  <option value="'">Single quote (')</option>
                  <option value="">None</option></select></label
                ><label>Replace NULL with<input v-model="c.csv.nullText" aria-label="CSV null value" class="short-input" /></label>
              </div>
              <label class="export-choice"><input type="checkbox" v-model="c.csv.headers" /> Put column names in the first row</label>
              <p v-if="!c.csv.enclosure" class="capability-note">Without an enclosure, a value containing the separator or a line break, or equal to the NULL text, stops the export and names its column.</p>
            </div>
          </section>
        </template>
        <section class="export-section">
          <h3>Output</h3>
          <div class="export-body">
            <div class="export-line">
              <span class="export-label">Compression:</span
              ><label><input type="radio" v-model="c.gzip" :value="false" /> None</label
              ><label><input type="radio" v-model="c.gzip" :value="true" /> GZIP</label>
              <span class="muted">File: {{ subject || 'export' }}.{{ extension }} (chosen in the save dialog)</span>
            </div>
          </div>
        </section>
      </fieldset>
      <div class="export-actions">
        <button class="button primary" type="submit" :disabled="running"><Download :size="13" /> Choose file and export</button>
        <button v-if="running" class="button" type="button" @click="cancel"><X :size="13" /> Cancel export</button>
      </div>
    </form>
    <p v-if="running" role="status"><LoaderCircle :size="14" class="spin" /> Exporting… {{ status?.rows ?? '0' }} rows</p>
    <p v-if="error" class="notice error" role="alert"><AlertCircle :size="14" /> {{ error }}</p>
    <template v-if="status && !running">
      <p v-if="status.state === 'succeeded'" class="accent-text" role="status"><CheckCircle2 :size="14" /> Exported {{ status.rows }} rows ({{ status.bytes }} bytes) to {{ status.fileName }}.</p>
      <p v-else-if="status.state === 'cancelled'" role="status">Export cancelled; no file was created or replaced.</p>
      <p v-else class="notice error" role="alert"><AlertCircle :size="14" /> Export failed{{ status.detail ? `: ${status.detail}` : '' }}. No file was created or replaced.</p>
    </template>
  </div>
</template>
