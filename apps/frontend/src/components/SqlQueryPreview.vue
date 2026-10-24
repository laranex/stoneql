<script setup lang="ts">
import { useId } from 'vue';
import { ChevronDown, ChevronRight, Pencil } from 'lucide-vue-next';
defineProps<{ expanded: boolean }>();
const emit = defineEmits<{ 'update:expanded': [value: boolean]; edit: [] }>();
const contentId = useId();
</script>
<template>
  <div class="query-strip sql-query-preview" :class="{ collapsed: !expanded }">
    <button
      type="button"
      class="preview-toggle"
      :aria-expanded="expanded"
      :aria-controls="contentId"
      :aria-label="expanded ? 'Collapse SQL query preview' : 'Expand SQL query preview'"
      :title="expanded ? 'Collapse SQL query preview' : 'Expand SQL query preview'"
      @click="emit('update:expanded', !expanded)"
    >
      <ChevronDown v-if="expanded" :size="14" /><ChevronRight v-else :size="14" />
    </button>
    <code :id="contentId" class="preview-sql"><slot /></code>
    <button v-if="expanded" type="button" class="preview-edit" @click="emit('edit')">
      <Pencil :size="12" /> Edit query
    </button>
  </div>
</template>
