<script setup lang="ts">
import { computed, ref } from 'vue';
import { X, AlertTriangle } from 'lucide-vue-next';
import type { Engine } from '../types';
import {
  addColumnSql, editColumnSql, createDatabaseSql, columnNameProblem, definitionProblem, charsetProblem,
  pgAddColumnSql, pgEditColumnSql, addIndexSql, type ColumnPosition, type PgColumnState,
} from '../services/liveDdl';
/**
 * Live schema change dialog (MySQL/MariaDB, read-write): add or change a column, or create a
 * database. The exact statement that Apply runs is previewed; nothing is regenerated after.
 */
const props = defineProps<{
  mode: 'add-column' | 'edit-column' | 'create-database' | 'add-index';
  engine: Engine;
  table?: string;
  columns?: string[];
  column?: string;
  definition?: string;
  /** PostgreSQL: the column as the catalog reports it (edit) — only changed parts are altered. */
  pg?: PgColumnState;
  busy?: boolean;
  error?: string;
}>();
const emit = defineEmits<{ close: []; apply: [sql: string, destructive: boolean] }>();
const name = ref(props.mode === 'edit-column' ? props.column ?? '' : '');
const definition = ref(props.mode === 'edit-column' ? props.definition ?? '' : 'VARCHAR(255) NULL');
const at = ref<string>('last');
const charset = ref(''), collation = ref('');
const postgres = computed(() => props.engine === 'PostgreSQL');
const pgType = ref(props.pg?.type ?? 'text'), pgNullable = ref(props.pg?.nullable ?? true), pgDefault = ref(props.pg?.default ?? ''), pgUsing = ref('');
/** Index columns in the order they were picked. */
const indexColumns = ref<string[]>([]), unique = ref(false);
function toggleIndexColumn(column: string, on: boolean) {
  indexColumns.value = on ? [...indexColumns.value.filter((c) => c !== column), column] : indexColumns.value.filter((c) => c !== column);
}
const optional = (text: string) => (text.trim() ? definitionProblem(text) : '');
const position = computed<ColumnPosition>(() => (at.value === 'last' ? { kind: 'last' } : at.value === 'first' ? { kind: 'first' } : { kind: 'after', column: at.value.slice(6) }));
const problem = computed(() => {
  if (props.mode === 'create-database') {
    if (!name.value.trim()) return 'Enter a database name.';
    if (name.value.length > 64 || /[\u0000-\u001f/\\.]/.test(name.value) || name.value !== name.value.trim()) return 'Database names are at most 64 characters, without /, \\, . or control characters.';
    return charsetProblem(charset.value) || charsetProblem(collation.value);
  }
  if (props.mode === 'add-index') {
    if (!name.value.trim()) return 'Enter an index name.';
    if (name.value.length > 63 || /[\u0000-\u001f]/.test(name.value)) return 'Index names are at most 63 characters, without control characters.';
    return indexColumns.value.length ? '' : 'Choose at least one column.';
  }
  if (postgres.value) return columnNameProblem(name.value) || definitionProblem(pgType.value) || optional(pgDefault.value) || optional(pgUsing.value);
  return columnNameProblem(name.value) || definitionProblem(definition.value);
});
const sql = computed(() => {
  if (problem.value) return '';
  if (props.mode === 'create-database') return createDatabaseSql(props.engine, name.value, charset.value, collation.value);
  if (props.mode === 'add-index') return addIndexSql(props.engine, props.table!, name.value, indexColumns.value, unique.value);
  const state = { type: pgType.value, nullable: pgNullable.value, default: pgDefault.value };
  if (postgres.value && props.mode === 'add-column') return pgAddColumnSql(props.table!, name.value, state);
  if (postgres.value) return pgEditColumnSql(props.table!, props.column!, name.value, props.pg!, { ...state, using: pgUsing.value });
  if (props.mode === 'add-column') return addColumnSql(props.engine, props.table!, name.value, definition.value, position.value);
  return editColumnSql(props.engine, props.table!, props.column!, name.value, props.definition ?? '', definition.value);
});
/** A changed definition can convert or truncate stored values. */
const destructive = computed(() => props.mode === 'edit-column' && (postgres.value ? pgType.value.trim() !== (props.pg?.type ?? '').trim() : definition.value.trim() !== (props.definition ?? '').trim()));
const title = computed(() => (props.mode === 'create-database' ? 'Create database' : props.mode === 'add-index' ? `Add index to ${props.table}` : props.mode === 'add-column' ? `Add column to ${props.table}` : `Change column ${props.column}`));
</script>
<template>
  <div class="modal-overlay" @mousedown.self="!busy && emit('close')">
    <section class="modal" role="dialog" aria-modal="true" :aria-label="title" data-testid="live-ddl-dialog">
      <header>
        <h2>{{ title }}</h2>
        <button class="icon-button" type="button" aria-label="Close" :disabled="busy" @click="emit('close')"><X :size="17" /></button>
      </header>
      <form class="modal-body" @submit.prevent="sql && emit('apply', sql, destructive)">
        <label class="form-label">{{ mode === 'create-database' ? 'Database name' : mode === 'add-index' ? 'Index name' : 'Column name' }}<input v-model="name" :disabled="busy" autocapitalize="off" autocorrect="off" spellcheck="false" autocomplete="off" /></label>
        <template v-if="mode === 'create-database'">
          <div class="form-two">
            <label class="form-label">Character set (optional)<input v-model="charset" :disabled="busy" placeholder="server default, e.g. utf8mb4" autocapitalize="off" spellcheck="false" /></label>
            <label class="form-label">Collation (optional)<input v-model="collation" :disabled="busy" placeholder="server default" autocapitalize="off" spellcheck="false" /></label>
          </div>
        </template>
        <template v-else-if="mode === 'add-index'">
          <label class="compact-choice"><input v-model="unique" type="checkbox" :disabled="busy" /> Unique</label>
          <div class="form-label">Columns, in index order
            <div class="index-columns" role="group" aria-label="Index columns">
              <label v-for="c in columns" :key="c" class="compact-choice"><input type="checkbox" :disabled="busy" :checked="indexColumns.includes(c)" @change="toggleIndexColumn(c, ($event.target as HTMLInputElement).checked)" /> {{ c }}<span v-if="indexColumns.includes(c)" class="muted"> ({{ indexColumns.indexOf(c) + 1 }})</span></label>
            </div>
          </div>
        </template>
        <template v-else-if="postgres">
          <label class="form-label">Type<input v-model="pgType" :disabled="busy" class="mono" aria-label="Column type" spellcheck="false" autocapitalize="off" /></label>
          <label class="compact-choice"><input v-model="pgNullable" type="checkbox" :disabled="busy" /> NULL allowed</label>
          <label class="form-label">Default expression<input v-model="pgDefault" :disabled="busy" class="mono" aria-label="Default expression" spellcheck="false" autocapitalize="off" placeholder="none" /><span class="muted">An SQL expression, for example 0, 'text' or now(). Empty means no default.</span></label>
          <label v-if="mode === 'edit-column'" class="form-label">USING (optional)<input v-model="pgUsing" :disabled="busy" class="mono" aria-label="USING expression" spellcheck="false" autocapitalize="off" placeholder="for a type change the server cannot cast, e.g. col::integer" /></label>
          <span class="muted">Only what you change is altered; comments, collation, identity and constraints stay as they are.</span>
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
.index-columns { display: flex; flex-wrap: wrap; gap: 6px 14px; margin-top: 4px; }
.ddl-preview { margin: 4px 0 0; padding: 8px 10px; white-space: pre-wrap; word-break: break-word; font-family: var(--font-mono, ui-monospace, monospace); font-size: 13px; background: var(--surface-muted, var(--panel)); border: 1px solid var(--border); border-radius: 6px; }
textarea.mono { font-family: var(--font-mono, ui-monospace, monospace); }
</style>
