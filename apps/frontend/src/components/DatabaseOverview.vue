<script setup lang="ts">
import type { Database, View, Table } from '../types';
import { Database as DatabaseIcon, Table2, Eye, Plus } from 'lucide-vue-next';
import { relations } from '../services/objectCapabilities';
defineProps<{
  databases: Database[];
  database?: Database;
  server: boolean;
}>();
const emit = defineEmits<{
  database: [
    name: string
  ];
  table: [
    database: string,
    table: string,
    view: View
  ];
  createTable: [
  ];
  createView: [
  ];
  definition: [
    view: Table
  ];
  dropView: [
    view: Table
  ];
}>();
</script>
<template>
 <div v-if="!server" class="workspace-toolbar"><span>{{database?.tables.length||0}} tables · {{database?.views?.length||0}} views</span><div class="row-actions"><button class="button" @click="emit('createTable')"><Plus :size="13" /> Create table</button><button class="button" @click="emit('createView')"><Plus :size="13" /> Create view</button></div></div>
 <div class="data-grid-wrap"><table class="data-grid overview-grid"><thead><tr><th>{{server?'Database':'Name'}}</th><th v-if="!server">Type</th><th>{{server?'Tables':'Rows (mock)'}}</th><th>Actions</th></tr></thead>
  <tbody v-if="server"><tr v-for="db in databases" :key="db.name"><td><button class="schema-name" @click="emit('database',db.name)"><DatabaseIcon :size="14" />{{db.name}}</button></td><td>{{db.tables.length}}</td><td><button @click="emit('database',db.name)">Structure</button></td></tr></tbody>
  <tbody v-else><tr v-for="table in relations(database)" :key="table.name"><td><button class="schema-name" @click="emit('table',database!.name,table.name,'Browse')"><Eye v-if="table.kind && table.kind!=='table'" :size="14" /><Table2 v-else :size="14" />{{table.name}}</button></td><td>{{table.kind==='materialized-view'?'Materialized view':table.kind==='view'?'View':'Table'}}</td><td>{{table.rows.length}}</td><td class="row-actions"><template v-if="!table.kind || table.kind==='table'"><button v-for="view in (['Browse','Structure','Search','Insert','Operations'] as const)" :key="view" @click="emit('table',database!.name,table.name,view)">{{view}}</button></template><template v-else><button @click="emit('table',database!.name,table.name,'Browse')">Browse</button><button @click="emit('definition',table)">Definition</button><button class="danger-text" @click="emit('dropView',table)">Drop</button></template></td></tr>
   <tr v-if="!relations(database).length"><td colspan="4">No tables or views in this mock database.</td></tr>
  </tbody></table></div>
</template>
