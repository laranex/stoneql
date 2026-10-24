<script setup lang="ts">
import { computed } from 'vue';
import type { Column, Engine } from '../types';
import { choiceDefinition } from '../services/valueValidation';
const props = defineProps<{ column: Column; engine: Engine; value: string | number }>();
const definition = computed(() => choiceDefinition(props.column, props.engine));
const values = computed(() =>
  definition.value?.kind === 'set' && String(props.value) !== ''
    ? String(props.value).split(',')
    : [String(props.value)],
);
function tone(value: string) {
  const v = value.toLowerCase();
  return ['active', 'enabled'].includes(v)
    ? 'success'
    : ['suspended'].includes(v)
      ? 'warning'
      : ['inactive', 'disabled'].includes(v)
        ? 'inactive'
        : 'neutral';
}
</script>
<template>
  <span class="enum-values" :title="String(value)" :data-kind="definition?.kind"
    ><span v-for="(item, index) in values" :key="index" class="enum-value" :class="tone(item)">{{
      item
    }}</span></span
  >
</template>
