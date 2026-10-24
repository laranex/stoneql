<script setup lang="ts">
import { computed } from 'vue';
import { KeyRound, Link2, ListTree } from 'lucide-vue-next';
const props = defineProps<{ kind: 'primary' | 'unique' | 'foreign' | 'index'; detail?: string; label?: string }>();
const labels = {
  primary: 'Primary key',
  unique: 'Unique key',
  foreign: 'Foreign key',
  index: 'Normal index',
};
const description = computed(() => props.label || labels[props.kind] + (props.detail ? `: ${props.detail}` : ''));
</script>
<template>
  <span
    class="schema-icon"
    :class="`schema-icon-${kind}`"
    role="img"
    :title="description"
    :aria-label="description"
    ><KeyRound v-if="kind === 'primary' || kind === 'unique'" :size="14" aria-hidden="true" /><Link2
      v-else-if="kind === 'foreign'"
      :size="14"
      aria-hidden="true" /><ListTree v-else :size="14" aria-hidden="true"
  /></span>
</template>
