<script setup lang="ts">
import { onMounted, ref, watch } from 'vue';
import { useData } from 'vitepress';
const props = defineProps<{ code: string }>();
const { isDark } = useData();
const svg = ref('');
const failed = ref(false);
let generation = 0;
let mounted = false;
// A pinned, browser-only loader keeps static documentation builds offline-capable.
async function render() {
  if (!mounted) return;
  const current = ++generation;
  try {
    const url = 'https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs';
    const { default: mermaid } = await import(/* @vite-ignore */ url);
    if (current !== generation) return;
    mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: isDark.value ? 'dark' : 'default' });
    const result = await mermaid.render(`diagram-${Math.random().toString(36).slice(2)}`, props.code);
    if (current === generation) { svg.value = result.svg; failed.value = false; }
  } catch {
    if (current === generation) { svg.value = ''; failed.value = true; }
  }
}
onMounted(() => { mounted = true; render(); });
watch([() => props.code, isDark], render);
</script>
<template>
  <figure class="diagram" aria-label="Architecture diagram">
    <div v-if="svg" v-html="svg" />
    <p v-if="failed" role="status">Diagram unavailable offline. The diagram source is shown below.</p>
    <details :open="!svg"><summary>Diagram source</summary><pre>{{ code }}</pre></details>
  </figure>
</template>
