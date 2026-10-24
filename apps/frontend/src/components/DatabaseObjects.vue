<script setup lang="ts">
import { computed, ref } from 'vue';
import { Plus, X, ArrowUp, ArrowDown } from 'lucide-vue-next';
import { useWorkspace } from '../stores/workspace';
import { objectCapabilities, relations } from '../services/objectCapabilities';
import { simulateRoutine } from '../services/databaseObjects';
import { dialog as vDialog } from '../directives/dialog';
import SqlText from './SqlText.vue';
import type { Table } from '../types';
import type { ObjectSection, Routine, RoutineParameter, Trigger, ScheduledEvent } from '../types/objects';
type Kind = 'table' | 'view' | 'materialized-view' | 'procedure' | 'function' | 'trigger' | 'event';
const props = defineProps<{
  section?: ObjectSection;
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
  changed: [
  ];
}>();
const store = useWorkspace(), cap = computed(() => objectCapabilities[store.connection.engine]);
const items = computed(() => props.section === 'Routines' ? store.database?.routines || [] : props.section === 'Triggers' ? store.database?.triggers || [] : store.database?.events || []);
const open = ref(false), original = ref(''), initial = ref(''), error = ref(''), notice = ref(''), mode = ref<'edit' | 'inspect' | 'execute'>('edit');
const draft = ref({ name: '', kind: 'view' as Kind, definition: '', returns: '', language: 'SQL', parameters: [] as RoutineParameter[], table: '', timing: 'BEFORE' as Trigger['timing'], event: 'INSERT' as Trigger['event'], schedule: 'EVERY 1 DAY', enabled: false });
const argumentsDraft = ref<Record<string, string>>({}), nulls = ref<Record<string, boolean>>({});
const isView = computed(() => ['view', 'materialized-view'].includes(draft.value.kind)), isRoutine = computed(() => ['procedure', 'function'].includes(draft.value.kind));
function launch(kind: Kind, item?: Table | Routine | Trigger | ScheduledEvent, display: 'edit' | 'inspect' | 'execute' = 'edit') {
  original.value = item?.name || '';
  draft.value = { name: '', kind, definition: '', returns: '', language: store.connection.engine === 'PostgreSQL' ? 'sql' : 'SQL', parameters: [], table: store.database?.tables[0]?.name || '', timing: 'BEFORE', event: 'INSERT', schedule: 'EVERY 1 DAY', enabled: false, ...JSON.parse(JSON.stringify(item || {})) };
  mode.value = display;
  initial.value = JSON.stringify(draft.value);
  error.value = '';
  notice.value = '';
  argumentsDraft.value = {};
  nulls.value = {};
  open.value = true;
}
function openView(view?: Table) { launch(view?.kind === 'materialized-view' ? 'materialized-view' : 'view', view, view ? 'inspect' : 'edit'); }
function itemKind(item: any): Kind { return props.section === 'Routines' ? item.kind : props.section === 'Triggers' ? 'trigger' : 'event'; }
async function close() { if (mode.value === 'edit' && JSON.stringify(draft.value) !== initial.value && !(await props.confirmDiscard('Discard database object changes?')))
  return; open.value = false; }
function save() {
  try {
    const value = JSON.parse(JSON.stringify(draft.value));
    if (value.kind === 'table')
      store.createTable(value.name);
    else if (isView.value)
      store.saveView({ name: value.name, definition: value.definition, kind: value.kind }, original.value);
    else if (isRoutine.value)
      store.saveRoutine(value, original.value);
    else if (value.kind === 'trigger')
      store.saveTrigger(value, original.value);
    else
      store.saveEvent(value, original.value);
    open.value = false;
    notice.value = 'Mock definition saved. No SQL was executed.';
    emit('changed');
  }
  catch (e) {
    error.value = (e as Error).message;
  }
}
function drop(kind: Kind, name: string) {
  emit('confirm', { title: `Drop ${kind} ${name}?`, message: 'Remove this object from the session mock schema. No database command will be executed.', acceptLabel: `Drop ${kind}`, action: () => {
      if (['view', 'materialized-view'].includes(kind))
        store.dropView(name);
      else
        store.dropObject(['procedure', 'function'].includes(kind) ? 'routines' : kind === 'trigger' ? 'triggers' : 'events', name);
      open.value = false;
      notice.value = 'Mock object removed.';
      emit('changed');
    } });
}
function call() {
  notice.value = '';
  error.value = '';
  try {
    const routine = store.database?.routines?.find(r => r.name === original.value);
    if (!routine)
      throw new Error('Routine no longer exists.');
    notice.value = simulateRoutine(routine, Object.fromEntries(routine.parameters.filter(p => p.mode !== 'OUT').map(p => [p.name, nulls.value[p.name] ? null : argumentsDraft.value[p.name] ?? ''])), store.connection.engine);
    error.value = '';
  }
  catch (e) {
    error.value = (e as Error).message;
  }
}
function moveParameter(index: number, delta: number) { const params = draft.value.parameters, next = params[index + delta]; if (next)
  [params[index], params[index + delta]] = [next, params[index]!]; }
defineExpose({ openTable: () => launch('table'), openView, dropView: (view: Table) => drop(view.kind === 'materialized-view' ? 'materialized-view' : 'view', view.name) });
</script>
<template>
 <div :class="section?'content-area object-content':'object-dialog-host'">
  <template v-if="section">
   <div class="workspace-toolbar"><span>{{items.length}} {{section.toLowerCase()}}</span><button class="button" @click="launch(section==='Routines'?'procedure':section==='Triggers'?'trigger':'event')"><Plus :size="13" /> Add {{section==='Routines'?'routine':section==='Triggers'?'trigger':'event'}}</button></div>
   <div class="notice info">Session mock definitions only. Routines are not executed, triggers do not fire, and events are not scheduled.</div>
   <div class="data-grid-wrap"><table class="data-grid"><thead><tr><th>Name</th><th>{{section==='Routines'?'Type':section==='Triggers'?'Table / event':'Schedule / status'}}</th><th>Actions</th></tr></thead><tbody>
    <tr v-for="item in items" :key="item.name"><td><button @click="launch(itemKind(item),item,'inspect')">{{item.name}}</button></td><td>{{section==='Routines'?(item as Routine).kind:section==='Triggers'?`${(item as Trigger).table} · ${(item as Trigger).timing} ${(item as Trigger).event}`:`${(item as ScheduledEvent).schedule} · ${(item as ScheduledEvent).enabled?'Enabled (mock)':'Disabled'}`}}</td><td class="row-actions"><button :aria-label="`Edit ${item.name}`" @click="launch(itemKind(item),item)">Edit</button><button v-if="section==='Routines'" :aria-label="`Execute ${item.name}`" @click="launch(itemKind(item),item,'execute')">Execute (mock)</button><button class="danger-text" :aria-label="`Drop ${item.name}`" @click="drop(itemKind(item),item.name)">Drop</button></td></tr>
    <tr v-if="!items.length"><td colspan="3">No {{section.toLowerCase()}} defined.</td></tr>
   </tbody></table></div>
  </template>
  <div v-if="notice && !open && section" class="notice info" role="status">{{notice}}</div>
  <div v-if="open" class="modal-overlay"><section class="modal object-dialog" role="dialog" aria-modal="true" :aria-label="`${mode==='execute'?'Execute':mode==='inspect'?'Definition':original?'Edit':'Create'} ${draft.kind}`" v-dialog="{close}">
   <header><h2>{{mode==='execute'?'Execute (mock)':mode==='inspect'?'Definition':original?'Edit':'Create'}} {{draft.kind}}</h2><button class="icon-button" aria-label="Close object dialog" @click="close"><X :size="16" /></button></header>
   <form @submit.prevent="mode==='execute'?call():save()"><div class="modal-body">
    <div class="notice info">Simulation only. SQL definitions are stored verbatim; syntax, privileges and server dependencies are not verified.</div>
    <template v-if="mode==='inspect'"><strong>{{draft.name}}</strong><pre class="object-definition"><SqlText :text="draft.definition" :engine="store.connection.engine" /></pre><p v-if="isView" class="muted">Browse shows a mock snapshot. Refresh explicitly to evaluate the supported SELECT subset. Changing a materialized-view definition clears its snapshot and indexes; a backend would need to recreate the object.</p></template>
    <template v-else-if="mode==='execute'">
     <p>{{draft.name}} · Body is not executed; no return or OUT values are fabricated.</p>
     <table v-if="draft.parameters.length" class="data-grid form-grid"><thead><tr><th>Parameter</th><th>Type / mode</th><th>NULL</th><th>Value</th></tr></thead><tbody><tr v-for="parameter in draft.parameters" :key="parameter.name"><td>{{parameter.name}}</td><td>{{parameter.type}} · {{parameter.mode}}</td><td><input v-if="parameter.mode!=='OUT'" type="checkbox" v-model="nulls[parameter.name]" :aria-label="`NULL parameter ${parameter.name}`" /></td><td><input v-if="parameter.mode!=='OUT'" v-model="argumentsDraft[parameter.name]" :aria-label="`Parameter ${parameter.name}`" :disabled="nulls[parameter.name]" /><span v-else>Not computed</span></td></tr></tbody></table>
    </template>
    <template v-else>
     <div class="form-two"><label class="form-label">Name<input v-model="draft.name" aria-label="Object name" required /></label>
      <label v-if="isRoutine" class="form-label">Type<select v-model="draft.kind" aria-label="Routine type"><option value="procedure">Procedure</option><option value="function">Function</option></select></label>
      <label v-else-if="isView" class="form-label">Type<select v-model="draft.kind" aria-label="View type" :disabled="!!original"><option value="view">View</option><option v-if="cap.materializedViews" value="materialized-view">Materialized view</option></select></label>
     </div>
     <template v-if="isRoutine">
      <div class="form-two"><label class="form-label">Language<select v-model="draft.language" aria-label="Routine language"><option v-for="language in store.connection.engine==='PostgreSQL'?['sql','plpgsql']:['SQL']" :key="language">{{language}}</option></select></label><label v-if="draft.kind==='function'" class="form-label">Returns<input v-model="draft.returns" aria-label="Return type" required /></label></div>
      <table class="data-grid form-grid"><thead><tr><th>Parameter</th><th>Type</th><th>Mode</th><th>Order</th></tr></thead><tbody><tr v-for="(parameter,i) in draft.parameters" :key="i"><td><input v-model="parameter.name" :aria-label="`Parameter name ${i+1}`" /></td><td><input v-model="parameter.type" :aria-label="`Parameter type ${i+1}`" /></td><td><select v-model="parameter.mode" :aria-label="`Parameter mode ${i+1}`"><option>IN</option><option v-if="draft.kind==='procedure'||store.connection.engine==='PostgreSQL'">OUT</option><option v-if="draft.kind==='procedure'||store.connection.engine==='PostgreSQL'">INOUT</option></select></td><td class="row-actions"><button type="button" :disabled="i===0" :aria-label="`Move parameter ${i+1} up`" @click="moveParameter(i,-1)"><ArrowUp :size="13" /></button><button type="button" :disabled="i===draft.parameters.length-1" :aria-label="`Move parameter ${i+1} down`" @click="moveParameter(i,1)"><ArrowDown :size="13" /></button><button type="button" :aria-label="`Remove parameter ${i+1}`" @click="draft.parameters.splice(i,1)"><X :size="13" /></button></td></tr></tbody></table>
      <button type="button" class="button" @click="draft.parameters.push({name:'',type:'INT',mode:'IN'})">Add parameter</button>
     </template>
     <template v-if="draft.kind==='trigger'"><div class="form-two"><label class="form-label">Table / view<select v-model="draft.table" aria-label="Trigger table"><option v-for="target in relations(store.database)" :key="target.name" :value="target.name">{{target.name}}</option></select></label><label class="form-label">Timing<select v-model="draft.timing" aria-label="Trigger timing"><option v-for="timing in cap.triggerTimings" :key="timing">{{timing}}</option></select></label></div><label class="form-label">Event<select v-model="draft.event" aria-label="Trigger event"><option>INSERT</option><option>UPDATE</option><option>DELETE</option></select></label><p v-if="store.connection.engine==='PostgreSQL'" class="muted">Enter an EXECUTE FUNCTION clause referencing a trigger-returning function. Server validation is not simulated.</p></template>
     <template v-if="draft.kind==='event'"><label class="form-label">Schedule<input v-model="draft.schedule" aria-label="Event schedule" placeholder="EVERY 1 DAY" required /></label><label class="checkbox-label"><input type="checkbox" v-model="draft.enabled" /> Enabled (metadata only; no scheduler)</label></template>
     <p v-if="draft.kind==='table'" class="notice info">Creates an empty mock table with a generated integer id primary key. Use the existing Structure → Add column dialog to define additional columns.</p><label v-else class="form-label">{{isView?'SELECT definition':draft.kind==='trigger'&&store.connection.engine==='PostgreSQL'?'Function invocation':'SQL body'}}<textarea class="mono object-sql-input" v-model="draft.definition" aria-label="SQL definition" rows="6" required spellcheck="false"></textarea></label>
    </template>
    <div v-if="error" class="notice error" role="alert">{{error}}</div><div v-if="notice && mode==='execute'" class="notice info" role="status">{{notice}}</div>
   </div><footer><button type="button" class="button" @click="close">{{mode==='inspect'?'Close':'Cancel'}}</button><button v-if="mode==='inspect'" type="button" class="button" @click="mode='edit'">Edit definition</button><button v-else class="button primary">{{mode==='execute'?'Simulate call':'Save definition'}}</button></footer></form>
  </section></div>
 </div>
</template>
