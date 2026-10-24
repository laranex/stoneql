<script setup lang="ts">
import type { Column } from '../types';
import ColumnKeys from './ColumnKeys.vue';
defineProps<{ column: Column }>();
</script>
<template>
  <span class="schema-name"
    ><ColumnKeys
      v-if="column.primary || column.unique || column.indexKinds?.includes('unique') || column.foreign || column.indexed"
      :column="column"
    /><span class="schema-name-text" :title="column.name">{{ column.name }}</span></span
  >
</template>
