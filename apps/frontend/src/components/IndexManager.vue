<script setup lang="ts">
import { ref, computed } from 'vue';
import { Plus, X, ArrowUp, ArrowDown } from 'lucide-vue-next';
import { useWorkspace } from '../stores/workspace';
import { tableIndexes } from '../services/indexes';
import { objectCapabilities } from '../services/objectCapabilities';
import { dialog as vDialog } from '../directives/dialog';
import SchemaIcon from './SchemaIcon.vue';
import type { TableIndex } from '../types/objects';
const props = defineProps<{
  confirmDiscard: (message: string) => Promise<boolean>;
}>();
const emit = defineEmits<{
  confirm: [
    request: {
      title: string;
      message: string;
      acceptLabel: string;
      action: () => void;
    }
  ];
}>();
const store = useWorkspace(), open = ref(false), original = ref(''), initial = ref(''), error = ref('');
const draft = ref<TableIndex>({ name: '', kind: 'normal', method: 'btree', columns: [] });
const indexes = computed(() => store.table ? tableIndexes(store.table) : []), cap = computed(() => objectCapabilities[store.connection.engine]);
function edit(index?: TableIndex) {
  original.value = index?.name || '';
  draft.value = index ? JSON.parse(JSON.stringify(index)) : { name: '', kind: 'normal', method: 'btree', columns: [{ column: store.table?.columns[0]?.name || '', order: 'ASC' }] };
  initial.value = JSON.stringify(draft.value);
  error.value = '';
  open.value = true;
}
async function close() { if (JSON.stringify(draft.value) !== initial.value && !(await props.confirmDiscard('Discard index changes?')))
  return; open.value = false; }
function normalizeDirection() {
  if (draft.value.method !== 'btree' || ['fulltext', 'spatial'].includes(draft.value.kind))
    draft.value.columns.forEach(part => { part.order = 'ASC'; });
}
function changeKind() {
  if (draft.value.kind === 'primary' && ['MySQL', 'MariaDB'].includes(store.connection.engine))
    draft.value.name = 'PRIMARY';
  draft.value.method = draft.value.kind === 'spatial' ? 'rtree' : draft.value.kind === 'fulltext' ? 'fulltext' : 'btree';
  normalizeDirection();
}
function move(index: number, direction: number) { const items = draft.value.columns, other = items[index + direction]; if (!other)
  return; [items[index], items[index + direction]] = [other, items[index]!]; }
function save() { try {
  store.saveTableIndex(draft.value, original.value);
  open.value = false;
}
catch (e) {
  error.value = (e as Error).message;
} }
function drop(index: TableIndex) { emit('confirm', { title: `Drop index ${index.name}?`, message: 'Remove this index from the mock schema. Referenced/generated-key requirements are checked before saving.', acceptLabel: 'Drop index', action: () => store.dropTableIndex(index.name) }); }
</script>
<template>
 <section class="panel index-manager" aria-label="Table indexes">
  <div class="workspace-toolbar"><h3><SchemaIcon kind="index" /> Indexes</h3><button class="button" @click="edit()"><Plus :size="13" /> Add index</button></div>
  <div class="data-grid-wrap"><table class="data-grid"><thead><tr><th>Name</th><th>Type</th><th>Method</th><th>Columns (in key order)</th><th>Actions</th></tr></thead><tbody>
   <tr v-for="index in indexes" :key="index.name"><td><span class="schema-name"><SchemaIcon :kind="index.kind==='primary'?'primary':index.kind==='unique'?'unique':'index'" :detail="index.name" :label="index.kind==='fulltext'?`Fulltext index: ${index.name}`:index.kind==='spatial'?`Spatial index: ${index.name}`:undefined" />{{index.name}}</span></td><td>{{index.kind.toUpperCase()}}</td><td>{{index.kind==='fulltext'?'Engine full-text':index.method.toUpperCase()}}</td><td>{{index.columns.map(p=>`${p.column} ${p.order}`).join(', ')}}</td><td class="row-actions"><button :aria-label="`Edit index ${index.name}`" @click="edit(index)">Edit</button><button class="danger-text" :aria-label="`Drop index ${index.name}`" @click="drop(index)">Drop</button></td></tr>
   <tr v-if="!indexes.length"><td colspan="5">No indexes defined.</td></tr>
  </tbody></table></div>
  <div v-if="open" class="modal-overlay"><section class="modal column-dialog" v-dialog="{close}" role="dialog" aria-modal="true" :aria-label="original?'Edit index':'Add index'">
   <header><h2>{{original?'Edit index':'Add index'}}</h2><button class="icon-button" aria-label="Close index dialog" @click="close"><X :size="16" /></button></header>
   <form @submit.prevent="save"><div class="modal-body">
    <div class="notice info">Mock schema only. No real index is built. Engine/operator-class compatibility remains backend-authoritative.</div>
    <div class="form-two"><label class="form-label">Name<input v-model="draft.name" aria-label="Index name" :disabled="draft.kind==='primary' && ['MySQL','MariaDB'].includes(store.connection.engine)" required /></label><label class="form-label">Type<select v-model="draft.kind" aria-label="Index type" @change="changeKind"><option v-for="kind in cap.indexKinds" :key="kind" :value="kind">{{kind.toUpperCase()}}</option></select></label></div>
    <label class="form-label">Method<select v-model="draft.method" aria-label="Index method" @change="normalizeDirection" :disabled="['spatial','fulltext'].includes(draft.kind)"><option v-for="method in draft.kind==='spatial'?['rtree']:draft.kind==='fulltext'?['fulltext']:cap.methods" :key="method" :value="method">{{draft.kind==='fulltext'?'Engine full-text':method.toUpperCase()}}</option></select></label>
    <table class="data-grid form-grid"><thead><tr><th>Column</th><th>Order</th><th>Position</th></tr></thead><tbody><tr v-for="(part,i) in draft.columns" :key="i"><td><select v-model="part.column" :aria-label="`Index column ${i+1}`"><option v-for="column in store.table?.columns" :key="column.name" :value="column.name">{{column.name}}</option></select></td><td><select v-model="part.order" :disabled="draft.method!=='btree' || ['fulltext','spatial'].includes(draft.kind)" :aria-label="`Index order ${i+1}`"><option>ASC</option><option>DESC</option></select></td><td class="row-actions"><button type="button" :disabled="i===0" :aria-label="`Move index column ${i+1} up`" @click="move(i,-1)"><ArrowUp :size="14" /></button><button type="button" :disabled="i===draft.columns.length-1" :aria-label="`Move index column ${i+1} down`" @click="move(i,1)"><ArrowDown :size="14" /></button><button type="button" :aria-label="`Remove index column ${i+1}`" @click="draft.columns.splice(i,1)"><X :size="14" /></button></td></tr></tbody></table>
    <button type="button" class="button" @click="draft.columns.push({column:store.table?.columns.find(c=>!draft.columns.some(p=>p.column===c.name))?.name||'',order:'ASC'})">Add key column</button>
    <div v-if="error" class="notice error" role="alert">{{error}}</div>
   </div><footer><button type="button" class="button" @click="close">Cancel</button><button class="button primary">Save index</button></footer></form>
  </section></div>
 </section>
</template>
