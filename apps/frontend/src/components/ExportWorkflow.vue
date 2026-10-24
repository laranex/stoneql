<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount, toRaw } from 'vue';
import {
  Download,
  CircleHelp,
  CheckCircle2,
  AlertCircle,
  LoaderCircle,
  X,
  ChevronDown,
} from 'lucide-vue-next';
import { useWorkspace } from '../stores/workspace';
import { useExportStore } from '../stores/export';
import { capabilities } from '../types/export';
import {
  effectiveExport,
  validateExport,
  exportSummary,
  simulatedExportTransport,
} from '../services/exportService';
const store = useWorkspace(),
  exports = useExportStore();
const key = computed(() => `${store.activeId}:${store.workspace.database}`);
const c = computed(() => exports.get(key.value, store.workspace.table));
if (store.workspace.scope === 'database') { c.value.scope = 'database'; c.value.tables = store.database?.tables.map(t => t.name) || []; }
const cap = computed(() => capabilities[store.connection.engine]);
const database = computed(() => store.database!);
const tables = computed(() => database.value.tables);
const effective = computed(() =>
  effectiveExport(
    JSON.parse(JSON.stringify(c.value)),
    store.connection.engine,
    c.value.scope === 'database' ? tables.value.map((t) => t.name) : [store.workspace.table],
  ),
);
const errors = computed(() => validateExport(effective.value, database.value));
const summary = computed(() =>
  exportSummary(effective.value, database.value, store.connection.engine),
);
const state = ref<'idle' | 'running' | 'success' | 'cancelled' | 'error'>('idle'),
  progress = ref(0),
  error = ref(''),
  simulateFailure = ref(false);
const completed = ref<ReturnType<typeof exportSummary> | null>(null);
let controller: AbortController | undefined;
watch(
  () => store.workspace.table,
  (name) => {
    if (c.value.scope === 'table') c.value.tables = name ? [name] : [];
  },
  { immediate: true },
);
watch(key, () => {
  controller?.abort();
  state.value = 'idle';
  completed.value = null;
});
watch(
  () => c.value.scope,
  (scope) => {
    c.value.tables = scope === 'table' ? [store.workspace.table] : tables.value.map((t) => t.name);
  },
);
async function run() {
  if (errors.value.length) return;
  controller = new AbortController();
  state.value = 'running';
  progress.value = 0;
  completed.value = null;
  error.value = '';
  const config = JSON.parse(JSON.stringify(effective.value)),
    db = JSON.parse(JSON.stringify(toRaw(database.value))),
    engine = store.connection.engine;
  try {
    completed.value = await simulatedExportTransport.run(
      config,
      db,
      engine,
      controller.signal,
      (p) => (progress.value = p),
      simulateFailure.value,
    );
    state.value = 'success';
  } catch (e) {
    if ((e as Error).name === 'AbortError') state.value = 'cancelled';
    else {
      state.value = 'error';
      error.value = (e as Error).message;
    }
  }
}
onBeforeUnmount(() => controller?.abort());
</script>
<template>
  <div class="export-workflow">
    <div class="notice info">
      <CircleHelp :size="15" /><span
        ><strong>Simulation only.</strong> No real backup, file, SQL dump, or compressed archive
        will be created.</span
      >
    </div>
    <form @submit.prevent="run">
      <fieldset class="export-fieldset" :disabled="state === 'running'">
        <section class="export-section">
          <h3>Export method</h3>
          <div class="export-body">
            <div class="export-line">
              <span class="export-label">Export from:</span
              ><label><input type="radio" v-model="c.scope" value="table" /> Current table</label
              ><label><input type="radio" v-model="c.scope" value="database" /> Database</label>
            </div>
            <div class="export-method-row">
              <div class="export-modes">
                <label class="export-choice"
                  ><input type="radio" v-model="c.mode" value="quick" /><span
                    ><strong>Quick</strong> — display only the minimal options</span
                  ></label
                ><label class="export-choice"
                  ><input type="radio" v-model="c.mode" value="custom" /><span
                    ><strong>Custom</strong> — display all possible options</span
                  ></label
                >
              </div>
              <label v-if="c.mode === 'custom'" class="export-type-control"
                >Export type<select v-model="c.content" aria-label="Export content">
                  <option value="both">Structure and Data</option>
                  <option value="structure">Structure Only</option>
                  <option value="data">Data Only</option>
                </select></label
              >
            </div>
          </div>
        </section>
        <section v-if="c.mode === 'custom'" class="export-section">
          <h3>
            Tables
            <span class="export-count">{{ c.tables.length }} of {{ tables.length }} selected</span>
          </h3>
          <div class="export-body">
            <template v-if="c.scope === 'database'"
              ><div class="table-selection-actions">
                <button type="button" @click="c.tables = tables.map((t) => t.name)">
                  Select all</button
                ><span>/</span><button type="button" @click="c.tables = []">Deselect all</button
                ><span class="muted">{{ c.tables.length }} selected</span>
              </div>
              <div class="export-table-list">
                <label v-for="t in tables" :key="t.name"
                  ><input type="checkbox" v-model="c.tables" :value="t.name" /><span>{{
                    t.name
                  }}</span
                  ><small>{{ t.rows.length.toLocaleString() }} rows</small></label
                >
              </div></template
            >
            <div v-else class="export-line">
              <strong>{{ store.workspace.table }}</strong
              ><span class="muted"
                >{{ store.table?.rows.length.toLocaleString() }} rows · current table only</span
              >
            </div>
          </div>
        </section>
        <section class="export-section">
          <h3>Format</h3>
          <div class="export-body">
            <div class="export-line">
              <select v-model="c.format" aria-label="Export format">
                <option>SQL</option>
                <option>CSV</option>
                <option>JSON</option></select
              ><span class="muted">{{
                c.format === 'SQL'
                  ? 'Database structure and SQL statements'
                  : c.format === 'CSV'
                    ? 'Comma-separated values, one dataset per table'
                    : 'Structured JSON data and optional schema'
              }}</span>
            </div>
          </div>
        </section>
        <template v-if="c.mode === 'custom'">
          <section class="export-section" v-if="c.format === 'SQL'">
            <h3>
              Format-specific options <span class="muted">SQL · {{ store.connection.engine }}</span>
            </h3>
            <div class="export-body">
              <h4>Object creation options</h4>
              <label class="export-choice"
                ><input type="checkbox" v-model="c.drop" :disabled="c.content === 'data'" /> Add
                DROP TABLE statement</label
              ><label class="export-choice indented"
                ><input
                  type="checkbox"
                  v-model="c.ifExists"
                  :disabled="!c.drop || c.content === 'data'"
                />
                Add IF EXISTS to DROP TABLE</label
              ><label class="export-choice"
                ><input type="checkbox" v-model="c.create" :disabled="c.content === 'data'" /> Add
                CREATE TABLE statement</label
              ><label v-if="cap.autoIncrement" class="export-choice"
                ><input
                  type="checkbox"
                  v-model="c.autoIncrement"
                  :disabled="c.content === 'data' || !c.create"
                />
                Include
                {{
                  store.connection.engine === 'SQLite' ? 'AUTOINCREMENT sequence' : 'AUTO_INCREMENT'
                }}
                values where applicable</label
              >
              <p v-else class="capability-note">
                PostgreSQL uses identity / sequence definitions in CREATE TABLE. MySQL
                AUTO_INCREMENT syntax is not available.
              </p>
              <h4>Data creation options</h4>
              <div class="export-line">
                <span class="export-label">INSERT statement style:</span
                ><select
                  v-model="c.insertStyle"
                  aria-label="INSERT statement style"
                  :disabled="c.content === 'structure'"
                >
                  <option value="standard">Complete INSERT — one row per statement</option>
                  <option value="extended">Extended INSERT — multiple rows per statement</option>
                  <option v-if="cap.insertStyles.includes('ignore')" value="ignore">
                    {{
                      store.connection.engine === 'SQLite' ? 'INSERT OR IGNORE' : 'INSERT IGNORE'
                    }}
                  </option>
                  <option v-if="cap.insertStyles.includes('replace')" value="replace">
                    {{ store.connection.engine === 'SQLite' ? 'INSERT OR REPLACE' : 'REPLACE' }}
                  </option>
                </select>
              </div>
              <label class="export-choice"
                ><input
                  type="checkbox"
                  v-model="c.transaction"
                  :disabled="c.content === 'structure'"
                />
                Enclose data in a transaction</label
              ><label v-if="cap.foreignKeys" class="export-choice"
                ><input type="checkbox" v-model="c.foreignKeys" /> Temporarily disable foreign key
                checks{{
                  store.connection.engine === 'SQLite' ? ' (outside transaction)' : ''
                }}</label
              >
              <p v-else class="capability-note">
                Global foreign key disabling is unavailable for PostgreSQL; constraints remain
                enabled.
              </p>
              <h4>Additional database objects</h4>
              <div class="export-object-options">
                <label
                  v-for="obj in ['views', 'triggers', 'routines', 'events'] as const"
                  :key="obj"
                  class="export-choice"
                  :class="{ unsupported: !cap[obj] }"
                  ><input
                    type="checkbox"
                    v-model="c[obj]"
                    :disabled="!cap[obj] || c.content === 'data'"
                  />{{ obj[0]!.toUpperCase() + obj.slice(1)
                  }}<span class="muted">{{
                    cap[obj] ? `(${database[obj]?.length || 0} in mock database)` : '(not supported)'
                  }}</span></label
                >
              </div>
              <p class="capability-note">
                Object definitions are counted from the mock schema. No definitions are dumped or files created.
              </p>
            </div>
          </section>
          <section v-else-if="c.format === 'CSV'" class="export-section">
            <h3>Format-specific options — CSV</h3>
            <div class="export-body">
              <div class="export-line">
                <label
                  >Columns separated with<input
                    v-model="c.csv.delimiter"
                    maxlength="1"
                    aria-label="CSV delimiter"
                    class="short-input" /></label
                ><label
                  >Columns enclosed with<select
                    v-model="c.csv.enclosure"
                    aria-label="CSV enclosure"
                  >
                    <option value='"'>Double quote (")</option>
                    <option value="'">Single quote (')</option>
                    <option value="">None</option>
                  </select></label
                ><label
                  >Replace NULL with<input
                    v-model="c.csv.nullValue"
                    class="short-input"
                    aria-label="CSV null value"
                /></label>
              </div>
              <label class="export-choice"
                ><input type="checkbox" v-model="c.csv.headers" /> Put column names in the first
                row</label
              >
              <p class="capability-note">
                UTF-8 encoding · CRLF line endings · a separate logical CSV dataset for each
                selected table.
              </p>
            </div>
          </section>
          <section v-else class="export-section">
            <h3>Format-specific options — JSON</h3>
            <div class="export-body">
              <label class="export-choice"
                ><input type="checkbox" v-model="c.json.pretty" /> Pretty print (2-space
                indentation)</label
              ><label class="export-choice"
                ><input type="checkbox" v-model="c.json.metadata" /> Include database and engine
                metadata</label
              >
              <p class="capability-note">
                Structure uses a schema object; data uses a records array per table. Content
                selection controls which are included.
              </p>
            </div>
          </section>
        </template>
        <section v-if="c.mode === 'custom'" class="export-section">
          <h3>Output</h3>
          <div class="export-body">
            <div class="export-line">
              <span class="export-label">Destination:</span
              ><strong>Local file <span class="muted">(simulated)</span></strong>
            </div>
            <div class="export-line">
              <label class="filename-label"
                >Filename template:<input
                  v-model="c.filename"
                  aria-label="Export filename"
                  spellcheck="false" /></label
              ><span class="file-extension"
                >.{{ c.format.toLowerCase()
                }}{{
                  c.compression === 'gzip' ? '.gz' : c.compression === 'zip' ? '.zip' : ''
                }}</span
              >
            </div>
            <p class="capability-note">
              Use @DATABASE@, @TABLE@, and @DATE@. No path or file is written.
            </p>
            <div class="export-line">
              <span class="export-label">Compression:</span
              ><label><input type="radio" v-model="c.compression" value="none" /> None</label
              ><label><input type="radio" v-model="c.compression" value="zip" /> ZIP</label
              ><label><input type="radio" v-model="c.compression" value="gzip" /> GZIP</label
              ><span class="muted">Simulation only</span>
            </div>
            <div class="export-preview">
              <span>Preview:</span><code>{{ summary.filename }}</code
              ><span
                >{{ summary.tables }} tables · {{ summary.structures }} structures ·
                {{ summary.rows.toLocaleString() }} rows</span
              >
            </div>
          </div>
        </section>
        <details v-if="c.mode === 'custom'" class="simulation-tools">
          <summary>Prototype testing controls</summary>
          <label class="export-choice"
            ><input type="checkbox" v-model="simulateFailure" /> Simulate an export failure
            (destination unavailable)</label
          >
        </details>
      </fieldset>
      <div v-if="errors.length" class="notice error export-errors" role="alert">
        <AlertCircle :size="15" />
        <ul>
          <li v-for="e in errors" :key="e">{{ e }}</li>
        </ul>
      </div>
      <div v-if="c.mode === 'quick'" class="quick-export-summary muted">
        Structure and Data · {{ summary.tables }} table(s) · {{ summary.rows }} rows ·
        {{ summary.filename }}
      </div>
      <div v-if="c.mode === 'quick' && c.format === 'CSV'" class="notice info">
        <span>CSV contains data only. Quick preserves Structure and Data.</span
        ><button
          class="button"
          type="button"
          @click="
            c.mode = 'custom';
            c.content = 'data';
          "
        >
          Use Custom CSV export
        </button>
      </div>
      <div class="export-submit">
        <span class="muted"
          >{{ c.mode === 'quick' ? 'Quick export' : 'Custom export' }} · {{ c.format }} ·
          {{ summary.tables }} table(s)</span
        ><button
          class="button primary"
          type="submit"
          :disabled="!!errors.length || state === 'running'"
        >
          <LoaderCircle v-if="state === 'running'" class="spin" :size="13" /><Download
            v-else
            :size="13"
          />{{ state === 'running' ? 'Simulating…' : 'Simulate export' }}
        </button>
      </div>
    </form>
    <section v-if="state !== 'idle'" class="export-job" role="status" aria-live="polite">
      <template v-if="state === 'running'"
        ><h3>
          <LoaderCircle :size="15" class="spin" />{{
            progress < 30
              ? 'Preparing mock export…'
              : progress < 70
                ? 'Reading mock structures and records…'
                : 'Finalizing simulated output…'
          }}<span>{{ progress }}%</span>
        </h3>
        <div
          class="progress-track"
          role="progressbar"
          :aria-valuenow="progress"
          aria-valuemin="0"
          aria-valuemax="100"
        >
          <div :style="{ width: progress + '%' }"></div>
        </div>
        <button type="button" class="button" @click="controller?.abort()">
          <X :size="13" /> Cancel simulation
        </button></template
      ><template v-else-if="state === 'success' && completed"
        ><h3 class="accent-text"><CheckCircle2 :size="16" /> Export simulation completed</h3>
        <dl class="export-summary">
          <dt>Proposed filename</dt>
          <dd class="mono">{{ completed.filename }}</dd>
          <dt>Engine / format</dt>
          <dd>{{ completed.engine }} / {{ completed.format }}</dd>
          <dt>Tables / structures / rows</dt>
          <dd>
            {{ completed.tables }} / {{ completed.structures }} /
            {{ completed.rows.toLocaleString() }}
          </dd>
          <dt>Compression</dt>
          <dd>
            {{ completed.compression === 'none' ? 'None' : completed.compression.toUpperCase() }}
            (not performed)
          </dd>
          <dt>Estimated uncompressed size</dt>
          <dd>~{{ (completed.estimatedBytes / 1024).toFixed(1) }} KiB · estimate only</dd>
          <dt>Additional objects</dt>
          <dd>{{completed.objects.views}} views · {{completed.objects.triggers}} triggers · {{completed.objects.routines}} routines · {{completed.objects.events}} events (simulation)</dd>
        </dl>
        <p class="simulation-reminder">
          No backup was created. No data was dumped, compressed, downloaded, or written to disk.
        </p></template
      ><template v-else-if="state === 'cancelled'"
        ><h3><X :size="15" /> Export simulation cancelled</h3>
        <p>
          The job stopped. Your export preferences are preserved. No backup or partial file was
          created.
        </p>
        <button class="button" type="button" @click="run">Run again</button></template
      ><template v-else-if="state === 'error'"
        ><h3 class="danger-text"><AlertCircle :size="15" /> Export simulation failed</h3>
        <p>{{ error }}</p>
        <p class="muted">
          No backup or partial file was created. Disable the failure control to simulate success.
        </p>
        <button class="button" type="button" @click="run">Retry simulation</button></template
      >
    </section>
  </div>
</template>
