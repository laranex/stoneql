<script setup lang="ts">
import type { Column } from '../types';
import SchemaIcon from './SchemaIcon.vue';
defineProps<{ column: Column }>();
</script>
<template>
  <span
    v-if="column.primary || column.unique || column.indexKinds?.includes('unique') || column.foreign || column.indexed"
    class="column-keys"
    ><SchemaIcon v-if="column.primary" kind="primary" :detail="column.name" /><SchemaIcon
      v-if="(column.unique || column.indexKinds?.includes('unique')) && !column.primary"
      kind="unique"
      :detail="column.name" /><SchemaIcon
      v-if="column.foreign"
      kind="foreign"
      :detail="`${column.name} → ${column.foreign}`" /><SchemaIcon
      v-if="column.indexed && !column.primary && !column.unique && !column.indexKinds?.includes('unique')"
      kind="index"
      :detail="column.name"
  /></span>
</template>
