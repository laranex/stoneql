<script setup lang="ts">
import { computed } from 'vue';
import { highlightTree, classHighlighter } from '@lezer/highlight';
import { sqlDialects } from '../sql/dialects';
import type { Engine } from '../types';
const props = defineProps<{ text: string; engine: Engine }>();
const parts = computed(() => {
  const result: { text: string; className: string }[] = [];
  let from = 0;
  highlightTree(
    sqlDialects[props.engine].language.parser.parse(props.text),
    classHighlighter,
    (start, end, className) => {
      if (start > from) result.push({ text: props.text.slice(from, start), className: '' });
      result.push({ text: props.text.slice(start, end), className });
      from = end;
    },
  );
  if (from < props.text.length) result.push({ text: props.text.slice(from), className: '' });
  return result;
});
</script>
<template>
  <span v-for="(part, index) in parts" :key="index" :class="part.className">{{ part.text }}</span>
</template>
