<script setup lang="ts">
import { computed, ref } from 'vue';
import { X, AlertTriangle } from 'lucide-vue-next';
import type { Engine } from '../types';
import {
  addColumnSql, editColumnSql, createDatabaseSql, columnNameProblem, definitionProblem, charsetProblem,
  type ColumnPosition,
} from '../services/liveDdl';
/**
 * Live schema change dialog (MySQL/MariaDB, read-write): add or change a column, or create a
 * database. The exact statement that Apply runs is previewed; nothing is regenerated after.
 */
const props = defineProps<{
  mode: 'add-column' | 'edit-column' | 'create-database';
  engine: Engine;
  table?: string;
  columns?: string[];
  column?: string;
  definition?: string;
  busy?: boolean;
  error?: string;
}>();
const emit = defineEmits<{ close: []; apply: [sql: string, destructive: boolean] }>();
const name = ref(props.mode === 'edit-column' ? props.column ?? '' : '');
const definition = ref(props.mode === 'edit-column' ? props.definition ?? '' : 'VARCHAR(255) NULL');
const at = ref<string>('last');
const charset = ref(''), collation = ref('');
const position = computed<ColumnPosition>(() => (at.value === 'last' ? { kind: 'last' } : at.value === 'first' ? { kind: 'first' } : { kind: 'after', column: at.value.slice(6) }));
const problem = computed(() => {
  if (props.mode === 'create-database') {
    if (!name.value.trim()) return 'Enter a database name.';
    if (name.value.length > 64 || /[\u0000-\u001f/\\.]/.test(name.value) || name.value !== name.value.trim()) return 'Database names are at most 64 characters, without /, \\, . or control characters.';
    return charsetProblem(charset.value) || charsetProblem(collation.value);
  }
  return columnNameProblem(name.value) || definitionProblem(definition.value);
});
const sql = computed(() => {
  if (problem.value) return '';
  if (props.mode === 'create-database') return createDatabaseSql(props.engine, name.value, charset.value, collation.value);
  if (props.mode === 'add-column') return addColumnSql(props.engine, props.table!, name.value, definition.value, position.value);
  return editColumnSql(props.engine, props.table!, props.column!, name.value, props.definition ?? '', definition.value);
});
/** A changed definition can convert or truncate stored values. */
const destructive = computed(() => props.mode === 'edit-column' && definition.value.trim() !== (props.definition ?? '').trim());
const title = computed(() => (props.mode === 'create-database' ? 'Create database' : props.mode === 'add-column' ? `Add column to ${props.table}` : `Change column ${props.column}`));
</script>
<template>
  <div class="modal-overlay" @mousedown.self="!busy && emit('close')">
    <section class="modal" role="dialog" aria-modal="true" :aria-label="title" data-testid="live-ddl-dialog">
      <header>
        <h2>{{ title }}</h2>
        <button class="icon-button" type="button" aria-label="Close" :disabled="busy" @click="emit('close')"><X :size="17" /></button>
      </header>
      <form class="modal-body" @submit.prevent="sql && emit('apply', sql, destructive)">
        <label class="form-label">{{ mode === 'create-database' ? 'Database name' : 'Column name' }}<input v-model="name" :disabled="busy" autocapitalize="off" autocorrect="off" spellcheck="false" autocomplete="off" /></label>
        <template v-if="mode === 'create-database'">
          <div class="form-two">
            <label class="form-label">Character set (optional)<input v-model="charset" :disabled="busy" placeholder="server default, e.g. utf8mb4" autocapitalize="off" spellcheck="false" /></label>
            <label class="form-label">Collation (optional)<input v-model="collation" :disabled="busy" placeholder="server default" autocapitalize="off" spellcheck="false" /></label>
          </div>
        </template>
        <template v-else>
          <label class="form-label">Column definition
            <textarea v-model="definition" :disabled="busy" rows="3" class="mono" aria-label="Column definition" spellcheck="false"></textarea>
            <span class="muted">{{ mode === 'edit-column' ? 'The server\'s current definition: type, NULL, default, collation, comment and other attributes. Change only what you need.' : 'Type and attributes, for example INT UNSIGNED NOT NULL DEFAULT 0 or VARCHAR(100) NULL COMMENT \'note\'.' }}</span>
          </label>
          <label v-if="mode === 'add-column'" class="form-label">Position<select v-model="at" :disabled="busy" aria-label="Column position">
            <option value="last">At the end</option>
            <option value="first">First</option>
            <option v-for="c in columns" :key="c" :value="`after:${c}`">After {{ c }}</option>
          </select></label>
        </template>
        <div class="form-label">SQL to run
          <pre class="ddl-preview" data-testid="ddl-preview">{{ sql || (problem || 'Nothing has changed.') }}</pre>
        </div>
        <p v-if="destructive" class="notice warning"><AlertTriangle :size="14" /> Changing the definition can convert or truncate stored values. You will be asked to confirm.</p>
        <p v-if="error" class="notice error" role="alert">{{ error }}</p>
        <footer>
          <button class="button" type="button" :disabled="busy" @click="emit('close')">Cancel</button>
          <button class="button primary" type="submit" :disabled="busy || !sql">{{ busy ? 'Applying…' : 'Apply' }}</button>
        </footer>
      </form>
    </section>
  </div>
</template>
<style scoped>
.ddl-preview { margin: 4px 0 0; padding: 8px 10px; white-space: pre-wrap; word-break: break-word; font-family: var(--font-mono, ui-monospace, monospace); font-size: 13px; background: var(--surface-muted, var(--panel)); border: 1px solid var(--border); border-radius: 6px; }
textarea.mono { font-family: var(--font-mono, ui-monospace, monospace); }
</style>
